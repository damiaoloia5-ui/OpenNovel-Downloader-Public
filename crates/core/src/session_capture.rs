//! User-confirmed snapshots of the normally rendered official reading page.
//! This module has no HTTP client, credentials, decryption, or automatic navigation.
use crate::{
    Error, Library, Result,
    sources::{Catalog, ChapterRef},
};
use reqwest::Url;
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

pub const MAX_CAPTURE_BYTES: usize = 2 * 1024 * 1024;
pub const QIDIAN_SESSION_SOURCE: &str = "qidian-session";

#[derive(Debug, Clone)]
pub struct SessionCatalog(Catalog);

#[derive(Debug, Clone, Serialize)]
pub struct SessionChapter {
    pub remote_book_id: String,
    pub chapter: crate::ImportChapter,
    pub declared_words: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionSaveReceipt {
    pub book_id: String,
    pub title: String,
    pub chapter_title: String,
    pub position: usize,
    pub saved: u32,
    pub total: usize,
    pub already_saved: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CatalogPage {
    url: String,
    title: String,
    author: String,
    declared_total: usize,
    chapters: Vec<CatalogItem>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CatalogItem {
    id: String,
    title: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ChapterPage {
    url: String,
    title: String,
    paragraphs: Vec<String>,
    declared_words: u32,
}

fn numeric_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= 30 && value.bytes().all(|c| c.is_ascii_digit())
}

/// A clean URL containing only a recognized reader/book identity, never login parameters.
pub fn qidian_capture_url(input: &str) -> Result<(String, Option<String>)> {
    let url = Url::parse(input).map_err(|_| Error::Invalid("请打开起点官网作品或章节页".into()))?;
    if url.scheme() != "https"
        || url.host_str() != Some("www.qidian.com")
        || url.port().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(Error::Invalid("只读取起点官网的干净作品或章节地址".into()));
    }
    let parts = url.path().trim_matches('/').split('/').collect::<Vec<_>>();
    match parts.as_slice() {
        ["book", book] if numeric_id(book) => Ok(((*book).into(), None)),
        ["book", book, "catalog"] if numeric_id(book) => Ok(((*book).into(), None)),
        ["chapter", book, chapter] if numeric_id(book) && numeric_id(chapter) => {
            Ok(((*book).into(), Some((*chapter).into())))
        }
        _ => Err(Error::Invalid(
            "当前页面不是支持的起点作品或章节地址".into(),
        )),
    }
}

fn parse_page<T: serde::de::DeserializeOwned>(raw: &str) -> Result<T> {
    if raw.len() > MAX_CAPTURE_BYTES {
        return Err(Error::Invalid("页面读取结果超过 2 MiB".into()));
    }
    serde_json::from_str(raw).map_err(|_| Error::SourceChanged("页面结构无法识别，未保存".into()))
}

fn same_page(actual: &str, expected: &str) -> Result<(String, Option<String>)> {
    let identity = qidian_capture_url(actual)?;
    if identity != qidian_capture_url(expected)? {
        return Err(Error::Conflict("页面已切换，请重新读取预览".into()));
    }
    Ok(identity)
}

impl SessionCatalog {
    pub fn parse(raw: &str, expected_url: &str) -> Result<Self> {
        let page: CatalogPage = parse_page(raw)?;
        let (book, chapter) = same_page(&page.url, expected_url)?;
        if chapter.is_some()
            || page.declared_total == 0
            || page.declared_total > crate::MAX_CHAPTERS
            || page.chapters.len() != page.declared_total
        {
            return Err(Error::SourceChanged(
                "目录未完整加载或官网总章数缺失；请展开完整目录后重试".into(),
            ));
        }
        crate::domain::validate_label(page.title.trim(), "书名", 500)?;
        if page.author.chars().count() > 500 || crate::domain::invalid_xml(&page.author) {
            return Err(Error::SourceChanged("作者字段无效".into()));
        }
        let catalog = Catalog {
            source: QIDIAN_SESSION_SOURCE.into(),
            remote_id: book,
            title: page.title.trim().into(),
            author: page.author.trim().into(),
            description: "起点官网当前可读章节；目录不表示阅读权限，正文逐章预览确认保存。".into(),
            chapters: page
                .chapters
                .into_iter()
                .map(|item| ChapterRef {
                    id: item.id,
                    title: item.title.trim().into(),
                    restricted: false,
                })
                .collect(),
        };
        catalog.validate()?;
        if catalog.chapters.iter().any(|c| {
            !numeric_id(&c.id)
                || c.title.chars().count() > 500
                || c.title.chars().any(char::is_control)
                || crate::domain::invalid_xml(&c.title)
        }) {
            return Err(Error::SourceChanged("目录章节身份或标题无效".into()));
        }
        Ok(Self(catalog))
    }
    pub fn catalog(&self) -> &Catalog {
        &self.0
    }
}

impl SessionChapter {
    pub fn parse(raw: &str, expected_url: &str) -> Result<Self> {
        let page: ChapterPage = parse_page(raw)?;
        let (book, chapter) = same_page(&page.url, expected_url)?;
        let chapter = chapter.ok_or_else(|| Error::Invalid("请先打开一章完整正文".into()))?;
        if page.declared_words == 0
            || page.declared_words > 200_000
            || page.paragraphs.len() > 10_000
        {
            return Err(Error::SourceChanged(
                "正文标注字数缺失或超过单章限制".into(),
            ));
        }
        let text_len = page
            .paragraphs
            .iter()
            .map(|p| p.chars().filter(|c| !c.is_whitespace()).count())
            .sum::<usize>();
        if text_len > 200_000 || text_len * 5 < page.declared_words as usize * 4 {
            return Err(Error::Restricted(
                "正文少于标注字数的 80%，可能是试读或尚未加载完整，未保存".into(),
            ));
        }
        if page.paragraphs.iter().any(|p| {
            p.chars()
                .any(|c| matches!(c, '\u{e000}'..='\u{f8ff}' | '\u{fffd}'))
                || [
                    "订阅本章",
                    "本章为付费章节",
                    "本章为VIP章节",
                    "试读结束",
                    "试看结束",
                ]
                .iter()
                .any(|marker| p.contains(marker))
        }) {
            return Err(Error::Restricted(
                "检测到订阅提示或受保护字形，未保存；请使用官网正常阅读方式".into(),
            ));
        }
        let document = crate::ImportDocument {
            id: book.clone(),
            title: "当前章".into(),
            author: String::new(),
            chapters: vec![crate::ImportChapter {
                id: chapter,
                title: page.title,
                paragraphs: page.paragraphs,
            }],
        }
        .validated()?;
        Ok(Self {
            remote_book_id: book,
            chapter: document
                .chapters
                .into_iter()
                .next()
                .expect("validated chapter"),
            declared_words: page.declared_words,
        })
    }
}

impl Library {
    pub fn is_session_download(&self, job_id: &str) -> Result<bool> {
        Ok(self.connection()?.query_row("SELECT EXISTS(SELECT 1 FROM jobs j JOIN books b ON b.id=j.book_id JOIN job_items i ON i.job_id=j.id WHERE j.id=?1 AND b.source=?2 AND CASE WHEN json_valid(i.payload) THEN json_extract(i.payload,'$.automatic_session') ELSE 0 END=1)",params![job_id,QIDIAN_SESSION_SOURCE],|r|r.get(0))?)
    }

    pub fn session_download_jobs(&self, book_id: &str) -> Result<Vec<crate::JobSnapshot>> {
        self.session_catalog(book_id)?;
        let ids = self.connection()?.prepare("SELECT j.id FROM jobs j WHERE j.book_id=?1 AND EXISTS(SELECT 1 FROM job_items i WHERE i.job_id=j.id AND CASE WHEN json_valid(i.payload) THEN json_extract(i.payload,'$.automatic_session') ELSE 0 END=1) ORDER BY j.rowid DESC LIMIT 20")?.query_map([book_id],|r|r.get::<_,String>(0))?.collect::<std::result::Result<Vec<_>,_>>()?;
        ids.iter().map(|id| self.job(id)).collect()
    }

    pub fn session_catalog(&self, book_id: &str) -> Result<Catalog> {
        let raw: String = self.connection()?.query_row(
            "SELECT c.catalog FROM source_catalogs c JOIN books b ON b.id=c.book_id WHERE b.id=?1 AND b.source=?2",
            params![book_id,QIDIAN_SESSION_SOURCE],|r|r.get(0)
        ).optional()?.ok_or_else(||Error::Invalid("请先确认保存起点的完整目录".into()))?;
        let catalog: Catalog = serde_json::from_str(&raw)?;
        catalog.validate()?;
        if catalog.source != QIDIAN_SESSION_SOURCE
            || !numeric_id(&catalog.remote_id)
            || catalog.chapters.iter().any(|c| !numeric_id(&c.id))
        {
            return Err(Error::Invalid("保存的起点目录身份无效".into()));
        }
        Ok(catalog)
    }

    /// Account-bound browser workers use this entry; generic HTTP/batch download stays disabled.
    pub fn create_session_download(
        &self,
        book_id: &str,
        selection: Option<&str>,
    ) -> Result<String> {
        let catalog = self.session_catalog(book_id)?;
        let positions = match selection.filter(|s| !s.trim().is_empty()) {
            Some(s) => crate::parse_selection(s, catalog.chapters.len())?,
            None => (1..=catalog.chapters.len()).collect(),
        };
        self.create_download_authorized(
            &catalog,
            &positions,
            None,
            crate::downloads::DownloadAuthorization::QidianSession,
        )
    }

    pub fn register_session_catalog(&self, captured: &SessionCatalog) -> Result<String> {
        let catalog = captured.catalog();
        let raw = serde_json::to_string(catalog)?;
        let mut db = self.connection()?;
        let tx = db.transaction()?;
        let existing: Option<String> = tx
            .query_row(
                "SELECT id FROM books WHERE source=?1 AND remote_id=?2",
                params![QIDIAN_SESSION_SOURCE, catalog.remote_id],
                |r| r.get(0),
            )
            .optional()?;
        let book_id = if let Some(id) = existing {
            let previous: String = tx.query_row(
                "SELECT catalog FROM source_catalogs WHERE book_id=?1",
                [&id],
                |r| r.get(0),
            )?;
            let old: Catalog = serde_json::from_str(&previous)?;
            if catalog.chapters.len() < old.chapters.len()
                || !old.chapters.iter().map(|c| &c.id).eq(catalog
                    .chapters
                    .iter()
                    .take(old.chapters.len())
                    .map(|c| &c.id))
            {
                return Err(Error::Conflict(
                    "目录包含删章或重排，已保留原书库；当前保存流程仅接受末尾增章".into(),
                ));
            }
            if previous != raw {
                let active: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM jobs WHERE book_id=?1 AND state IN ('queued','running','paused'))",[&id],|r|r.get(0))?;
                if active {
                    return Err(Error::Conflict("请先完成或取消自动任务，再更新目录".into()));
                }
                crate::revisions::snapshot(&tx, &id, &previous, "catalog_update")?;
                tx.execute(
                    "UPDATE books SET title=?2,author=?3,total=?4 WHERE id=?1",
                    params![id, catalog.title, catalog.author, catalog.chapters.len()],
                )?;
                tx.execute(
                    "UPDATE source_catalogs SET catalog=?2 WHERE book_id=?1",
                    params![id, raw],
                )?;
            }
            id
        } else {
            let id = uuid::Uuid::new_v4().to_string();
            tx.execute("INSERT INTO books(id,source,remote_id,title,author,total) VALUES(?1,?2,?3,?4,?5,?6)",params![id,QIDIAN_SESSION_SOURCE,catalog.remote_id,catalog.title,catalog.author,catalog.chapters.len()])?;
            tx.execute(
                "INSERT INTO source_catalogs(book_id,catalog) VALUES(?1,?2)",
                params![id, raw],
            )?;
            id
        };
        tx.commit()?;
        Ok(book_id)
    }

    /// Body and completed task state commit together. Repeated/changed pages never overwrite text.
    pub fn save_session_chapter(&self, captured: &SessionChapter) -> Result<SessionSaveReceipt> {
        // Public fields are useful for previews; revalidate before accepting a caller-built value.
        let url = format!(
            "https://www.qidian.com/chapter/{}/{}/",
            captured.remote_book_id, captured.chapter.id
        );
        let captured = SessionChapter::parse(
            &serde_json::to_string(
                &serde_json::json!({"url":url,"title":captured.chapter.title,"paragraphs":captured.chapter.paragraphs,"declared_words":captured.declared_words}),
            )?,
            &url,
        )?;
        let mut db = self.connection()?;
        let tx = db.transaction()?;
        let (book_id,raw):(String,String) = tx.query_row("SELECT b.id,c.catalog FROM books b JOIN source_catalogs c ON c.book_id=b.id WHERE b.source=?1 AND b.remote_id=?2",params![QIDIAN_SESSION_SOURCE,captured.remote_book_id],|r|Ok((r.get(0)?,r.get(1)?))).optional()?.ok_or_else(||Error::Invalid("请先读取并确认保存该作品的完整目录".into()))?;
        let catalog: Catalog = serde_json::from_str(&raw)?;
        let chapter = &captured.chapter;
        let position = catalog
            .chapters
            .iter()
            .position(|c| c.id == chapter.id)
            .ok_or_else(|| Error::Conflict("当前章不在已确认目录中，请先更新完整目录".into()))?;
        if catalog.chapters[position]
            .title
            .split_whitespace()
            .collect::<String>()
            != chapter.title.split_whitespace().collect::<String>()
        {
            return Err(Error::Conflict(
                "章节标题与目录不一致，请重新核对页面和目录".into(),
            ));
        }
        let body = serde_json::to_string(&chapter.paragraphs)?;
        let old: Option<(String, String)> = tx
            .query_row(
                "SELECT title,paragraphs FROM chapters WHERE book_id=?1 AND chapter_id=?2",
                params![book_id, chapter.id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let already_saved = old.is_some();
        if let Some((title, previous)) = old {
            if previous != body || title != chapter.title {
                return Err(Error::Conflict(
                    "该章已保存且内容不同，本次未覆盖原文；请核对官网修订或试读差异".into(),
                ));
            }
        } else {
            let job = uuid::Uuid::new_v4().to_string();
            tx.execute("INSERT INTO chapters(book_id,chapter_id,position,title,paragraphs) VALUES(?1,?2,?3,?4,?5)",params![book_id,chapter.id,position,chapter.title,body])?;
            tx.execute(
                "INSERT INTO jobs(id,book_id,state) VALUES(?1,?2,'completed')",
                params![job, book_id],
            )?;
            tx.execute("INSERT INTO job_items(job_id,chapter_id,position,payload,state) VALUES(?1,?2,?3,?4,'completed')",params![job,chapter.id,position,serde_json::to_string(chapter)?])?;
        }
        let saved = tx.query_row(
            "SELECT COUNT(*) FROM chapters WHERE book_id=?1",
            [&book_id],
            |r| r.get(0),
        )?;
        let receipt = SessionSaveReceipt {
            book_id,
            title: catalog.title,
            chapter_title: chapter.title.clone(),
            position: position + 1,
            saved,
            total: catalog.chapters.len(),
            already_saved,
        };
        tx.commit()?;
        Ok(receipt)
    }
}
