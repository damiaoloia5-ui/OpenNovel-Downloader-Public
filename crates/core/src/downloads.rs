use crate::{
    Error, ImportChapter, ImportDocument, JobSnapshot, Library, Result,
    sources::{Catalog, ChapterRef, SourceAdapter},
};
use rusqlite::{OptionalExtension, params};
use std::sync::atomic::{AtomicBool, Ordering};
use uuid::Uuid;

impl Library {
    /// Persist the catalog and selected remote chapter identities before any text request.
    pub fn create_download(&self, catalog: &Catalog, first: usize, last: usize) -> Result<String> {
        catalog.validate()?;
        if catalog.source != "fanqie-web" {
            return Err(Error::Invalid("尚未接入该网络来源".into()));
        }
        if first == 0 || last < first || last > catalog.chapters.len() {
            return Err(Error::Invalid("章节范围超出目录；序号从 1 开始".into()));
        }
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let existing = transaction
            .query_row(
                "SELECT id FROM books WHERE source=?1 AND remote_id=?2",
                params![catalog.source, catalog.remote_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let book_id = if let Some(book_id) = existing {
            let previous: String = transaction.query_row(
                "SELECT catalog FROM source_catalogs WHERE book_id=?1",
                [&book_id],
                |row| row.get(0),
            )?;
            let previous: Catalog = serde_json::from_str(&previous)?;
            if !previous
                .chapters
                .iter()
                .map(|c| &c.id)
                .eq(catalog.chapters.iter().map(|c| &c.id))
            {
                return Err(Error::Conflict(
                    "官网目录发生变化，当前预览版暂停该作品的新下载以保护已保存内容".into(),
                ));
            }
            let active:u32=transaction.query_row("SELECT COUNT(*) FROM jobs WHERE book_id=?1 AND state IN ('queued','running','paused')",[&book_id],|row|row.get(0))?;
            if active > 0 {
                return Err(Error::Conflict(
                    "该作品还有待执行或中断任务，请先继续或取消原任务".into(),
                ));
            }
            book_id
        } else {
            let book_id = Uuid::new_v4().to_string();
            transaction.execute("INSERT INTO books (id,source,remote_id,title,author,total) VALUES (?1,?2,?3,?4,?5,?6)",params![book_id,catalog.source,catalog.remote_id,catalog.title,catalog.author,catalog.chapters.len()])?;
            transaction.execute(
                "INSERT INTO source_catalogs (book_id,catalog) VALUES (?1,?2)",
                params![book_id, serde_json::to_string(catalog)?],
            )?;
            book_id
        };
        let job_id = Uuid::new_v4().to_string();
        transaction.execute(
            "INSERT INTO jobs (id,book_id,state) VALUES (?1,?2,'queued')",
            params![job_id, book_id],
        )?;
        for (position, chapter) in catalog
            .chapters
            .iter()
            .enumerate()
            .take(last)
            .skip(first - 1)
        {
            let saved: bool = transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM chapters WHERE book_id=?1 AND chapter_id=?2)",
                params![book_id, chapter.id],
                |row| row.get(0),
            )?;
            let state = if saved { "completed" } else { "queued" };
            transaction.execute("INSERT INTO job_items (job_id,chapter_id,position,payload,state) VALUES (?1,?2,?3,?4,?5)",params![job_id,chapter.id,position,serde_json::to_string(chapter)?,state])?;
        }
        transaction.commit()?;
        Ok(job_id)
    }

    pub fn run_job(
        &self,
        job_id: &str,
        source: &dyn SourceAdapter,
        cancelled: &AtomicBool,
    ) -> Result<JobSnapshot> {
        let job = self.job(job_id)?;
        if job.source == "local-json" {
            return self.run_import(job_id, cancelled);
        }
        if job.source != source.source_id() {
            return Err(Error::Invalid("该任务需要其他来源适配器".into()));
        }
        self.start_job(job_id)?;
        self.connection()?.execute(
            "UPDATE job_items SET state='queued' WHERE job_id=?1 AND state='failed'",
            [job_id],
        )?;
        let result = self.download_items(job_id, source, cancelled);
        if let Err(error) = &result {
            self.connection()?.execute(
                "UPDATE jobs SET state='failed',error=?2 WHERE id=?1 AND state='running'",
                params![job_id, error.to_string()],
            )?;
        }
        result?;
        self.job(job_id)
    }

