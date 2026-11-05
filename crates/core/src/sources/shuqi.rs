use super::{
    Catalog, ChapterRef, SourceAdapter,
    page::{self, changed},
    web::WebClient,
};
use crate::{Error, ImportChapter, ImportDocument, Result};
use base64::{Engine, engine::general_purpose::STANDARD};
use reqwest::Url;
use scraper::Html;
use serde_json::Value;
use std::{
    collections::HashMap,
    sync::{Mutex, atomic::AtomicBool},
    time::{Duration, Instant},
};

#[derive(Clone)]
struct FreeChapter {
    url: String,
    words: usize,
    title: String,
}
struct ReaderCatalog {
    title: String,
    author: String,
    description: String,
    chapters: Vec<ChapterRef>,
    free: HashMap<String, FreeChapter>,
}

pub struct Shuqi {
    http: WebClient,
    content: WebClient,
    cached: Mutex<Option<(String, Instant, ReaderCatalog)>>,
}
impl Shuqi {
    pub fn new() -> Result<Self> {
        Ok(Self {
            http: WebClient::new(&["www.shuqi.com", "shuqi.com"])?,
            content: WebClient::new(&["c13.shuqireader.com"])?,
            cached: Mutex::new(None),
        })
    }
    pub fn inspect(&self, book: &str) -> Result<Catalog> {
        self.inspect_with_stop(book, &AtomicBool::new(false))
    }

    pub fn inspect_with_stop(&self, book: &str, stop: &AtomicBool) -> Result<Catalog> {
        if !page::numeric(book) {
            return Err(Error::Invalid("书旗作品 ID 无效".into()));
        }
        // The reader provides identities, permission and metadata in one
        // snapshot; the decorative directory embeds VIP badges in titles.
        let reader = self.reader(book, "", stop)?;
        let catalog = Catalog {
            source: "shuqi-web".into(),
            remote_id: book.into(),
            title: reader.title.clone(),
            author: reader.author.clone(),
            description: reader.description.clone(),
            chapters: reader.chapters.clone(),
        };
        catalog.validate()?;
        *self
            .cached
            .lock()
            .map_err(|_| changed("书旗目录缓存不可用"))? =
            Some((book.into(), Instant::now(), reader));
        Ok(catalog)
    }

    fn reader(&self, book: &str, chapter: &str, stop: &AtomicBool) -> Result<ReaderCatalog> {
        let url = if chapter.is_empty() {
            format!("https://www.shuqi.com/reader?bid={book}")
        } else {
            format!("https://www.shuqi.com/reader?bid={book}&cid={chapter}")
        };
        let html = self.http.text(&url, stop, 8 * 1024 * 1024)?;
        Self::parse_reader(&html, book)
    }

