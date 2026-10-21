use crate::{
    Error, ImportChapter, ImportDocument, JobDetails, JobItemDetail, JobSnapshot, Library, Result,
    sources::{Catalog, ChapterRef, SourceAdapter},
};
use rusqlite::{OptionalExtension, params};
use std::sync::atomic::{AtomicBool, Ordering};
use uuid::Uuid;

impl Library {
    /// Persist the catalog and selected remote chapter identities before any text request.
    pub fn create_download(&self, catalog: &Catalog, first: usize, last: usize) -> Result<String> {
        if first == 0 || last < first || last > catalog.chapters.len() {
            return Err(Error::Invalid("章节范围超出目录；序号从 1 开始".into()));
        }
        self.create_download_selected(catalog, &(first..=last).collect::<Vec<_>>())
    }

    pub fn create_download_selected(
        &self,
        catalog: &Catalog,
        positions: &[usize],
    ) -> Result<String> {
        self.create_download_linked(catalog, positions, None)
    }
    pub(crate) fn create_download_linked(
        &self,
        catalog: &Catalog,
        positions: &[usize],
        batch: Option<(&str, usize, u64)>,
    ) -> Result<String> {
        catalog.validate()?;
        if !crate::sources::download_supported(&catalog.source) {
            return Err(Error::Invalid(
                "该来源尚未通过正文下载验收，不能创建下载任务".into(),
            ));
        }
        let selected: std::collections::BTreeSet<_> = positions.iter().copied().collect();
        if selected.is_empty()
            || selected.len() != positions.len()
            || selected
                .iter()
                .any(|p| *p == 0 || *p > catalog.chapters.len())
        {
            return Err(Error::Invalid("章节选择为空、重复或超出目录".into()));
        }
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        if let Some((batch_id, position, generation)) = batch {
            let input:Option<String>=transaction.query_row("SELECT i.input FROM batch_items i JOIN batches b ON b.id=i.batch_id WHERE b.id=?1 AND i.position=?2 AND b.state='running' AND b.generation=?3 AND i.state='queued' AND i.job_id IS NULL",params![batch_id,position,generation],|r|r.get(0)).optional()?;
            let input =
                input.ok_or_else(|| Error::Conflict("批次已停止或该项已创建任务".into()))?;
            let (source, id) = crate::sources::identify(&input)?;
            if source != catalog.source || id != catalog.remote_id {
                return Err(Error::SourceChanged("批次目录与请求作品身份不一致".into()));
            }
        }
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
            if catalog.chapters.len() < previous.chapters.len()
                || !previous.chapters.iter().map(|c| &c.id).eq(catalog
                    .chapters
                    .iter()
                    .take(previous.chapters.len())
                    .map(|c| &c.id))
            {
                return Err(Error::Conflict(
                    "官网目录包含删章、插章或重排，暂停更新以保护已保存内容；目前仅支持末尾增章"
                        .into(),
                ));
            }
            let active:u32=transaction.query_row("SELECT COUNT(*) FROM jobs WHERE book_id=?1 AND state IN ('queued','running','paused')",[&book_id],|row|row.get(0))?;
            if active > 0 {
                return Err(Error::Conflict(
                    "该作品还有待执行或中断任务，请先继续或取消原任务".into(),
                ));
            }
            let old_raw = serde_json::to_string(&previous)?;
            if old_raw != serde_json::to_string(catalog)? {
                crate::revisions::snapshot(&transaction, &book_id, &old_raw, "catalog_update")?;
            }
            transaction.execute(
                "UPDATE books SET title=?2,author=?3,total=?4 WHERE id=?1",
                params![
                    book_id,
                    catalog.title,
                    catalog.author,
                    catalog.chapters.len()
                ],
            )?;
            transaction.execute(
                "UPDATE source_catalogs SET catalog=?2 WHERE book_id=?1",
                params![book_id, serde_json::to_string(catalog)?],
            )?;
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
            .filter(|(position, _)| selected.contains(&(position + 1)))
        {
            let saved: bool = transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM chapters WHERE book_id=?1 AND chapter_id=?2)",
                params![book_id, chapter.id],
                |row| row.get(0),
            )?;
            let state = if saved { "completed" } else { "queued" };
            transaction.execute("INSERT INTO job_items (job_id,chapter_id,position,payload,state) VALUES (?1,?2,?3,?4,?5)",params![job_id,chapter.id,position,serde_json::to_string(chapter)?,state])?;
        }
        if let Some((batch_id, position, _)) = batch {
            transaction.execute("UPDATE batch_items SET job_id=?3,state='running',error=NULL WHERE batch_id=?1 AND position=?2",params![batch_id,position,job_id])?;
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
        if self.active_batch_for_job(job_id)?.is_some() {
            return Err(Error::Conflict("任务由批次管理，请继续原批次".into()));
        }
        self.run_batch_job(job_id, source, cancelled)
    }
    pub(crate) fn run_batch_job(
        &self,
        job_id: &str,
        source: &dyn SourceAdapter,
        cancelled: &AtomicBool,
    ) -> Result<JobSnapshot> {
        self.run_selected(job_id, source, cancelled, None)
    }

    /// Retry only named failed items; pending chapters outside this selection remain untouched.
    pub fn retry_chapters(
        &self,
        job_id: &str,
        source: &dyn SourceAdapter,
        cancelled: &AtomicBool,
        chapters: &[String],
    ) -> Result<JobSnapshot> {
        if self.active_batch_for_job(job_id)?.is_some() {
            return Err(Error::Conflict("任务由批次管理，请先完成或取消批次".into()));
        }
        if chapters.is_empty()
            || chapters.len() > crate::MAX_CHAPTERS
            || chapters
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len()
                != chapters.len()
        {
            return Err(Error::Invalid("请选择不重复的失败章节".into()));
        }
        self.run_selected(job_id, source, cancelled, Some(chapters))
    }

    pub fn job_details(&self, job_id: &str) -> Result<JobDetails> {
        let job = self.job(job_id)?;
        let connection = self.connection()?;
        let mut query = connection.prepare("SELECT i.chapter_id,i.position,COALESCE(hc.title,c.title,CASE WHEN json_valid(i.payload) THEN json_extract(i.payload,'$.title') END,'未标注章节'),i.state,i.error,i.error_kind
            FROM job_items i JOIN jobs j ON j.id=i.job_id LEFT JOIN chapters c ON c.book_id=j.book_id AND c.chapter_id=i.chapter_id LEFT JOIN job_history jh ON jh.job_id=j.id LEFT JOIN history_chapters hc ON hc.snapshot_id=jh.snapshot_id AND hc.chapter_id=i.chapter_id
            WHERE i.job_id=?1 ORDER BY i.position")?;
        let items = query
            .query_map([job_id], |row| {
                let state: String = row.get(3)?;
                Ok(JobItemDetail {
                    chapter_id: row.get(0)?,
                    position: row.get::<_, u32>(1)? + 1,
                    title: row.get(2)?,
                    retryable: job.source != "local-json"
                        && state == "failed"
                        && job.state != "running",
                    state,
                    error: row.get(4)?,
                    error_kind: row.get(5)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(JobDetails { job, items })
    }

    fn run_selected(
        &self,
        job_id: &str,
        source: &dyn SourceAdapter,
        cancelled: &AtomicBool,
        selection: Option<&[String]>,
    ) -> Result<JobSnapshot> {
        let job = self.job(job_id)?;
        if job.source == "local-json" {
            if selection.is_some() {
                return Err(Error::Invalid("本地导入请使用继续任务".into()));
            }
            return self.run_import(job_id, cancelled);
        }
        if job.source != source.source_id() {
            return Err(Error::Invalid("该任务需要其他来源适配器".into()));
        }
        let selected = selection.map(serde_json::to_string).transpose()?;
        // Validate selection, claim the task, and reset retry items in one transaction.
        {
            let mut connection = self.connection()?;
            let transaction = connection.transaction()?;
            let current: String =
                transaction
                    .query_row("SELECT state FROM jobs WHERE id=?1", [job_id], |r| r.get(0))?;
            if cancelled.load(Ordering::Acquire)
                && matches!(current.as_str(), "paused" | "cancelled")
            {
                drop(transaction);
                drop(connection);
                return self.job(job_id);
            }
            if let Some(chapters) = selection {
                let count:usize=transaction.query_row("SELECT COUNT(*) FROM job_items WHERE job_id=?1 AND state='failed' AND chapter_id IN (SELECT value FROM json_each(?2))",params![job_id,selected],|row|row.get(0))?;
                if count != chapters.len() {
                    return Err(Error::Conflict(
                        "只能重试该任务中的失败章节；受限章节请先检查官网目录更新".into(),
                    ));
                }
            }
            let changed=transaction.execute("UPDATE jobs SET state='running',error=NULL WHERE id=?1 AND state IN ('queued','paused','cancelled','failed','partial','blocked') AND NOT EXISTS (SELECT 1 FROM jobs other WHERE other.book_id=jobs.book_id AND other.id!=jobs.id AND other.state IN ('queued','running','paused'))",[job_id])?;
            if changed != 1 {
                return Err(Error::Conflict(
                    "任务不可执行，或该作品有其他待执行任务".into(),
                ));
            }
            transaction.execute("UPDATE job_items SET state='queued',error=NULL,error_kind=NULL WHERE job_id=?1 AND state='failed' AND (?2 IS NULL OR chapter_id IN (SELECT value FROM json_each(?2)))",params![job_id,selected])?;
            transaction.commit()?;
        }
        let result = self.download_items(job_id, source, cancelled, selected.as_deref());
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
        selected: Option<&str>,
    ) -> Result<()> {
        let mut last_error = None::<String>;
        loop {
            if !self.is_running(job_id)? {
                return Ok(());
            }
            if cancelled.load(Ordering::Acquire) {
                return self.finish_cancel(job_id);
            }
            let next = {
                let connection = self.connection()?;
                connection.query_row("SELECT i.payload,i.position,j.book_id,b.remote_id FROM job_items i JOIN jobs j ON i.job_id=j.id JOIN books b ON j.book_id=b.id WHERE i.job_id=?1 AND i.state='queued' AND (?2 IS NULL OR i.chapter_id IN (SELECT value FROM json_each(?2))) ORDER BY i.position LIMIT 1",params![job_id,selected],|row|Ok((row.get::<_,String>(0)?,row.get::<_,u32>(1)?,row.get::<_,String>(2)?,row.get::<_,String>(3)?))).optional()?
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
                    "UPDATE job_items SET state='completed',error=NULL,error_kind=NULL WHERE job_id=?1 AND chapter_id=?2",
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
            }
            .and_then(|content| {
                if content.id != chapter.id {
                    return Err(Error::SourceChanged("正文身份与任务不一致".into()));
                }
                validate_content(&remote_id, content)
            });
            if cancelled.load(Ordering::Acquire) || matches!(fetched, Err(Error::Cancelled)) {
                return self.finish_cancel(job_id);
            }
            match fetched {
                Ok(content) => {
                    let mut connection = self.connection()?;
                    let transaction = connection.transaction()?;
                    let state: String = transaction.query_row(
                        "SELECT state FROM jobs WHERE id=?1",
                        [job_id],
                        |r| r.get(0),
                    )?;
                    if state != "running" {
                        return Ok(());
                    }
                    if cancelled.load(Ordering::Acquire) {
                        drop(transaction);
                        drop(connection);
                        return self.finish_cancel(job_id);
                    }
                    transaction.execute("INSERT INTO chapters (book_id,chapter_id,position,title,paragraphs) VALUES (?1,?2,?3,?4,?5) ON CONFLICT(book_id,chapter_id) DO NOTHING",params![book_id,chapter.id,position,content.title,serde_json::to_string(&content.paragraphs)?])?;
                    transaction.execute(
                        "UPDATE job_items SET state='completed',error=NULL,error_kind=NULL WHERE job_id=?1 AND chapter_id=?2",
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
                        "UPDATE job_items SET state=?3,error=?4,error_kind=?5 WHERE job_id=?1 AND chapter_id=?2",
                        params![job_id, chapter.id, state,message,error_kind(&error)],
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

fn error_kind(error: &Error) -> &'static str {
    match error {
        Error::Network(_) => "network",
        Error::Restricted(_) => "restricted",
        Error::SourceChanged(_) => "source_changed",
        Error::Invalid(_) | Error::Json(_) => "invalid_content",
        Error::Io(_) => "io",
        _ => "other",
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
