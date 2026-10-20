use super::{
    Catalog, ChapterRef,
    page::{self, changed},
    web::WebClient,
};
use crate::{Error, Result};
use reqwest::Url;
use scraper::Html;
use serde_json::Value;
use std::sync::atomic::AtomicBool;

pub struct Shuqi {
    http: WebClient,
}
impl Shuqi {
    pub fn new() -> Result<Self> {
        Ok(Self {
            http: WebClient::new(&["www.shuqi.com", "shuqi.com"])?,
        })
    }
    pub fn inspect(&self, book: &str) -> Result<Catalog> {
        if !page::numeric(book) {
            return Err(Error::Invalid("书旗作品 ID 无效".into()));
        }
        let stop = AtomicBool::new(false);
        let html = self.http.text(
            &format!("https://www.shuqi.com/book/{book}.html"),
            &stop,
            8 * 1024 * 1024,
        )?;
        let chapters = self.http.text(
            &format!("https://www.shuqi.com/chapter?bid={book}"),
            &stop,
            8 * 1024 * 1024,
        )?;
        Self::parse_catalog(&html, &chapters, book)
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
