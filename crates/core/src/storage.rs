use crate::{
    BookContent, BookSummary, Error, ImportChapter, ImportDocument, JobSnapshot, LibrarySnapshot,
    Result,
};
use fs2::FileExt;
use rusqlite::{Connection, OptionalExtension, params};
use std::{
    fs::{File, OpenOptions},
    path::Path,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use uuid::Uuid;

pub struct Library {
    connection: Mutex<Connection>,
    _lock: File,
}

impl Library {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }
        let mut lock_name = path.as_os_str().to_owned();
        lock_name.push(".lock");
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(Path::new(&lock_name))?;
        lock.try_lock_exclusive().map_err(|_| Error::Locked)?;
        let mut connection = Connection::open(path)?;
        connection.busy_timeout(Duration::from_secs(5))?;
        let version: u32 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if version > 8 {
            return Err(Error::Conflict("书库来自较新版本，请使用匹配的程序".into()));
        }
        connection.execute_batch(
            "PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;",
        )?;
        if (1..8).contains(&version) {
            let backup =
                path.with_file_name(format!("library-v{version}-backup-{}.db", Uuid::new_v4()));
            let reserved = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&backup)?;
            drop(reserved);
            connection.backup(rusqlite::MAIN_DB, &backup, None)?;
        }
        if version == 0 {
            let transaction = connection.transaction()?;
            transaction.execute_batch(
                "CREATE TABLE books (
                    id TEXT PRIMARY KEY, source TEXT NOT NULL, remote_id TEXT NOT NULL,
                    title TEXT NOT NULL, author TEXT NOT NULL, total INTEGER NOT NULL,
                    UNIQUE(source, remote_id)
                );
                CREATE TABLE jobs (
                    id TEXT PRIMARY KEY, book_id TEXT NOT NULL REFERENCES books(id),
                    state TEXT NOT NULL, error TEXT, created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
                );
                CREATE TABLE job_items (
                    job_id TEXT NOT NULL REFERENCES jobs(id), chapter_id TEXT NOT NULL,
                    position INTEGER NOT NULL, payload TEXT, state TEXT NOT NULL,
                    PRIMARY KEY(job_id, chapter_id), UNIQUE(job_id, position)
                );
                CREATE TABLE chapters (
                    book_id TEXT NOT NULL REFERENCES books(id), chapter_id TEXT NOT NULL,
                    position INTEGER NOT NULL, title TEXT NOT NULL, paragraphs TEXT NOT NULL,
                    PRIMARY KEY(book_id, chapter_id), UNIQUE(book_id, position)
                );
                PRAGMA user_version=1;",
            )?;
            transaction.commit()?;
        }
        if version < 2 {
            let transaction = connection.transaction()?;
            transaction.execute_batch(
                "CREATE TABLE source_catalogs (book_id TEXT PRIMARY KEY REFERENCES books(id), catalog TEXT NOT NULL);
                 CREATE INDEX job_items_pending ON job_items(job_id,state,position);
                 PRAGMA user_version=2;"
            )?;
            transaction.commit()?;
        }
        if version < 3 {
            let transaction = connection.transaction()?;
            transaction.execute_batch(
                "ALTER TABLE job_items ADD COLUMN error TEXT;
                ALTER TABLE job_items ADD COLUMN error_kind TEXT;
                PRAGMA user_version=3;",
            )?;
            transaction.commit()?;
        }
        if version < 4 {
            let transaction = connection.transaction()?;
            transaction.execute_batch("CREATE TABLE exports (
                id TEXT PRIMARY KEY,book_id TEXT NOT NULL REFERENCES books(id),job_id TEXT REFERENCES jobs(id),
                path TEXT NOT NULL,format TEXT NOT NULL,saved INTEGER NOT NULL,total INTEGER NOT NULL,partial INTEGER NOT NULL,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP);
                PRAGMA user_version=4;")?;
            transaction.commit()?;
        }
        if version < 5 {
            let transaction = connection.transaction()?;
            transaction.execute_batch("CREATE TABLE book_preferences (
                book_id TEXT PRIMARY KEY REFERENCES books(id), tags TEXT NOT NULL, archived INTEGER NOT NULL DEFAULT 0 CHECK(archived IN (0,1))
            ); PRAGMA user_version=5;")?;
            transaction.commit()?;
        }
        if version < 6 {
            let transaction = connection.transaction()?;
            transaction.execute_batch("CREATE TABLE batches (
                id TEXT PRIMARY KEY,state TEXT NOT NULL,selection TEXT NOT NULL,duplicates INTEGER NOT NULL,generation INTEGER NOT NULL DEFAULT 0,error TEXT
            ); CREATE TABLE batch_items (
                batch_id TEXT NOT NULL REFERENCES batches(id),position INTEGER NOT NULL,input TEXT NOT NULL,
                state TEXT NOT NULL,job_id TEXT REFERENCES jobs(id),error TEXT,PRIMARY KEY(batch_id,position),UNIQUE(batch_id,input)
            ); PRAGMA user_version=6;")?;
            transaction.commit()?;
        }
        if version < 7 {
            let transaction = connection.transaction()?;
            transaction.execute_batch("CREATE TABLE content_blobs(hash TEXT PRIMARY KEY,paragraphs TEXT NOT NULL);
                CREATE TABLE catalog_history(id TEXT PRIMARY KEY,book_id TEXT NOT NULL REFERENCES books(id),catalog TEXT NOT NULL,reason TEXT NOT NULL,created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP);
                CREATE TABLE history_chapters(snapshot_id TEXT NOT NULL REFERENCES catalog_history(id),chapter_id TEXT NOT NULL,position INTEGER NOT NULL,title TEXT NOT NULL,body_hash TEXT NOT NULL REFERENCES content_blobs(hash),PRIMARY KEY(snapshot_id,chapter_id));
                CREATE TABLE job_history(job_id TEXT PRIMARY KEY REFERENCES jobs(id),snapshot_id TEXT NOT NULL REFERENCES catalog_history(id));
                PRAGMA user_version=7;")?;
            transaction.commit()?;
        }
        if version < 8 {
            let transaction = connection.transaction()?;
            transaction.execute_batch("CREATE TABLE accounts(id TEXT PRIMARY KEY,platform TEXT NOT NULL CHECK(platform IN ('qidian','shuqi')),label TEXT NOT NULL); PRAGMA user_version=8;")?;
            transaction.commit()?;
        }
        connection.execute(
            "UPDATE batches SET state='paused' WHERE state='running'",
            [],
        )?;
        connection.execute(
            "UPDATE jobs SET state='paused', error='程序中断，等待继续' WHERE state='running'",
            [],
        )?;
        Ok(Self {
            connection: Mutex::new(connection),
            _lock: lock,
        })
    }

    pub(crate) fn connection(&self) -> Result<std::sync::MutexGuard<'_, Connection>> {
        self.connection
            .lock()
            .map_err(|_| Error::Conflict("数据库锁失效，请重启程序".into()))
    }

    pub fn create_import(&self, json: &str) -> Result<String> {
        let document = ImportDocument::parse(json)?;
        let book_id = Uuid::new_v4().to_string();
        let job_id = Uuid::new_v4().to_string();
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let exists = transaction
            .query_row(
                "SELECT id FROM books WHERE source='local-json' AND remote_id=?1",
                [&document.id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        if exists.is_some() {
            return Err(Error::Conflict(
                "作品已在书库中；请继续原任务，避免覆盖已保存内容".into(),
            ));
        }
        transaction.execute(
            "INSERT INTO books (id,source,remote_id,title,author,total) VALUES (?1,'local-json',?2,?3,?4,?5)",
            params![book_id, document.id, document.title, document.author, document.chapters.len()],
        )?;
        transaction.execute(
            "INSERT INTO jobs (id,book_id,state) VALUES (?1,?2,'queued')",
            params![job_id, book_id],
        )?;
        for (position, chapter) in document.chapters.iter().enumerate() {
            transaction.execute(
                "INSERT INTO job_items (job_id,chapter_id,position,payload,state) VALUES (?1,?2,?3,?4,'queued')",
                params![job_id, chapter.id, position, serde_json::to_string(chapter)?],
            )?;
        }
        transaction.commit()?;
        Ok(job_id)
    }

    pub fn run_import(&self, job_id: &str, cancelled: &AtomicBool) -> Result<JobSnapshot> {
        self.run_import_observed(job_id, cancelled, |_| {})
    }

    /// The observer runs after each committed chapter, useful for deterministic failure tests.
    pub fn run_import_observed(
        &self,
        job_id: &str,
        cancelled: &AtomicBool,
        mut committed: impl FnMut(u32),
    ) -> Result<JobSnapshot> {
        if !self.start_job(job_id, cancelled)? {
            return self.job(job_id);
        }
        let result = self.run_items(job_id, cancelled, &mut committed);
        if let Err(error) = &result {
            self.connection()?.execute(
                "UPDATE jobs SET state='failed',error=?2 WHERE id=?1 AND state='running'",
                params![job_id, error.to_string()],
            )?;
        }
        result?;
        self.job(job_id)
    }

    fn run_items(
        &self,
        job_id: &str,
        cancelled: &AtomicBool,
        committed: &mut impl FnMut(u32),
    ) -> Result<()> {
        loop {
            if !self.is_running(job_id)? {
                return Ok(());
            }
            if cancelled.load(Ordering::Acquire) {
                self.connection()?.execute(
                    "UPDATE jobs SET state='cancelled' WHERE id=?1 AND state='running'",
                    [job_id],
                )?;
                return Ok(());
            }
            let next = {
                let connection = self.connection()?;
                connection.query_row(
                    "SELECT i.payload,i.position,j.book_id FROM job_items i JOIN jobs j ON j.id=i.job_id
                     WHERE i.job_id=?1 AND i.state='queued' ORDER BY i.position LIMIT 1",
                    [job_id], |row| Ok((row.get::<_, String>(0)?, row.get::<_, u32>(1)?, row.get::<_, String>(2)?)),
                ).optional()?
            };
            let Some((payload, position, book_id)) = next else {
                self.connection()?.execute(
                    "UPDATE jobs SET state='completed',error=NULL WHERE id=?1 AND state='running'",
                    [job_id],
                )?;
                return Ok(());
            };
            let chapter: ImportChapter = serde_json::from_str(&payload)?;
            {
                let mut connection = self.connection()?;
                let transaction = connection.transaction()?;
                let state: String =
                    transaction
                        .query_row("SELECT state FROM jobs WHERE id=?1", [job_id], |r| r.get(0))?;
                if state != "running" {
                    return Ok(());
                }
                if cancelled.load(Ordering::Acquire) {
                    transaction
                        .execute("UPDATE jobs SET state='cancelled' WHERE id=?1", [job_id])?;
                    transaction.commit()?;
                    return Ok(());
                }
                transaction.execute(
                    "INSERT INTO chapters (book_id,chapter_id,position,title,paragraphs) VALUES (?1,?2,?3,?4,?5)",
                    params![book_id, chapter.id, position, chapter.title, serde_json::to_string(&chapter.paragraphs)?],
                )?;
                transaction.execute(
                    "UPDATE job_items SET state='completed',payload=NULL WHERE job_id=?1 AND chapter_id=?2",
                    params![job_id, chapter.id],
                )?;
                transaction.commit()?;
            }
            committed(position + 1);
        }
    }

    pub(crate) fn start_job(&self, job_id: &str, cancelled: &AtomicBool) -> Result<bool> {
        let connection = self.connection()?;
        let current: String =
            connection.query_row("SELECT state FROM jobs WHERE id=?1", [job_id], |r| r.get(0))?;
        if cancelled.load(Ordering::Acquire) && matches!(current.as_str(), "paused" | "cancelled") {
            return Ok(false);
        }
        let changed = connection.execute(
            "UPDATE jobs SET state='running',error=NULL WHERE id=?1 AND state IN ('queued','paused','cancelled','failed','partial','blocked')
             AND NOT EXISTS (SELECT 1 FROM jobs other WHERE other.book_id=jobs.book_id AND other.id!=jobs.id AND other.state='running')",
            [job_id],
        )?;
        if changed != 1 {
            return Err(Error::Conflict("任务不存在、正在执行或已经完成".into()));
        }
        Ok(true)
    }

    pub fn pause_job(&self, job_id: &str) -> Result<()> {
        self.request_stop(job_id, true, None)
    }

    pub fn cancel_job(&self, job_id: &str) -> Result<()> {
        self.request_stop(job_id, false, None)
    }

    pub fn request_stop(
        &self,
        job_id: &str,
        paused: bool,
        token: Option<&AtomicBool>,
    ) -> Result<()> {
        if self.active_batch_for_job(job_id)?.is_some() {
            return Err(Error::Conflict(
                "任务由批次管理，请在批次面板停止整批".into(),
            ));
        }
        let connection = self.connection()?;
        let state = if paused { "paused" } else { "cancelled" };
        let changed=connection.execute("UPDATE jobs SET state=?2,error=NULL WHERE id=?1 AND (state IN ('queued','running') OR (?2='cancelled' AND state='paused'))",[job_id,state])?;
        if changed != 1 {
            return Err(Error::Conflict("任务不在可停止状态".into()));
        }
        if let Some(token) = token {
            token.store(true, Ordering::Release);
        }
        Ok(())
    }

    pub(crate) fn is_running(&self, job_id: &str) -> Result<bool> {
        Ok(self
            .connection()?
            .prepare_cached("SELECT state='running' FROM jobs WHERE id=?1")?
            .query_row([job_id], |r| r.get(0))?)
    }

    pub fn record_worker_failure(&self, job_id: &str, message: &str) -> Result<()> {
        self.connection()?.execute("UPDATE jobs SET state='failed',error=?2 WHERE id=?1 AND state IN ('queued','failed','partial','blocked')",[job_id,message])?;
        Ok(())
    }

    pub fn job(&self, job_id: &str) -> Result<JobSnapshot> {
        self.connection()?
            .query_row(
                "SELECT j.id,j.book_id,b.title,j.state,
             COALESCE(SUM(i.state='completed'),0),COUNT(i.chapter_id),j.error,b.source,
             COALESCE(SUM(i.state='blocked'),0),COALESCE(SUM(i.state='failed'),0)
             FROM jobs j JOIN books b ON b.id=j.book_id LEFT JOIN job_items i ON i.job_id=j.id
             WHERE j.id=?1 GROUP BY j.id",
                [job_id],
                |row| {
                    Ok(JobSnapshot {
                        id: row.get(0)?,
                        book_id: row.get(1)?,
                        title: row.get(2)?,
                        state: row.get(3)?,
                        saved: row.get(4)?,
                        total: row.get(5)?,
                        error: row.get(6)?,
                        source: row.get(7)?,
                        blocked: row.get(8)?,
                        failed: row.get(9)?,
                    })
                },
            )
            .optional()?
            .ok_or_else(|| Error::Invalid("任务不存在".into()))
    }

    pub fn book_summary(&self, book_id: &str) -> Result<BookSummary> {
        self.connection()?.query_row("SELECT b.id,b.title,b.author,b.source,(SELECT COUNT(*) FROM chapters c WHERE c.book_id=b.id),b.total,b.remote_id FROM books b WHERE b.id=?1",[book_id],|row|Ok(BookSummary{id:row.get(0)?,title:row.get(1)?,author:row.get(2)?,source:row.get(3)?,saved_chapters:row.get(4)?,total_chapters:row.get(5)?,remote_id:row.get(6)?})).optional()?.ok_or_else(||Error::Invalid("作品不存在".into()))
    }

    pub fn snapshot(&self) -> Result<LibrarySnapshot> {
        let connection = self.connection()?;
        let mut books_query = connection.prepare(
            "SELECT b.id,b.title,b.author,b.source,COUNT(c.chapter_id),b.total,b.remote_id FROM books b
             LEFT JOIN chapters c ON c.book_id=b.id GROUP BY b.id ORDER BY b.rowid DESC",
        )?;
        let books = books_query
            .query_map([], |row| {
                Ok(BookSummary {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    author: row.get(2)?,
                    source: row.get(3)?,
                    saved_chapters: row.get(4)?,
                    total_chapters: row.get(5)?,
                    remote_id: row.get(6)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let mut jobs_query = connection.prepare(
            "SELECT j.id,j.book_id,b.title,j.state,
             SUM(CASE WHEN i.state='completed' THEN 1 ELSE 0 END),COUNT(i.chapter_id),j.error,b.source,
             SUM(CASE WHEN i.state='blocked' THEN 1 ELSE 0 END),SUM(CASE WHEN i.state='failed' THEN 1 ELSE 0 END)
             FROM jobs j JOIN books b ON b.id=j.book_id JOIN job_items i ON i.job_id=j.id
             GROUP BY j.id ORDER BY j.rowid DESC",
        )?;
        let jobs = jobs_query
            .query_map([], |row| {
                Ok(JobSnapshot {
                    id: row.get(0)?,
                    book_id: row.get(1)?,
                    title: row.get(2)?,
                    state: row.get(3)?,
                    saved: row.get(4)?,
                    total: row.get(5)?,
                    error: row.get(6)?,
                    source: row.get(7)?,
                    blocked: row.get(8)?,
                    failed: row.get(9)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(LibrarySnapshot { books, jobs })
    }

    pub(crate) fn content(&self, book_id: &str) -> Result<BookContent> {
        let connection = self.connection()?;
        let transaction = connection.unchecked_transaction()?;
        let summary = transaction
            .query_row(
                "SELECT b.id,b.title,b.author,b.source,COUNT(c.chapter_id),b.total,b.remote_id FROM books b
             LEFT JOIN chapters c ON c.book_id=b.id WHERE b.id=?1 GROUP BY b.id",
                [book_id],
                |row| {
                    Ok(BookSummary {
                        id: row.get(0)?,
                        title: row.get(1)?,
                        author: row.get(2)?,
                        source: row.get(3)?,
                        saved_chapters: row.get(4)?,
                        total_chapters: row.get(5)?,
                        remote_id: row.get(6)?,
                    })
                },
            )
            .optional()?
            .ok_or_else(|| Error::Invalid("作品不存在".into()))?;
        let chapters = {
            let mut query = transaction.prepare("SELECT chapter_id,title,paragraphs FROM chapters WHERE book_id=?1 ORDER BY position")?;
            let raw = query
                .query_map([book_id], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            raw.into_iter()
                .map(|(id, title, body)| {
                    Ok(ImportChapter {
                        id,
                        title,
                        paragraphs: serde_json::from_str(&body)?,
                    })
                })
                .collect::<Result<Vec<_>>>()?
        };
        if chapters.is_empty() {
            return Err(Error::Conflict("尚无已保存章节，无法导出".into()));
        }
        let modified=transaction.query_row("SELECT MAX(moment) FROM (SELECT created_at AS moment FROM jobs WHERE book_id=?1 UNION ALL SELECT created_at AS moment FROM catalog_history WHERE book_id=?1)",[book_id],|r|r.get::<_,String>(0))?;
        transaction.commit()?;
        Ok(BookContent {
            summary,
            chapters,
            modified,
        })
    }
}
