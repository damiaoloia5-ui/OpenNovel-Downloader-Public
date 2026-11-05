//! Official mobile catalog and explicitly free, paginated chapter responses.
use super::{Catalog, ChapterRef, SourceAdapter, page, web::WebClient};
use crate::{Error, ImportChapter, ImportDocument, Result};
use reqwest::Url;
use scraper::Html;
use serde_json::Value;
use std::sync::atomic::AtomicBool;

const ORIGIN: &str = "https://n.cmread.com";
const CONTENT_PATH: &str = "/nap/f/ms.content.wechatService/wechatForChapterContentAction.json";

pub struct Migu {
    http: WebClient,
}

fn field<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value[key]
        .as_str()
        .ok_or_else(|| page::changed("咪咕响应必需字段缺失"))
}
fn number(value: &Value) -> Result<usize> {
    let n = value
        .as_u64()
        .and_then(|n| usize::try_from(n).ok())
        .or_else(|| value.as_str().and_then(|s| s.parse().ok()))
        .ok_or_else(|| page::changed("咪咕数量字段无效"))?;
    Ok(n)
}

fn hidden_style(style: &str) -> bool {
    style
        .split(';')
        .filter_map(|rule| rule.split_once(':'))
        .any(|(key, value)| {
            let value = value.trim().trim_end_matches("!important").trim();
            match key.trim() {
                "display" => value == "none",
                "visibility" => matches!(value, "hidden" | "collapse"),
                "opacity" | "font-size" => {
                    value
                        .trim_end_matches(|c: char| c.is_ascii_alphabetic() || c == '%')
                        .parse::<f32>()
                        == Ok(0.0)
                }
                _ => false,
            }
        })
}
fn component<'a>(value: &'a Value, name: &str) -> Result<&'a Value> {
    let object = value
        .as_object()
        .ok_or_else(|| page::changed("咪咕页面数据无效"))?;
    let mut found = object.values().filter(|v| v["pluginCode"] == name);
    let result = found
        .next()
        .ok_or_else(|| page::changed("咪咕页面组件缺失"))?;
    if found.next().is_some() || result["status"] != "200" || result["isvisable"] != "1" {
        return Err(page::changed("咪咕页面组件异常"));
    }
    Ok(&result["data"])
}

impl Migu {
    pub fn new() -> Result<Self> {
        Ok(Self {
            http: WebClient::new(&["n.cmread.com"])?,
        })
    }

    pub fn page_data(html: &str) -> Result<Value> {
        let (_, tail) = html
            .split_once("var pageData")
            .ok_or_else(|| page::changed("咪咕页面数据缺失"))?;
        let tail = tail
            .trim_start()
            .strip_prefix('=')
            .ok_or_else(|| page::changed("咪咕页面数据声明变化"))?;
        serde_json::Deserializer::from_str(tail.trim_start())
            .into_iter::<Value>()
            .next()
            .ok_or_else(|| page::changed("咪咕页面数据缺失"))?
            .map_err(|_| page::changed("咪咕页面数据不是 JSON"))
    }

    pub fn metadata(value: &Value, book: &str) -> Result<(Catalog, usize)> {
        if !page::numeric(book) {
            return Err(Error::Invalid("咪咕作品 ID 无效".into()));
        }
        let details = component(value, "wap_bk_content_book")?;
        let stats = component(value, "wap_bk_data_book")?;
        let intro = component(value, "wap_bk_brief_book")?;
        let reading = component(value, "wap_reading_page")?;
        if details["bid"] != book || details["bookIds"] != book || reading["bid"] != book {
            return Err(page::changed("咪咕目录作品身份不符"));
        }
        let total = number(&stats["totalChapterNum"])?;
        if total == 0
            || total > crate::MAX_CHAPTERS
            || number(&reading["chapterList"]["totalRecordCount"])? != total
        {
            return Err(page::changed("咪咕目录总数无效或不一致"));
        }
        Ok((
            Catalog {
                source: "migu-web".into(),
                remote_id: book.into(),
                title: field(details, "bookName")?.into(),
                author: field(details, "bookAuthor")?.into(),
                description: field(intro, "bookDesc")?.into(),
                chapters: Vec::new(),
            },
            total,
        ))
    }

