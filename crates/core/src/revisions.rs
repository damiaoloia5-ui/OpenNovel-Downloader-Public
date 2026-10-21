use crate::{
    BookContent, BookDetails, BookSummary, Error, ExportFormat, ExportReceipt, ImportChapter,
    ImportDocument, Library, ReadingChapter, Result,
    sources::{Catalog, SourceAdapter},
};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogChange {
    pub chapter_id: String,
    pub kind: String,
    pub previous_position: Option<usize>,
    pub position: Option<usize>,
    pub previous_title: Option<String>,
    pub title: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogUpdate {
    pub previous_hash: String,
    pub proposed_hash: String,
    pub changed: bool,
    pub changes: Vec<CatalogChange>,
    pub metadata: Vec<CatalogFieldChange>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogFieldChange {
    pub field: String,
    pub previous: String,
    pub proposed: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: String,
    pub book_id: String,
    pub title: String,
    pub reason: String,
    pub created_at: String,
    pub saved: usize,
    pub total: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefreshReceipt {
    pub changed: bool,
    pub snapshot_id: Option<String>,
}
fn digest(raw: &str) -> String {
    format!("{:x}", Sha256::digest(raw.as_bytes()))
}
fn catalog_raw(db: &Connection, book: &str) -> Result<String> {
    db.query_row(
        "SELECT catalog FROM source_catalogs WHERE book_id=?1",
        [book],
        |r| r.get(0),
    )
    .optional()?
    .ok_or_else(|| Error::Invalid("该作品没有可更新的在线目录".into()))
}
fn idle(db: &Connection, book: &str) -> Result<()> {
    let active:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM jobs WHERE book_id=?1 AND state IN ('queued','running','paused'))",[book],|r|r.get(0))?;
    if active {
        return Err(Error::Conflict(
            "请先完成或取消该作品的待处理任务，再应用修订".into(),
        ));
    }
    Ok(())
}
pub(crate) fn snapshot(
    tx: &Transaction<'_>,
    book: &str,
    raw: &str,
    reason: &str,
) -> Result<String> {
    let id = uuid::Uuid::new_v4().to_string();
    tx.execute(
        "INSERT INTO catalog_history(id,book_id,catalog,reason) VALUES(?1,?2,?3,?4)",
        params![id, book, raw, reason],
    )?;
    let rows=tx.prepare("SELECT chapter_id,position,title,paragraphs FROM chapters WHERE book_id=?1 ORDER BY position")?.query_map([book],|r|Ok((r.get::<_,String>(0)?,r.get::<_,usize>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?)))?.collect::<std::result::Result<Vec<_>,_>>()?;
    for (chapter, position, title, body) in rows {
        let hash = digest(&body);
        tx.execute(
            "INSERT OR IGNORE INTO content_blobs(hash,paragraphs) VALUES(?1,?2)",
            params![hash, body],
        )?;
        tx.execute("INSERT INTO history_chapters(snapshot_id,chapter_id,position,title,body_hash) VALUES(?1,?2,?3,?4,?5)",params![id,chapter,position,title,hash])?;
    }
    // Freeze completed job exports at the first subsequent content/catalog change.
    tx.execute("INSERT OR IGNORE INTO job_history(job_id,snapshot_id) SELECT id,?2 FROM jobs WHERE book_id=?1",params![book,id])?;
    Ok(id)
}
impl Library {
    pub fn preview_catalog_update(&self, book: &str, proposed: &Catalog) -> Result<CatalogUpdate> {
        proposed.validate()?;
        let db = self.connection()?;
        let raw = catalog_raw(&db, book)?;
        let previous: Catalog = serde_json::from_str(&raw)?;
        if proposed.source != previous.source || proposed.remote_id != previous.remote_id {
            return Err(Error::Invalid("更新目录与原作品身份不一致".into()));
        }
        let old = previous
            .chapters
            .iter()
            .enumerate()
            .map(|(i, c)| (&c.id, (i, c)))
            .collect::<HashMap<_, _>>();
        let new = proposed
            .chapters
            .iter()
            .enumerate()
            .map(|(i, c)| (&c.id, (i, c)))
            .collect::<HashMap<_, _>>();
        let mut changes = Vec::new();
        for (i, c) in previous.chapters.iter().enumerate() {
            if !new.contains_key(&c.id) {
                changes.push(CatalogChange {
                    chapter_id: c.id.clone(),
                    kind: "removed".into(),
                    previous_position: Some(i + 1),
                    position: None,
                    previous_title: Some(c.title.clone()),
                    title: None,
                });
            }
        }
        for (i, c) in proposed.chapters.iter().enumerate() {
            let prior = old.get(&c.id);
            let kind = if let Some((p, o)) = prior {
                if *p != i || o.title != c.title {
                    Some("moved_or_renamed")
                } else if o.restricted != c.restricted {
                    Some("access_changed")
                } else {
                    None
                }
            } else {
                Some("added")
            };
            if let Some(kind) = kind {
                changes.push(CatalogChange {
                    chapter_id: c.id.clone(),
                    kind: kind.into(),
                    previous_position: prior.map(|(p, _)| p + 1),
                    position: Some(i + 1),
                    previous_title: prior.map(|(_, o)| o.title.clone()),
                    title: Some(c.title.clone()),
                });
            }
        }
        let proposed_hash = digest(&serde_json::to_string(proposed)?);
        let previous_hash = digest(&raw);
        let mut metadata = Vec::new();
        for (field, before, after) in [
            ("title", &previous.title, &proposed.title),
            ("author", &previous.author, &proposed.author),
            ("description", &previous.description, &proposed.description),
        ] {
            if before != after {
                metadata.push(CatalogFieldChange {
                    field: field.into(),
                    previous: before.clone(),
                    proposed: after.clone(),
                });
            }
        }
        Ok(CatalogUpdate {
            changed: proposed_hash != previous_hash,
            proposed_hash,
            previous_hash,
            changes,
            metadata,
        })
    }
    pub fn accept_catalog_update(
        &self,
        book: &str,
        proposed: &Catalog,
        expected: &str,
    ) -> Result<String> {
        let preview = self.preview_catalog_update(book, proposed)?;
        if !preview.changed {
            return Err(Error::Conflict("目录没有变化，无需建立历史快照".into()));
        }
        let mut db = self.connection()?;
        let tx = db.transaction()?;
        idle(&tx, book)?;
        let raw = catalog_raw(&tx, book)?;
        if digest(&raw) != expected || preview.previous_hash != expected {
            return Err(Error::Conflict("本地目录已变化，请重新预览".into()));
        }
        let id = snapshot(&tx, book, &raw, "catalog_update")?;
        // Move all saved positions out of the target range before assigning any new order.
        tx.execute(
            "UPDATE chapters SET position=position+10001 WHERE book_id=?1",
            [book],
        )?;
        let ids = proposed.chapters.iter().map(|c| &c.id).collect::<Vec<_>>();
        tx.execute("DELETE FROM chapters WHERE book_id=?1 AND chapter_id NOT IN (SELECT value FROM json_each(?2))",params![book,serde_json::to_string(&ids)?])?;
        for (position, c) in proposed.chapters.iter().enumerate() {
            tx.execute(
                "UPDATE chapters SET position=?3,title=?4 WHERE book_id=?1 AND chapter_id=?2",
                params![book, c.id, position, c.title],
            )?;
        }
        tx.execute(
            "UPDATE source_catalogs SET catalog=?2 WHERE book_id=?1",
            params![book, serde_json::to_string(proposed)?],
        )?;
        tx.execute(
            "UPDATE books SET title=?2,author=?3,total=?4 WHERE id=?1",
            params![
                book,
                proposed.title,
                proposed.author,
                proposed.chapters.len()
            ],
        )?;
        tx.commit()?;
        Ok(id)
    }
    pub fn refresh_chapter(
        &self,
        book: &str,
        chapter: &str,
        source: &dyn SourceAdapter,
        stop: &AtomicBool,
    ) -> Result<RefreshReceipt> {
        let (raw, before) = {
            let db = self.connection()?;
            idle(&db, book)?;
            let raw = catalog_raw(&db, book)?;
            let before: Option<String> = db
                .query_row(
                    "SELECT paragraphs FROM chapters WHERE book_id=?1 AND chapter_id=?2",
                    params![book, chapter],
                    |r| r.get(0),
                )
                .optional()?;
            (
                raw,
                before.ok_or_else(|| Error::Invalid("只能检查已经保存的章节修订".into()))?,
            )
        };
        let catalog: Catalog = serde_json::from_str(&raw)?;
        if catalog.source != source.source_id() {
            return Err(Error::Invalid("正文修订来源不匹配".into()));
        }
        let ch = catalog
            .chapters
            .iter()
            .find(|c| c.id == chapter)
            .ok_or_else(|| Error::Invalid("章节不在当前目录".into()))?;
        if ch.restricted {
            return Err(Error::Restricted("当前目录标记受限，不重新请求正文".into()));
        }
        let fetched = source.fetch_chapter(&catalog.remote_id, ch, stop)?;
        if stop.load(Ordering::Acquire) {
            return Err(Error::Cancelled);
        }
        if fetched.id != ch.id || fetched.title != ch.title {
            return Err(Error::SourceChanged(
                "修订正文与请求章节身份/标题不符".into(),
            ));
        }
        let input = ImportDocument {
            id: catalog.remote_id,
            title: catalog.title,
            author: catalog.author,
            chapters: vec![fetched],
        };
        let fetched = ImportDocument::parse(&serde_json::to_string(&input)?)?
            .chapters
            .remove(0);
        let after = serde_json::to_string(&fetched.paragraphs)?;
        let mut db = self.connection()?;
        let tx = db.transaction()?;
        idle(&tx, book)?;
        let current: String = tx.query_row(
            "SELECT paragraphs FROM chapters WHERE book_id=?1 AND chapter_id=?2",
            params![book, chapter],
            |r| r.get(0),
        )?;
        if catalog_raw(&tx, book)? != raw || current != before {
            return Err(Error::Conflict("修订期间书库内容已变化，请重试".into()));
        }
        if after == before {
            return Ok(RefreshReceipt {
                changed: false,
                snapshot_id: None,
            });
        }
        if stop.load(Ordering::Acquire) {
            return Err(Error::Cancelled);
        }
        let id = snapshot(&tx, book, &raw, "chapter_refresh")?;
        tx.execute(
            "UPDATE chapters SET paragraphs=?3 WHERE book_id=?1 AND chapter_id=?2",
            params![book, chapter, after],
        )?;
        tx.commit()?;
        Ok(RefreshReceipt {
            changed: true,
            snapshot_id: Some(id),
        })
    }
    pub fn history(&self, book: &str) -> Result<Vec<HistoryEntry>> {
        let db = self.connection()?;
        let rows=db.prepare("SELECT h.id,h.catalog,h.reason,h.created_at,(SELECT COUNT(*) FROM history_chapters c WHERE c.snapshot_id=h.id) FROM catalog_history h WHERE h.book_id=?1 ORDER BY h.rowid DESC")?.query_map([book],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,usize>(4)?)))?.collect::<std::result::Result<Vec<_>,_>>()?;
        rows.into_iter()
            .map(|(id, raw, reason, created_at, saved)| {
                let cat: Catalog = serde_json::from_str(&raw)?;
                Ok(HistoryEntry {
                    id,
                    book_id: book.into(),
                    title: cat.title,
                    reason,
                    created_at,
                    saved,
                    total: cat.chapters.len(),
                })
            })
            .collect()
    }
    pub(crate) fn job_history_id(&self, job: &str) -> Result<Option<String>> {
        Ok(self
            .connection()?
            .query_row(
                "SELECT snapshot_id FROM job_history WHERE job_id=?1",
                [job],
                |r| r.get(0),
            )
            .optional()?)
    }
    pub(crate) fn historical_content(&self, id: &str) -> Result<BookContent> {
        let db = self.connection()?;
        let (book, raw, modified) = db
            .query_row(
                "SELECT book_id,catalog,created_at FROM catalog_history WHERE id=?1",
                [id],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()?
            .ok_or_else(|| Error::Invalid("历史快照不存在".into()))?;
        let cat: Catalog = serde_json::from_str(&raw)?;
        let rows=db.prepare("SELECT c.chapter_id,c.title,b.paragraphs FROM history_chapters c JOIN content_blobs b ON b.hash=c.body_hash WHERE c.snapshot_id=?1 ORDER BY c.position")?.query_map([id],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?)))?.collect::<std::result::Result<Vec<_>,_>>()?;
        let chapters = rows
            .into_iter()
            .map(|(id, title, body)| {
                Ok(ImportChapter {
                    id,
                    title,
                    paragraphs: serde_json::from_str(&body)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(BookContent {
            summary: BookSummary {
                id: book,
                title: cat.title,
                author: cat.author,
                source: cat.source,
                remote_id: cat.remote_id,
                saved_chapters: chapters.len() as u32,
                total_chapters: cat.chapters.len() as u32,
            },
            chapters,
            modified,
        })
    }
    pub fn history_details(&self, id: &str) -> Result<BookDetails> {
        let db = self.connection()?;
        let (book, raw, date) = db.query_row(
            "SELECT book_id,catalog,created_at FROM catalog_history WHERE id=?1",
            [id],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            },
        )?;
        let cat: Catalog = serde_json::from_str(&raw)?;
        let saved = db
            .prepare("SELECT chapter_id FROM history_chapters WHERE snapshot_id=?1")?
            .query_map([id], |r| r.get::<_, String>(0))?
            .collect::<std::result::Result<std::collections::HashSet<_>, _>>()?;
        let summary = BookSummary {
            id: book,
            title: cat.title,
            author: cat.author,
            source: cat.source,
            remote_id: cat.remote_id,
            saved_chapters: saved.len() as u32,
            total_chapters: cat.chapters.len() as u32,
        };
        Ok(BookDetails {
            book: summary,
            description: cat.description,
            acquired_at: date,
            chapters: cat
                .chapters
                .into_iter()
                .enumerate()
                .map(|(p, c)| ReadingChapter {
                    saved: saved.contains(&c.id),
                    id: c.id,
                    title: c.title,
                    position: p as u32 + 1,
                })
                .collect(),
            exports: Vec::new(),
        })
    }
    pub fn read_history(&self, id: &str, chapter: &str) -> Result<ImportChapter> {
        let row=self.connection()?.query_row("SELECT c.title,b.paragraphs FROM history_chapters c JOIN content_blobs b ON b.hash=c.body_hash WHERE c.snapshot_id=?1 AND c.chapter_id=?2",params![id,chapter],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?))).optional()?.ok_or_else(||Error::Invalid("历史快照中没有该章正文".into()))?;
        Ok(ImportChapter {
            id: chapter.into(),
            title: row.0,
            paragraphs: serde_json::from_str(&row.1)?,
        })
    }
    pub fn export_history(
        &self,
        id: &str,
        directory: impl AsRef<Path>,
        format: ExportFormat,
    ) -> Result<ExportReceipt> {
        let mut content = self.historical_content(id)?;
        if content.chapters.is_empty() {
            return Err(Error::Conflict("历史快照没有已保存正文".into()));
        }
        let book = content.summary.id.clone();
        content.summary.id = format!("history-{id}");
        content.summary.title.push_str("（历史快照）");
        self.persist_export(&book, None, content, directory.as_ref(), format)
    }
}