    fn parse_reader(html: &str, book: &str) -> Result<ReaderCatalog> {
        let doc = Html::parse_document(html);
        let value: Value = serde_json::from_str(&page::text(&doc, ".js-dataChapters")?)
            .map_err(|_| changed("书旗阅读目录不是 JSON"))?;
        if value["bookId"].as_str() != Some(book) || value["readIsOpen"] != true {
            return Err(changed("书旗阅读作品身份或开放状态不符"));
        }
        let volumes = value["chapterList"]
            .as_array()
            .ok_or_else(|| changed("书旗阅读目录缺失"))?;
        let mut result = ReaderCatalog {
            title: value["bookName"]
                .as_str()
                .filter(|s| !s.trim().is_empty())
                .ok_or_else(|| changed("书旗书名缺失"))?
                .into(),
            author: value["authorName"]
                .as_str()
                .ok_or_else(|| changed("书旗作者缺失"))?
                .into(),
            description: value["intro"].as_str().unwrap_or_default().into(),
            chapters: Vec::new(),
            free: HashMap::new(),
        };
        let mut seen = std::collections::HashSet::new();
        for volume in volumes {
            for item in volume["volumeList"]
                .as_array()
                .ok_or_else(|| changed("书旗分卷目录缺失"))?
            {
                let id = item["chapterId"]
                    .as_str()
                    .filter(|s| page::numeric(s))
                    .ok_or_else(|| changed("书旗阅读章节身份缺失"))?;
                let title = item["chapterName"]
                    .as_str()
                    .filter(|s| !s.trim().is_empty())
                    .ok_or_else(|| changed("书旗阅读章节标题缺失"))?;
                if !seen.insert(id)
                    || item["chapterOrdid"].as_u64() != Some(result.chapters.len() as u64 + 1)
                {
                    return Err(changed("书旗阅读章节重复或顺序不连续"));
                }
                let free = item["isFreeRead"] == true
                    && item["payStatus"] == "0"
                    && item["chapterPrice"].as_f64() == Some(0.0)
                    && item["shelf"] == 1
                    && item["vipPriorityRead"] == false
                    && item["chapterLockDesc"].is_null();
                if free {
                    if value["freeContUrlPrefix"]
                        != "https://c13.shuqireader.com/pcapi/chapter/contentfree/"
                    {
                        return Err(changed("书旗免费正文域名发生变化"));
                    }
                    let suffix = item["contUrlSuffix"]
                        .as_str()
                        .filter(|s| s.starts_with('?'))
                        .ok_or_else(|| changed("书旗免费正文地址缺失"))?;
                    let url = Url::parse(&format!(
                        "https://c13.shuqireader.com/pcapi/chapter/contentfree/{suffix}"
                    ))
                    .map_err(|_| changed("书旗正文地址无效"))?;
                    let pairs = url.query_pairs().collect::<Vec<_>>();
                    let keys = ["bookId", "chapterId", "ut", "num", "ver", "aut", "sign"];
                    if url.fragment().is_some()
                        || pairs.len() != keys.len()
                        || keys
                            .iter()
                            .any(|k| pairs.iter().filter(|(key, _)| key == k).count() != 1)
                        || pairs.iter().any(|(k, v)| match k.as_ref() {
                            "bookId" => v != book,
                            "chapterId" => v != id,
                            "num" | "ver" => v != "1",
                            "ut" => !page::numeric(v),
                            // Anonymous official free links may explicitly use aut=0.
                            "aut" => v != "0" && !page::numeric(v),
                            "sign" => v.len() != 32 || !v.bytes().all(|b| b.is_ascii_hexdigit()),
                            _ => true,
                        })
                    {
                        return Err(changed("书旗正文地址身份或参数不符"));
                    }
                    let words = item["wordCount"]
                        .as_u64()
                        .filter(|n| *n > 0 && *n <= 2_000_000)
                        .ok_or_else(|| changed("书旗声明字数无效"))?
                        as usize;
                    result.free.insert(
                        id.into(),
                        FreeChapter {
                            url: url.into(),
                            words,
                            title: title.into(),
                        },
                    );
                }
                result.chapters.push(ChapterRef {
                    id: id.into(),
                    title: title.into(),
                    restricted: !free,
                });
            }
        }
        if result.chapters.is_empty()
            || result.chapters.len() > crate::MAX_CHAPTERS
            || value["chapterNum"].as_u64() != Some(result.chapters.len() as u64)
        {
            return Err(changed("书旗阅读目录数量不符"));
        }
        Ok(result)
    }

