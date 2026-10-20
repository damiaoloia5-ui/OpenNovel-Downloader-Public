use super::{Catalog, ChapterRef, SourceAdapter};
use crate::{Error, ImportChapter, ImportDocument, Result};
use reqwest::{Url, blocking::Client};
use scraper::{Html, Selector};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    io::Read,
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

const FONT_URL: &str = "https://lf6-awef.bytetos.com/obj/awesome-font/c/dc027189e0ba4cd.woff";
const HOSTS: &[&str] = &[
    "fanqienovel.com",
    "www.fanqienovel.com",
    "lf6-awef.bytetos.com",
    "lf3-awef.bytetos.com",
];

pub struct Fanqie {
    client: Client,
    next_request: Mutex<Instant>,
    font_verified: OnceLock<bool>,
}

impl Fanqie {
    pub fn new() -> Result<Self> {
        let client = Client::builder()
            .user_agent("OpenNovelDownloader/0.0.1 (public web reader)")
            .connect_timeout(Duration::from_secs(8))
            .timeout(Duration::from_secs(20))
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                if attempt.previous().len() >= 4 || !allowed(attempt.url()) {
                    attempt.stop()
                } else {
                    attempt.follow()
                }
            }))
            .build()
            .map_err(|_| Error::Network("无法初始化 HTTPS 客户端".into()))?;
        Ok(Self {
            client,
            next_request: Mutex::new(Instant::now()),
            font_verified: OnceLock::new(),
        })
    }

    pub fn recognize(input: &str) -> Result<String> {
        let input = input.trim();
        if valid_id(input) {
            return Ok(input.to_owned());
        }
        let url = Url::parse(input)
            .map_err(|_| Error::Invalid("请输入番茄作品详情页链接或作品 ID".into()))?;
        if !matches!(
            url.host_str(),
            Some("fanqienovel.com" | "www.fanqienovel.com")
        ) || !matches!(url.scheme(), "http" | "https")
            || !url.username().is_empty()
            || url.password().is_some()
            || url.port().is_some()
        {
            return Err(Error::Invalid(
                "只支持番茄官网作品详情页，不接受其他域名、账户信息或端口".into(),
            ));
        }
        let parts: Vec<_> = url.path().trim_end_matches('/').split('/').collect();
        if parts.len() != 3 || parts[1] != "page" || !valid_id(parts[2]) {
            return Err(Error::Invalid("请使用 /page/作品ID 详情页链接".into()));
        }
        Ok(parts[2].to_owned())
    }

    pub fn inspect(&self, input: &str) -> Result<Catalog> {
        let id = Self::recognize(input)?;
        let bytes = self.get(
            &format!("https://fanqienovel.com/page/{id}"),
            &AtomicBool::new(false),
            8 * 1024 * 1024,
        )?;
        let html = String::from_utf8(bytes)
            .map_err(|_| Error::SourceChanged("作品页不是 UTF-8 文本".into()))?;
        Self::parse_catalog(&html, &id)
    }

    fn get(&self, url: &str, cancelled: &AtomicBool, maximum: usize) -> Result<Vec<u8>> {
        let parsed = Url::parse(url).map_err(|_| Error::Invalid("来源地址无效".into()))?;
        if !allowed(&parsed) {
            return Err(Error::Invalid("来源地址超出允许范围".into()));
        }
        {
            let mut next = self
                .next_request
                .lock()
                .map_err(|_| Error::Network("请求调度器不可用".into()))?;
            while Instant::now() < *next {
                if cancelled.load(Ordering::Acquire) {
                    return Err(Error::Cancelled);
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            if cancelled.load(Ordering::Acquire) {
                return Err(Error::Cancelled);
            }
            *next = Instant::now() + Duration::from_secs(1);
        }
        let response = self
            .client
            .get(parsed)
            .send()
            .map_err(|_| Error::Network("连接、TLS 或读取超时，请稍后重试".into()))?;
        let status = response.status();
        if status.as_u16() == 429 {
            let seconds = response
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(60);
            if let Ok(mut next) = self.next_request.lock() {
                *next = Instant::now() + Duration::from_secs(seconds.min(86400));
            }
            return Err(Error::Network(format!(
                "来源限流，请至少等待 {} 秒再重试",
                seconds.min(86400)
            )));
        }
        if matches!(status.as_u16(), 401 | 403) {
            return Err(Error::Restricted(
                "官网拒绝访问或需要验证，不进行绕过".into(),
            ));
        }
        if !status.is_success() {
            return Err(Error::Network(format!("官网返回 HTTP {}", status.as_u16())));
        }
        let mut bytes = Vec::new();
        response
            .take((maximum + 1) as u64)
            .read_to_end(&mut bytes)?;
        if bytes.len() > maximum {
            return Err(Error::SourceChanged("响应超过大小限制".into()));
        }
        if cancelled.load(Ordering::Acquire) {
            return Err(Error::Cancelled);
        }
        Ok(bytes)
    }

    pub fn parse_catalog(html: &str, expected_id: &str) -> Result<Catalog> {
        let state = initial_state(html)?;
        let page = &state["page"];
        if text(page, "bookId")? != expected_id {
            return Err(Error::SourceChanged("作品 ID 与请求不一致".into()));
        }
        let volumes = page["chapterListWithVolume"]
            .as_array()
            .ok_or_else(|| changed("目录字段缺失"))?;
        let mut chapters = Vec::new();
        for volume in volumes {
            for item in volume.as_array().ok_or_else(|| changed("分卷结构变化"))? {
                let id = text(item, "itemId")?;
                if !valid_id(&id) {
                    return Err(changed("章节 ID 无效"));
                }
                chapters.push(ChapterRef {
                    id,
                    title: text(item, "title")?,
                    restricted: access_restricted(item)?,
                });
            }
        }
        let item_ids = page["itemIds"]
            .as_array()
            .ok_or_else(|| changed("目录索引缺失"))?;
        let indexed: std::collections::HashSet<_> =
            item_ids.iter().filter_map(Value::as_str).collect();
        // The site uses a separate (often reverse) order for itemIds. Reading order comes from volumes.
        if item_ids.len() != chapters.len()
            || indexed.len() != chapters.len()
            || !chapters
                .iter()
                .all(|chapter| indexed.contains(chapter.id.as_str()))
        {
            return Err(changed("目录列表与索引不一致，不能确定完整目录"));
        }
        let catalog = Catalog {
            source: "fanqie-web".into(),
            remote_id: expected_id.into(),
            title: text(page, "bookName")?,
            author: page["author"].as_str().unwrap_or_default().to_owned(),
            chapters,
        };
        catalog.validate()?;
        Ok(catalog)
    }

    pub fn parse_chapter(
        html: &str,
        expected_book: &str,
        expected_chapter: &str,
    ) -> Result<ImportChapter> {
        let state = initial_state(html)?;
        let chapter = &state["reader"]["chapterData"];
        if text(chapter, "bookId")? != expected_book || text(chapter, "itemId")? != expected_chapter
        {
            return Err(changed("正文身份与目录不一致"));
        }
        if access_restricted(chapter)? {
            return Err(Error::Restricted(
                "官网网页未开放该章完整内容，预览不保存为正文".into(),
            ));
        }
        if !html.contains("/dc027189e0ba4cd.woff") {
            return Err(changed("字体版本变化，暂停保存以避免乱码"));
        }
        let body = text(chapter, "content")?;
        let mapping = font_map();
        let mut decoded = String::new();
        for character in body.chars() {
            if ('\u{e000}'..='\u{f8ff}').contains(&character) {
                decoded.push(
                    *mapping
                        .get(&(character as u32))
                        .ok_or_else(|| changed("遇到未识别的字形编码"))?,
                );
            } else {
                decoded.push(character);
            }
        }
        let document = Html::parse_fragment(&decoded);
        let paragraphs: Vec<_> = document
            .select(&Selector::parse("p").expect("constant selector"))
            .map(|p| p.text().collect::<String>().trim().to_owned())
            .filter(|p| !p.is_empty())
            .collect();
        let declared = chapter["chapterWordNumber"]
            .as_str()
            .and_then(|v| v.parse::<usize>().ok())
            .ok_or_else(|| changed("正文字数字段变化"))?;
        let actual = paragraphs.iter().map(|p| p.chars().count()).sum::<usize>();
        if declared == 0
            || (actual as u128) * 100 < (declared as u128) * 80
            || (actual as u128) * 100 > (declared as u128) * 150
        {
            return Err(changed("返回正文与声明长度不符，疑似截断或提示页"));
        }
        let output = ImportChapter {
            id: expected_chapter.into(),
            title: text(chapter, "title")?,
            paragraphs,
        };
        let validate = ImportDocument {
            id: expected_book.into(),
            title: "正文校验".into(),
            author: String::new(),
            chapters: vec![output],
        };
        Ok(ImportDocument::parse(&serde_json::to_string(&validate)?)?
            .chapters
            .remove(0))
    }
}

impl SourceAdapter for Fanqie {
    fn source_id(&self) -> &str {
        "fanqie-web"
    }
    fn fetch_chapter(
        &self,
        book_id: &str,
        chapter: &ChapterRef,
        cancelled: &AtomicBool,
    ) -> Result<ImportChapter> {
        if chapter.restricted {
            return Err(Error::Restricted("官网目录标记该章网页受限".into()));
        }
        if !valid_id(book_id) || !valid_id(&chapter.id) {
            return Err(changed("章节身份无效"));
        }
        if self.font_verified.get() != Some(&true) {
            let bytes = self.get(FONT_URL, cancelled, 512 * 1024)?;
            let expected = font_data()["font_sha256"]
                .as_str()
                .expect("embedded font digest");
            if format!("{:x}", Sha256::digest(&bytes)) != expected {
                return Err(changed("官网字体校验值变化"));
            }
            let _ = self.font_verified.set(true);
        }
        let bytes = self.get(
            &format!("https://fanqienovel.com/reader/{}", chapter.id),
            cancelled,
            2 * 1024 * 1024,
        )?;
        let html = String::from_utf8(bytes).map_err(|_| changed("正文页不是 UTF-8 文本"))?;
        Self::parse_chapter(&html, book_id, &chapter.id)
    }
}

fn valid_id(value: &str) -> bool {
    (10..=30).contains(&value.len()) && value.bytes().all(|b| b.is_ascii_digit())
}
fn allowed(url: &Url) -> bool {
    url.scheme() == "https"
        && url.port().is_none()
        && url.host_str().is_some_and(|host| HOSTS.contains(&host))
        && url.username().is_empty()
        && url.password().is_none()
}
fn changed(message: &str) -> Error {
    Error::SourceChanged(message.into())
}
fn text(value: &Value, key: &str) -> Result<String> {
    value[key]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| changed(&format!("字段 {key} 缺失或类型变化")))
}
fn access_restricted(value: &Value) -> Result<bool> {
    let locked = value["isChapterLock"]
        .as_bool()
        .ok_or_else(|| changed("章节开放状态缺失"))?;
    let pay = value["needPay"]
        .as_i64()
        .ok_or_else(|| changed("章节付费状态缺失"))?;
    Ok(locked || pay != 0 || value["isPaidPublication"] == true || value["isPaidStory"] == true)
}
fn font_data() -> &'static Value {
    static DATA: OnceLock<Value> = OnceLock::new();
    DATA.get_or_init(|| {
        serde_json::from_str(include_str!("fanqie_font.json")).expect("embedded font data")
    })
}
fn font_map() -> &'static HashMap<u32, char> {
    static MAP: OnceLock<HashMap<u32, char>> = OnceLock::new();
    MAP.get_or_init(|| {
        font_data()["mapping"]
            .as_object()
            .expect("embedded map")
            .iter()
            .map(|(a, b)| {
                (
                    a.parse().expect("codepoint"),
                    char::from_u32(b.as_u64().expect("unicode") as u32).expect("unicode scalar"),
                )
            })
            .collect()
    })
}