    fn download_items(
        &self,
        job_id: &str,
        source: &dyn SourceAdapter,
        cancelled: &AtomicBool,
    ) -> Result<()> {
        let mut last_error = None::<String>;
        loop {
            if cancelled.load(Ordering::Acquire) {
                return self.finish_cancel(job_id);
            }
            let next = {
                let connection = self.connection()?;
                connection.query_row("SELECT i.payload,i.position,j.book_id,b.remote_id FROM job_items i JOIN jobs j ON i.job_id=j.id JOIN books b ON j.book_id=b.id WHERE i.job_id=?1 AND i.state='queued' ORDER BY i.position LIMIT 1",[job_id],|row|Ok((row.get::<_,String>(0)?,row.get::<_,u32>(1)?,row.get::<_,String>(2)?,row.get::<_,String>(3)?))).optional()?
            };
            let Some((metadata, position, book_id, remote_id)) = next else {
                break;
            };
            let chapter: ChapterRef = serde_json::from_str(&metadata)?;
            let cached: bool = self.connection()?.query_row(
                "SELECT EXISTS(SELECT 1 FROM chapters WHERE book_id=?1 AND chapter_id=?2)",
                params![book_id, chapter.id],
                |row| row.get(0),
            )?;
            if cached {
                self.connection()?.execute(
                    "UPDATE job_items SET state='completed' WHERE job_id=?1 AND chapter_id=?2",
                    params![job_id, chapter.id],
                )?;
                continue;
            }
            let fetched = if chapter.restricted {
                Err(Error::Restricted(
                    "官网目录标记网页受限，不保存截断预览".into(),
                ))
            } else {
                source.fetch_chapter(&remote_id, &chapter, cancelled)
            };
            if cancelled.load(Ordering::Acquire) || matches!(fetched, Err(Error::Cancelled)) {
                return self.finish_cancel(job_id);
            }
            match fetched {
                Ok(content) => {
                    if content.id != chapter.id {
                        return Err(Error::SourceChanged("正文身份与任务不一致".into()));
                    }
                    let content = validate_content(&remote_id, content)?;
                    let mut connection = self.connection()?;
                    let transaction = connection.transaction()?;
                    if cancelled.load(Ordering::Acquire) {
                        drop(transaction);
                        drop(connection);
                        return self.finish_cancel(job_id);
                    }
                    transaction.execute("INSERT INTO chapters (book_id,chapter_id,position,title,paragraphs) VALUES (?1,?2,?3,?4,?5) ON CONFLICT(book_id,chapter_id) DO NOTHING",params![book_id,chapter.id,position,content.title,serde_json::to_string(&content.paragraphs)?])?;
                    transaction.execute(
                        "UPDATE job_items SET state='completed' WHERE job_id=?1 AND chapter_id=?2",
                        params![job_id, chapter.id],
                    )?;
                    transaction.commit()?;
                }
                Err(error) => {
                    let state = if matches!(error, Error::Restricted(_)) {
                        "blocked"
                    } else {
                        "failed"
                    };
                    let message = error.to_string();
                    self.connection()?.execute(
                        "UPDATE job_items SET state=?3 WHERE job_id=?1 AND chapter_id=?2",
                        params![job_id, chapter.id, state],
                    )?;
                    last_error = Some(format!("{}：{message}", chapter.title));
                    // A rate limit or changed page can affect every chapter; stop the run instead of hammering the source.
                    if !matches!(error, Error::Restricted(_)) {
                        break;
                    }
                }
            }
        }
        let snapshot = self.job(job_id)?;
        let state = if snapshot.saved == snapshot.total {
            "completed"
        } else if snapshot.saved > 0 {
            "partial"
        } else if snapshot.blocked > 0 && snapshot.failed == 0 {
            "blocked"
        } else {
            "failed"
        };
        let error = if state == "completed" {
            None
        } else {
            Some(last_error.unwrap_or_else(|| {
                format!(
                    "已保存 {} / {} 章；受限 {} 章，失败 {} 章。",
                    snapshot.saved, snapshot.total, snapshot.blocked, snapshot.failed
                )
            }))
        };
        self.connection()?.execute(
            "UPDATE jobs SET state=?2,error=?3 WHERE id=?1 AND state='running'",
            params![job_id, state, error],
        )?;
        Ok(())
    }

    fn finish_cancel(&self, job_id: &str) -> Result<()> {
        self.connection()?.execute(
            "UPDATE jobs SET state='cancelled',error=NULL WHERE id=?1 AND state='running'",
            [job_id],
        )?;
        Ok(())
    }
}

fn validate_content(book_id: &str, chapter: ImportChapter) -> Result<ImportChapter> {
    let input = ImportDocument {
        id: book_id.into(),
        title: "正文校验".into(),
        author: String::new(),
        chapters: vec![chapter],
    };
    Ok(ImportDocument::parse(&serde_json::to_string(&input)?)?
        .chapters
        .remove(0))
}