    fn decode_body(raw: &str, chapter: &ChapterRef, words: usize) -> Result<ImportChapter> {
        let data: Value =
            serde_json::from_str(raw).map_err(|_| changed("书旗正文响应不是 JSON"))?;
        if data["state"] != "200" {
            return Err(changed("书旗正文服务未返回成功，未保存本章"));
        }
        let encoded = data["ChapterContent"]
            .as_str()
            .ok_or_else(|| changed("书旗正文缺失"))?;
        // Official anonymous free-reader format: ROT13 of Base64 UTF-8.
        // No keys, account credentials or charged-content endpoint are involved.
        let rotated: Vec<u8> = encoded
            .trim()
            .bytes()
            .map(|b| match b {
                b'a'..=b'z' => b'a' + (b - b'a' + 13) % 26,
                b'A'..=b'Z' => b'A' + (b - b'A' + 13) % 26,
                _ => b,
            })
            .collect();
        let bytes = STANDARD
            .decode(rotated)
            .map_err(|_| changed("书旗免费正文编码不符"))?;
        let text = String::from_utf8(bytes).map_err(|_| changed("书旗正文不是 UTF-8"))?;
        let text = text.replace("<br/>", "\n");
        if text.contains('<') || text.chars().any(|c| ('\u{e000}'..='\u{f8ff}').contains(&c)) {
            return Err(changed("书旗正文呈现结构未知"));
        }
        let paragraphs: Vec<String> = text
            .lines()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect();
        let count: usize = paragraphs
            .iter()
            .map(|s| s.chars().filter(|c| !c.is_whitespace()).count())
            .sum();
        if (count as u128) * 100 < (words as u128) * 80 {
            return Err(changed("书旗正文疑似截断，未保存本章"));
        }
        let doc = ImportDocument {
            id: "shuqi-validation".into(),
            title: "书旗正文校验".into(),
            author: String::new(),
            chapters: vec![ImportChapter {
                id: chapter.id.clone(),
                title: chapter.title.clone(),
                paragraphs,
            }],
        };
        Ok(ImportDocument::parse(&serde_json::to_string(&doc)?)?
            .chapters
            .remove(0))
    }
    pub fn parse_catalog(html: &str, chapters: &str, book: &str) -> Result<Catalog> {
        if !page::numeric(book) {
            return Err(changed("书旗作品 ID 无效"));
        }
        let meta = Html::parse_document(html);
        let directory = Html::parse_document(chapters);
        let json = page::text(&meta, ".js-pageData")?;
        let value: Value =
            serde_json::from_str(&json).map_err(|_| changed("书旗元数据不是 JSON"))?;
        if value["bookId"].as_str() != Some(book) {
            return Err(changed("书旗作品身份与请求不符"));
        }
        let title = value["bookName"]
            .as_str()
            .ok_or_else(|| changed("书旗书名缺失"))?
            .to_owned();
        let author = value["authorName"].as_str().unwrap_or_default().to_owned();
        let description = meta
            .select(&page::selector(".js-showDesc"))
            .next()
            .and_then(|e| e.value().attr("data-text"))
            .map(str::to_owned)
            .unwrap_or_else(|| {
                meta.select(&page::selector(".js-descContent"))
                    .next()
                    .map(|e| e.text().collect())
                    .unwrap_or_default()
            });
        let declared = meta
            .select(&page::selector(".titleline .title"))
            .find_map(|e| {
                let text = e.text().collect::<String>();
                text.contains("目录")
                    .then(|| text.split_once('共').map(|(_, s)| page::count(s)))
            })
            .flatten()
            .ok_or_else(|| changed("书旗目录总数缺失"))??;
        let base = Url::parse("https://www.shuqi.com/").expect("static URL");
        let mut items = Vec::new();
        for link in directory.select(&page::selector(".chapterul a[href]")) {
            let url = base
                .join(link.value().attr("href").unwrap_or_default())
                .map_err(|_| changed("书旗章节链接无效"))?;
            if !super::web::allowed(&url, &["www.shuqi.com"])
                || url.path() != "/reader"
                || url.fragment().is_some()
            {
                return Err(changed("书旗章节链接超出官网范围"));
            }
            let pairs = url.query_pairs().collect::<Vec<_>>();
            if pairs.len() != 2
                || pairs
                    .iter()
                    .filter(|(k, v)| k == "bid" && v == book)
                    .count()
                    != 1
            {
                return Err(changed("书旗章节作品身份不符"));
            }
            let id = pairs
                .iter()
                .find(|(k, _)| k == "cid")
                .map(|(_, v)| v.to_string())
                .filter(|s| page::numeric(s))
                .ok_or_else(|| changed("书旗章节身份缺失"))?;
            let label = link.text().collect::<String>();
            let (position, title) = label
                .trim()
                .split_once('.')
                .ok_or_else(|| changed("书旗章节序号缺失"))?;
            if position.parse::<usize>().ok() != Some(items.len() + 1) {
                return Err(changed("书旗章节顺序不连续"));
            }
            // This is directory metadata only. No affirmative chapter-reading permission is inferred.
            items.push(ChapterRef {
                id,
                title: title.trim().into(),
                restricted: false,
            });
        }
        if items.len() != declared {
            return Err(changed("书旗完整目录与声明数量不符"));
        }
        let catalog = Catalog {
            source: "shuqi-web".into(),
            remote_id: book.into(),
            title,
            author,
            description,
            chapters: items,
        };
        catalog.validate()?;
        Ok(catalog)
    }
}