fn initial_state(html: &str) -> Result<Value> {
    let marker = "window.__INITIAL_STATE__";
    let start = html
        .find(marker)
        .ok_or_else(|| changed("官网页面未包含阅读数据，可能需要验证"))?;
    let raw = html[start + marker.len()..]
        .trim_start()
        .strip_prefix('=')
        .ok_or_else(|| changed("页面数据结构变化"))?
        .trim_start();
    if !raw.starts_with('{') {
        return Err(changed("阅读数据不是预期对象"));
    }
    let chars: Vec<char> = raw.chars().collect();
    let (mut quoted, mut escaped, mut index, mut depth) = (false, false, 0, 0i32);
    let mut normalized = String::new();
    while index < chars.len() {
        let ch = chars[index];
        if !quoted && ch.is_ascii_alphabetic() {
            let begin = index;
            while index < chars.len()
                && (chars[index].is_ascii_alphanumeric() || chars[index] == '_')
            {
                index += 1;
            }
            let token: String = chars[begin..index].iter().collect();
            normalized.push_str(if token == "undefined" { "null" } else { &token });
            continue;
        }
        normalized.push(ch);
        if quoted {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                quoted = false;
            }
        } else {
            match ch {
                '"' => quoted = true,
                '{' | '[' => depth += 1,
                '}' | ']' => depth -= 1,
                _ => {}
            }
        }
        index += 1;
        if depth == 0 {
            break;
        }
    }
    Value::deserialize(&mut serde_json::Deserializer::from_str(&normalized))
        .map_err(|_| changed("官网数据无法解析"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const BOOK: &str = "10000000001";
    const CHAPTER: &str = "10000000002";

    fn page(value: Value) -> String {
        format!("<link href='{FONT_URL}'><script>window.__INITIAL_STATE__ = {value};</script>")
    }

    fn chapter() -> Value {
        json!({"reader":{"chapterData":{
            "bookId":BOOK,"itemId":CHAPTER,"title":"原创测试章",
            "needPay":0,"isChapterLock":false,"chapterWordNumber":"12",
            "content":"<p>山间的小屋亮起一盏灯。</p>"
        }}})
    }

    #[test]
    fn links_are_canonical_and_never_accept_other_hosts_or_credentials() {
        for input in [
            BOOK.to_string(),
            format!("https://fanqienovel.com/page/{BOOK}?a=b"),
            format!("http://www.fanqienovel.com/page/{BOOK}/"),
        ] {
            assert_eq!(Fanqie::recognize(&input).unwrap(), BOOK);
        }
        for input in [
            format!("https://fanqienovel.com.evil.test/page/{BOOK}"),
            format!("https://user:secret@fanqienovel.com/page/{BOOK}"),
            format!("https://fanqienovel.com:8080/page/{BOOK}"),
            format!("https://fanqienovel.com/reader/{BOOK}"),
            "file:///C:/private".into(),
            "javascript:alert(1)".into(),
        ] {
            assert!(Fanqie::recognize(&input).is_err());
        }
        assert!(!allowed(&Url::parse("http://fanqienovel.com").unwrap()));
        assert!(!allowed(&Url::parse("https://127.0.0.1/").unwrap()));
    }

    #[test]
    fn state_parser_preserves_strings_and_never_executes_javascript() {
        let state=initial_state(r#"window.__INITIAL_STATE__ = {"a":undefined,"b":"undefined } \\\"","c":[{"x":true}]}; malicious()"#).unwrap();
        assert!(state["a"].is_null());
        assert!(state["b"].as_str().unwrap().starts_with("undefined }"));
        assert_eq!(state["c"][0]["x"], true);
        assert!(initial_state("window.__INITIAL_STATE__ = malicious()").is_err());
    }

    #[test]
    fn catalog_requires_matching_index_and_explicit_access_fields() {
        let mut value = json!({"page":{"bookId":BOOK,"bookName":"原创","author":"测试","chapterListWithVolume":[[{"itemId":CHAPTER,"title":"第一章","needPay":0,"isChapterLock":false}]],"itemIds":[CHAPTER]}});
        assert_eq!(
            Fanqie::parse_catalog(&page(value.clone()), BOOK)
                .unwrap()
                .chapters
                .len(),
            1
        );
        let mut reverse = value.clone();
        reverse["page"]["chapterListWithVolume"][0]
            .as_array_mut()
            .unwrap()
            .push(
                json!({"itemId":"10000000003","title":"第二章","needPay":0,"isChapterLock":false}),
            );
        reverse["page"]["itemIds"] = json!(["10000000003", CHAPTER]);
        let parsed = Fanqie::parse_catalog(&page(reverse), BOOK).unwrap();
        assert_eq!(
            parsed
                .chapters
                .iter()
                .map(|c| c.id.as_str())
                .collect::<Vec<_>>(),
            [CHAPTER, "10000000003"]
        );
        value["page"]["itemIds"] = json!(["99999999999"]);
        assert!(Fanqie::parse_catalog(&page(value.clone()), BOOK).is_err());
        value["page"]["itemIds"] = json!([CHAPTER]);
        value["page"]["chapterListWithVolume"][0][0]
            .as_object_mut()
            .unwrap()
            .remove("isChapterLock");
        assert!(Fanqie::parse_catalog(&page(value), BOOK).is_err());
    }

    #[test]
    fn chapter_decodes_known_glyphs_and_rejects_partial_or_changed_content() {
        let mut value = chapter();
        let original = "山间的小屋亮起一盏灯。";
        let encoded: String = original
            .chars()
            .map(|c| {
                font_map()
                    .iter()
                    .find(|(_, v)| **v == c)
                    .map(|(k, _)| char::from_u32(*k).unwrap())
                    .unwrap_or(c)
            })
            .collect();
        value["reader"]["chapterData"]["content"] = json!(format!("<p>{encoded}</p>"));
        assert_eq!(
            Fanqie::parse_chapter(&page(value.clone()), BOOK, CHAPTER)
                .unwrap()
                .paragraphs,
            [original]
        );
        assert!(Fanqie::parse_chapter(&page(value.clone()), "99999999999", CHAPTER).is_err());
        value["reader"]["chapterData"]["isChapterLock"] = json!(true);
        assert!(matches!(
            Fanqie::parse_chapter(&page(value.clone()), BOOK, CHAPTER),
            Err(Error::Restricted(_))
        ));
        value["reader"]["chapterData"]["isChapterLock"] = json!(false);
        value["reader"]["chapterData"]["chapterWordNumber"] = json!("1000");
        assert!(Fanqie::parse_chapter(&page(value.clone()), BOOK, CHAPTER).is_err());
        value["reader"]["chapterData"]["chapterWordNumber"] = json!(usize::MAX.to_string());
        assert!(Fanqie::parse_chapter(&page(value), BOOK, CHAPTER).is_err());
        assert!(
            Fanqie::parse_chapter(
                &page(chapter()).replace("dc027189e0ba4cd", "changed"),
                BOOK,
                CHAPTER
            )
            .is_err()
        );
        let mut unknown = chapter();
        unknown["reader"]["chapterData"]["content"] =
            json!("<p>\u{f8ff}山间的小屋亮起一盏灯。</p>");
        assert!(Fanqie::parse_chapter(&page(unknown), BOOK, CHAPTER).is_err());
    }
}