    pub fn append_catalog(
        catalog: &mut Catalog,
        block: &Value,
        total: usize,
    ) -> Result<Option<String>> {
        if number(&block["totalRecordCount"])? != total {
            return Err(page::changed("咪咕分页目录总数变化"));
        }
        let before = catalog.chapters.len();
        let volumes = block["volumeInfo"]
            .as_array()
            .ok_or_else(|| page::changed("咪咕分卷目录缺失"))?;
        for volume in volumes {
            for row in volume["chapterInfo"]
                .as_array()
                .ok_or_else(|| page::changed("咪咕章节列表缺失"))?
            {
                let id = field(row, "chapterId")?;
                if !page::numeric(id) || number(&row["chapterSort"])? != catalog.chapters.len() + 1
                {
                    return Err(page::changed("咪咕章节身份或顺序无效"));
                }
                let url = Url::parse(field(row, "chapterUrl")?)
                    .map_err(|_| page::changed("咪咕章节地址无效"))?;
                if !super::web::allowed(&url, &["n.cmread.com"])
                    || url.path() != format!("/nap/{}/{id}.htm", catalog.remote_id)
                {
                    return Err(page::changed("咪咕章节地址与作品身份不符"));
                }
                let fee = number(&row["feeStatus"])?;
                // The official preview selects feeStatus == 0. All other
                // declared states remain unavailable without guessing their meaning.
                let hidden = number(&row["isContentHide"])? != 0
                    || number(&row["isHide"])? != 0
                    || number(&row["onlyContentHide"])? != 0
                    || number(&row["otherHidden"])? != 0;
                let ahead = row["isReadAhead"]
                    .as_bool()
                    .ok_or_else(|| page::changed("咪咕提前阅读标记未知"))?;
                catalog.chapters.push(ChapterRef {
                    id: id.into(),
                    title: field(row, "chapterName")?.into(),
                    restricted: fee != 0 || hidden || ahead,
                });
            }
        }
        if catalog.chapters.len() == before || catalog.chapters.len() > total {
            return Err(page::changed("咪咕目录分页为空或越界"));
        }
        let next = field(block, "nextPageUrl")?;
        if next.is_empty() {
            if catalog.chapters.len() != total {
                return Err(page::changed("咪咕目录缺少末尾章节"));
            }
            catalog.validate()?;
            return Ok(None);
        }
        if catalog.chapters.len() == total {
            return Err(page::changed("咪咕末页仍声明下一页"));
        }
        let url = Url::parse(ORIGIN)
            .expect("fixed origin")
            .join(next)
            .map_err(|_| page::changed("咪咕目录分页地址无效"))?;
        if !super::web::allowed(&url, &["n.cmread.com"]) || url.path() != "/nap/p/catalogForWap.jsp"
        {
            return Err(page::changed("咪咕目录分页地址越界"));
        }
        for (key, expected) in [
            ("bid", catalog.remote_id.clone()),
            ("start", (catalog.chapters.len() + 1).to_string()),
            ("count", "10".into()),
            ("order", "0".into()),
        ] {
            let values: Vec<_> = url
                .query_pairs()
                .filter(|(k, _)| k == key)
                .map(|(_, v)| v.into_owned())
                .collect();
            if values != vec![expected] {
                return Err(page::changed("咪咕目录分页身份或位置不符"));
            }
        }
        // Reconstruct only reading parameters, discarding tracking and referral data.
        Ok(Some(format!(
            "{ORIGIN}/nap/p/catalogForWap.jsp?bid={}&start={}&count=10&order=0&vt=9",
            catalog.remote_id,
            catalog.chapters.len() + 1
        )))
    }

    pub fn inspect(&self, book: &str) -> Result<Catalog> {
        if !page::numeric(book) {
            return Err(Error::Invalid("咪咕作品 ID 无效".into()));
        }
        let stop = AtomicBool::new(false);
        let raw = self.http.text(
            &format!("{ORIGIN}/nap/p/wap_detail_new.jsp?bid={book}"),
            &stop,
            3 * 1024 * 1024,
        )?;
        let data = Self::page_data(&raw)?;
        let (mut catalog, total) = Self::metadata(&data, book)?;
        let mut next = Self::append_catalog(
            &mut catalog,
            &component(&data, "wap_reading_page")?["chapterList"],
            total,
        )?;
        while let Some(url) = next {
            let raw = self.http.text(&url, &stop, 3 * 1024 * 1024)?;
            let value: Value =
                serde_json::from_str(&raw).map_err(|_| page::changed("咪咕分页目录不是 JSON"))?;
            next = Self::append_catalog(
                &mut catalog,
                &component(&value, "bkdetail_catalog")?["chapter"],
                total,
            )?;
        }
        Ok(catalog)
    }