impl SourceAdapter for Shuqi {
    fn source_id(&self) -> &str {
        "shuqi-web"
    }
    fn fetch_chapter(
        &self,
        book: &str,
        chapter: &ChapterRef,
        stop: &AtomicBool,
    ) -> Result<ImportChapter> {
        if chapter.restricted {
            return Err(Error::Restricted("书旗当前官网未开放本章免费正文".into()));
        }
        if !page::numeric(book) || !page::numeric(&chapter.id) {
            return Err(changed("书旗章节身份无效"));
        }
        let mut cached = self
            .cached
            .lock()
            .map_err(|_| changed("书旗目录缓存不可用"))?;
        if cached
            .as_ref()
            .is_none_or(|(id, time, _)| id != book || time.elapsed() > Duration::from_secs(120))
        {
            *cached = Some((
                book.into(),
                Instant::now(),
                self.reader(book, &chapter.id, stop)?,
            ));
        }
        let entry = cached
            .as_ref()
            .and_then(|(_, _, r)| r.free.get(&chapter.id))
            .cloned()
            .ok_or_else(|| Error::Restricted("书旗当前官网未开放本章免费正文".into()))?;
        drop(cached);
        if entry.title != chapter.title {
            return Err(changed("书旗正文标题与目录不同"));
        }
        let raw = self.content.text(&entry.url, stop, 2 * 1024 * 1024)?;
        Self::decode_body(&raw, chapter, entry.words)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn fixture() -> Value {
        json!({"bookId":"123","bookName":"原创作品","authorName":"测试作者","intro":"原创简介","readIsOpen":true,"chapterNum":1,"freeContUrlPrefix":"https://c13.shuqireader.com/pcapi/chapter/contentfree/","chapterList":[{"volumeList":[{"chapterId":"456","chapterName":"原创一章","chapterOrdid":1,"isFreeRead":true,"payStatus":"0","chapterPrice":0,"shelf":1,"vipPriorityRead":false,"chapterLockDesc":null,"wordCount":12,"contUrlSuffix":"?bookId=123&chapterId=456&ut=123&num=1&ver=1&aut=123&sign=00000000000000000000000000000000"}]}]})
    }
    fn parse(v: &Value) -> Result<ReaderCatalog> {
        Shuqi::parse_reader(&format!("<i class='js-dataChapters'>{v}</i>"), "123")
    }
    #[test]
    fn free_reader_validates_identity_order_and_explicit_permission() {
        let v = fixture();
        assert_eq!(parse(&v).unwrap().free.len(), 1);
        for (key, value) in [
            ("isFreeRead", json!(false)),
            ("payStatus", json!("3")),
            ("chapterPrice", json!(1)),
            ("shelf", json!(0)),
            ("vipPriorityRead", json!(true)),
            ("chapterLockDesc", json!("锁定")),
        ] {
            let mut x = v.clone();
            x["chapterList"][0]["volumeList"][0][key] = value;
            let parsed = parse(&x).unwrap();
            assert!(parsed.free.is_empty());
            assert!(parsed.chapters[0].restricted);
        }
        for (key, value) in [
            ("bookId", json!("999")),
            ("chapterNum", json!(2)),
            ("freeContUrlPrefix", json!("https://example.invalid/")),
        ] {
            let mut x = v.clone();
            x[key] = value;
            assert!(parse(&x).is_err());
        }
        for suffix in [
            "?bookId=999&chapterId=456&ut=123&num=1&ver=1&aut=123&sign=00000000000000000000000000000000",
            "//evil.test/",
            "?bookId=123#fragment",
        ] {
            let mut x = v.clone();
            x["chapterList"][0]["volumeList"][0]["contUrlSuffix"] = json!(suffix);
            assert!(parse(&x).is_err());
        }
    }
    #[test]
    fn anonymous_zero_aut_is_valid_but_identity_and_permission_remain_required() {
        let mut value = fixture();
        let suffix = value["chapterList"][0]["volumeList"][0]["contUrlSuffix"]
            .as_str()
            .unwrap()
            .replace("aut=123", "aut=0");
        value["chapterList"][0]["volumeList"][0]["contUrlSuffix"] = json!(suffix);
        assert_eq!(parse(&value).unwrap().free.len(), 1);
        for invalid in ["aut=-1", "aut=", "aut=0&aut=123", "aut=other"] {
            let mut bad = value.clone();
            bad["chapterList"][0]["volumeList"][0]["contUrlSuffix"] =
                json!(suffix.replace("aut=0", invalid));
            assert!(parse(&bad).is_err());
        }
        value["chapterList"][0]["volumeList"][0]["payStatus"] = json!("3");
        assert!(parse(&value).unwrap().free.is_empty());
    }
    #[test]
    fn free_response_decodes_original_text_and_rejects_short_or_error_data() {
        let text = "山间的小屋亮起一盏灯。<br/>旅人推开门。";
        let encoded = STANDARD.encode(text);
        let rot: String = encoded
            .bytes()
            .map(|b| match b {
                b'a'..=b'z' => b'a' + (b - b'a' + 13) % 26,
                b'A'..=b'Z' => b'A' + (b - b'A' + 13) % 26,
                _ => b,
            })
            .map(char::from)
            .collect();
        let data = json!({"state":"200","ChapterContent":rot});
        let c = ChapterRef {
            id: "456".into(),
            title: "原创一章".into(),
            restricted: false,
        };
        assert_eq!(
            Shuqi::decode_body(&data.to_string(), &c, 17)
                .unwrap()
                .paragraphs,
            text.split("<br/>").collect::<Vec<_>>()
        );
        assert!(Shuqi::decode_body(&data.to_string(), &c, 200).is_err());
        assert!(Shuqi::decode_body("{\"state\":\"403\"}", &c, 17).is_err());
    }
    #[test]
    #[ignore = "Explicit local snapshot validation; never network or account access"]
    fn manual_shuqi_snapshots() {
        let root = std::path::PathBuf::from(std::env::var_os("OPENNOVEL_SHUQI_SNAPSHOT").unwrap());
        let reader = Shuqi::parse_reader(
            &std::fs::read_to_string(root.join("shuqi-reader.html")).unwrap(),
            "157219",
        )
        .unwrap();
        for index in 0..2 {
            let c = &reader.chapters[index];
            let e = &reader.free[&c.id];
            let body = Shuqi::decode_body(
                &std::fs::read_to_string(root.join(format!("shuqi-free-{index}.json"))).unwrap(),
                c,
                e.words,
            )
            .unwrap();
            println!(
                "chapter={} paragraphs={} words={}",
                index + 1,
                body.paragraphs.len(),
                e.words
            );
        }
    }
}
