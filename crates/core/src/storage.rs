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
        if version > 1 {
            return Err(Error::Conflict("书库来自较新版本，请使用匹配的程序".into()));
        }
        connection.execute_batch(
            "PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;",
        )?;
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
        connection.execute(
            "UPDATE jobs SET state='paused', error='程序中断，等待继续' WHERE state='running'",
            [],
        )?;
        Ok(Self {
            connection: Mutex::new(connection),
            _lock: lock,
        })
    }

    fn connection(&self) -> Result<std::sync::MutexGuard<'_, Connection>> {
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
        self.start_job(job_id)?;
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

    fn start_job(&self, job_id: &str) -> Result<()> {
        let connection = self.connection()?;
        let changed = connection.execute(
            "UPDATE jobs SET state='running',error=NULL WHERE id=?1 AND state IN ('queued','paused','cancelled','failed')",
            [job_id],
        )?;
        if changed != 1 {
            return Err(Error::Conflict("任务不存在、正在执行或已经完成".into()));
        }
        Ok(())
    }

    pub fn job(&self, job_id: &str) -> Result<JobSnapshot> {
        self.snapshot()?
            .jobs
            .into_iter()
            .find(|job| job.id == job_id)
            .ok_or_else(|| Error::Invalid("任务不存在".into()))
    }

    pub fn snapshot(&self) -> Result<LibrarySnapshot> {
        let connection = self.connection()?;
        let mut books_query = connection.prepare(
            "SELECT b.id,b.title,b.author,b.source,COUNT(c.chapter_id),b.total FROM books b
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
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let mut jobs_query = connection.prepare(
            "SELECT j.id,j.book_id,b.title,j.state,
             SUM(CASE WHEN i.state='completed' THEN 1 ELSE 0 END),COUNT(i.chapter_id),j.error
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
                "SELECT b.id,b.title,b.author,b.source,COUNT(c.chapter_id),b.total FROM books b
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
        transaction.commit()?;
        if chapters.is_empty() {
            return Err(Error::Conflict("尚无已保存章节，无法导出".into()));
        }
        Ok(BookContent { summary, chapters })
    }
}
