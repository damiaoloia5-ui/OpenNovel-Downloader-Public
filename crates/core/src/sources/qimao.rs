use super::{
    Catalog, ChapterRef, SourceAdapter,
    page::{self, changed},
    web::WebClient,
};
use crate::{Error, ImportChapter, ImportDocument, Result};
use scraper::Html;
use serde_json::Value;
use std::sync::atomic::AtomicBool;

pub struct Qimao {
    http: WebClient,
}
impl Qimao {
    pub fn new() -> Result<Self> {
        Ok(Self {
            // The public site currently sends obsolete folded security header lines.
            http: WebClient::with_obsolete_header_folding(&["www.qimao.com", "qimao.com"])?,
        })
    }
    pub fn inspect(&self, book: &str) -> Result<Catalog> {
        if !page::numeric(book) {
            return Err(Error::Invalid("七猫作品 ID 无效".into()));
        }
        let stop = AtomicBool::new(false);
        let html = self.http.text(
            &format!("https://www.qimao.com/shuku/{book}/"),
            &stop,
            8 * 1024 * 1024,
        )?;
        let json = self.http.text(
            &format!("https://www.qimao.com/api/book/chapter-list?book_id={book}"),
            &stop,
            8 * 1024 * 1024,
        )?;
        Self::parse_catalog(&html, &json, book)
    }

    pub fn parse_catalog(html: &str, json: &str, book: &str) -> Result<Catalog> {
        if !page::numeric(book) {
            return Err(changed("七猫作品 ID 无效"));
        }
        page::route_identity(html, &format!("/shuku/{book}/"))?;
        let document = Html::parse_document(html);
        let title = page::text(&document, ".book-information .title .txt")?;
        let author = page::text(&document, ".author-information .author-name a")?;
        let description = page::text(&document, ".book-introduction .intro")?;
        let total = document
            .select(&page::selector(".tab-inner"))
            .find_map(|e| {
                let text = e.text().collect::<String>();
                text.split_once("作品目录")
                    .map(|(_, count)| page::count(count))
            })
            .ok_or_else(|| changed("七猫目录声明数量缺失"))??;
        let value: Value = serde_json::from_str(json).map_err(|_| changed("七猫目录不是 JSON"))?;
        let list = value["data"]["chapters"]
            .as_array()
            .ok_or_else(|| changed("七猫目录数据缺失"))?;
        if list.len() != total {
            return Err(changed("七猫完整目录数量与作品页不符，请重新读取"));
        }
        let mut chapters = Vec::with_capacity(list.len());
        for (position, item) in list.iter().enumerate() {
            let id = item["id"]
                .as_str()
                .filter(|s| page::numeric(s))
                .ok_or_else(|| changed("七猫章节身份无效"))?;
            let title = item["title"]
                .as_str()
                .ok_or_else(|| changed("七猫章节标题缺失"))?;
            for field in ["index", "chapter_sort"] {
                if item[field].as_str().and_then(|s| s.parse::<usize>().ok()) != Some(position + 1)
                {
                    return Err(changed("七猫章节顺序不连续或发生变化"));
                }
            }
            let restricted = match item["is_vip"].as_str() {
                Some("0") => false,
                Some("1") => true,
                _ => return Err(changed("七猫章节权限标记未知")),
            };
            chapters.push(ChapterRef {
                id: id.into(),
                title: title.into(),
                restricted,
            });
        }
        let latest = page::one(&document, ".book-information .update-chapter-title a")?;
        let expected = format!(
            "https://www.qimao.com/shuku/{book}-{}/",
            chapters.last().expect("nonempty list").id
        );
        if latest.value().attr("href") != Some(expected.as_str()) {
            return Err(changed("七猫目录最新章节与作品页身份不符，请重新读取"));
        }
        let catalog = Catalog {
            source: "qimao-web".into(),
            remote_id: book.into(),
            title,
            author,
            description,
            chapters,
        };
        catalog.validate()?;
        Ok(catalog)
    }

    pub fn parse_chapter(html: &str, book: &str, chapter: &ChapterRef) -> Result<ImportChapter> {
        if chapter.restricted {
            return Err(Error::Restricted(
                "七猫目录标记该章需订阅，当前仅支持明确免订阅正文".into(),
            ));
        }
        if !page::numeric(book) || !page::numeric(&chapter.id) {
            return Err(changed("七猫章节身份无效"));
        }
        page::route_identity(html, &format!("/shuku/{book}-{}/", chapter.id))?;
        let document = Html::parse_document(html);
        let title = page::text(
            &document,
            ".chapter-detail-wrap .chapter-detail-wrap-info .chapter-title",
        )?;
        if title.split_whitespace().collect::<String>()
            != chapter.title.split_whitespace().collect::<String>()
        {
            return Err(changed("七猫正文标题与目录不一致"));
        }
        let tips = page::one(&document, ".chapter-detail-wrap .chapter-tips")?;
        let labels = tips
            .select(&page::selector("dt"))
            .map(|e| e.text().collect::<String>())
            .collect::<Vec<_>>();
        let values = tips
            .select(&page::selector("dd"))
            .map(|e| e.text().collect::<String>())
            .collect::<Vec<_>>();
        let words = labels
            .iter()
            .position(|s| s.trim().trim_end_matches(['：', ':']) == "本章字数")
            .and_then(|i| values.get(i))
            .and_then(|s| s.trim().parse::<usize>().ok())
            .filter(|n| *n > 0)
            .ok_or_else(|| changed("七猫正文声明字数缺失"))?;
        let body = page::one(
            &document,
            ".chapter-detail-wrap > .chapter-detail-wrap-content",
        )?;
        let paragraphs = body
            .select(&page::selector(".chapter-detail-article > .article > p"))
            .map(|p| p.text().collect::<String>().trim().to_owned())
            .filter(|p| !p.is_empty())
            .collect::<Vec<_>>();
        let actual = paragraphs.iter().map(|p| p.chars().count()).sum::<usize>();
        if (actual as u128) * 100 < (words as u128) * 80
            || (actual as u128) * 100 > (words as u128) * 150
            || paragraphs
                .iter()
                .flat_map(|p| p.chars())
                .any(|c| ('\u{e000}'..='\u{f8ff}').contains(&c))
        {
            return Err(changed("七猫正文长度不符或含未知字形，不能保存截断预览"));
        }
        let input = ImportDocument {
            id: book.into(),
            title: "七猫正文校验".into(),
            author: String::new(),
            chapters: vec![ImportChapter {
                id: chapter.id.clone(),
                title,
                paragraphs,
            }],
        };
        Ok(ImportDocument::parse(&serde_json::to_string(&input)?)?
            .chapters
            .remove(0))
    }
}
impl SourceAdapter for Qimao {
    fn source_id(&self) -> &str {
        "qimao-web"
    }
    fn fetch_chapter(
        &self,
        book: &str,
        chapter: &ChapterRef,
        stop: &AtomicBool,
    ) -> Result<ImportChapter> {
        if chapter.restricted {
            return Err(Error::Restricted("七猫目录标记该章需订阅".into()));
        }
        if !page::numeric(book) || !page::numeric(&chapter.id) {
            return Err(changed("七猫章节身份无效"));
        }
        let html = self.http.text(
            &format!("https://www.qimao.com/shuku/{book}-{}/", chapter.id),
            stop,
            2 * 1024 * 1024,
        )?;
        Self::parse_chapter(&html, book, chapter)
    }
}