    pub fn parse_content(
        raw: &str,
        book: &str,
        chapter: &ChapterRef,
        expected_page: usize,
    ) -> Result<(Vec<String>, usize)> {
        if chapter.restricted {
            return Err(Error::Restricted("咪咕目录未明确开放本章".into()));
        }
        let value: Value =
            serde_json::from_str(raw).map_err(|_| page::changed("咪咕正文不是 JSON"))?;
        let content = &value["chapterContent"];
        if value["status"] != "200" || value["isvisable"] != "1" {
            return Err(Error::Restricted("咪咕正文服务未开放本章".into()));
        }
        if !field(content, "payUrl")?.is_empty()
            || number(&content["isSDKSub"])? != 0
            || number(&content["onlyContentHide"])? != 0
            || number(&content["otherHidden"])? != 0
        {
            return Err(Error::Restricted("咪咕本章需要权益或存在隐藏内容".into()));
        }
        let total = number(&content["totalPage"])?;
        if total == 0
            || total > 100
            || number(&content["currPage"])? != expected_page
            || expected_page > total
        {
            return Err(page::changed("咪咕正文分页不完整"));
        }
        let action = Url::parse(ORIGIN)
            .expect("fixed origin")
            .join(field(content, "pageActionUrl")?)
            .map_err(|_| page::changed("咪咕正文分页地址无效"))?;
        if !super::web::allowed(&action, &["n.cmread.com"]) || action.path() != CONTENT_PATH {
            return Err(page::changed("咪咕正文来源不符"));
        }
        for (key, id) in [("bid", book), ("cid", chapter.id.as_str())] {
            let ids: Vec<_> = action
                .query_pairs()
                .filter(|(k, _)| k == key)
                .map(|(_, v)| v.into_owned())
                .collect();
            if ids != vec![id] {
                return Err(page::changed("咪咕正文作品或章节身份不符"));
            }
        }
        let html = Html::parse_fragment(field(content, "chapterContent")?);
        for element in html.select(&page::selector("*")) {
            if !["html", "p", "span", "br", "b", "strong", "i", "em"]
                .contains(&element.value().name())
            {
                return Err(page::changed("咪咕正文包含未知或图片结构"));
            }
            let style = element
                .value()
                .attr("style")
                .unwrap_or("")
                .replace(' ', "")
                .to_lowercase();
            if element.value().attr("hidden").is_some()
                || element.value().attr("aria-hidden") == Some("true")
                || hidden_style(&style)
            {
                return Err(page::changed("咪咕正文含隐藏段落"));
            }
        }
        let paragraphs: Vec<String> = html
            .select(&page::selector("p"))
            .map(|p| p.text().collect::<String>().trim().to_owned())
            .filter(|p| !p.is_empty())
            .collect();
        let all_text: String = html.root_element().text().collect();
        let selected_text = paragraphs.join("");
        if all_text
            .chars()
            .filter(|c| !c.is_whitespace())
            .ne(selected_text.chars().filter(|c| !c.is_whitespace()))
        {
            return Err(page::changed("咪咕正文包含段落之外的文字，未保存"));
        }
        if paragraphs.is_empty()
            || paragraphs.iter().any(|p| {
                p.chars()
                    .any(|c| matches!(c, '\u{e000}'..='\u{f8ff}' | '\u{fffd}'))
                    || ["订阅本章", "购买本章", "试读结束", "解锁本章"]
                        .iter()
                        .any(|s| p.contains(s))
            })
        {
            return Err(Error::Restricted("咪咕正文为空或含限制提示".into()));
        }
        Ok((paragraphs, total))
    }
}

impl SourceAdapter for Migu {
    fn source_id(&self) -> &str {
        "migu-web"
    }
    fn fetch_chapter(
        &self,
        book: &str,
        chapter: &ChapterRef,
        cancelled: &AtomicBool,
    ) -> Result<ImportChapter> {
        if !page::numeric(book) || !page::numeric(&chapter.id) {
            return Err(Error::Invalid("咪咕章节身份无效".into()));
        }
        if chapter.restricted {
            return Err(Error::Restricted("咪咕目录未明确开放本章".into()));
        }
        let mut paragraphs = Vec::new();
        let mut total = None;
        let mut size = 0;
        for current in 1..=100 {
            let url = format!(
                "{ORIGIN}{CONTENT_PATH}?bid={book}&cid={}&pageNo={current}",
                chapter.id
            );
            let raw = self.http.text(&url, cancelled, 2 * 1024 * 1024)?;
            size += raw.len();
            if size > crate::MAX_CAPTURE_BYTES {
                return Err(page::changed("咪咕整章超过资源预算"));
            }
            let (part, pages) = Self::parse_content(&raw, book, chapter, current)?;
            if total.is_some_and(|expected| expected != pages) {
                return Err(page::changed("咪咕正文分页数量变化"));
            }
            total = Some(pages);
            paragraphs.extend(part);
            if current == pages {
                break;
            }
        }
        let mut validated = ImportDocument {
            id: book.into(),
            title: "正文校验".into(),
            author: String::new(),
            chapters: vec![ImportChapter {
                id: chapter.id.clone(),
                title: chapter.title.clone(),
                paragraphs,
            }],
        }
        .validated()?;
        Ok(validated.chapters.remove(0))
    }
}
