use crate::{Error, Library, Result};
use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::OpenOptions,
    io::{Read, Write},
    path::Path,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BookPreferences {
    pub book_id: String,
    pub tags: Vec<String>,
    pub archived: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackupManifest {
    pub format: u32,
    pub application: String,
    pub version: String,
    pub schema: u32,
    pub books: u64,
    pub chapters: u64,
    pub sha256: String,
    pub created_at: String,
    pub export_policy: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticReport {
    pub version: String,
    pub operating_system: String,
    pub architecture: String,
    pub schema: u32,
    pub books: u64,
    pub saved_chapters: u64,
    pub jobs: u64,
    pub job_states: BTreeMap<String, u64>,
    pub integrity_ok: bool,
    pub foreign_keys_ok: bool,
}
impl DiagnosticReport {
    pub fn save(&self, path: impl AsRef<Path>) -> Result<()> {
        write_new(path.as_ref(), &serde_json::to_vec_pretty(self)?)
    }
}
fn count(db: &Connection, table: &str) -> Result<u64> {
    // All table identifiers are internal constants, never user input.
    Ok(db.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))?)
}
fn check(db: &Connection) -> Result<()> {
    let result: String = db.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
    let broken: Option<String> = db
        .query_row("PRAGMA foreign_key_check", [], |r| r.get(0))
        .optional()?;
    if result != "ok" || broken.is_some() {
        return Err(Error::Invalid("备份数据库完整性或外键校验失败".into()));
    }
    Ok(())
}
fn hash_file(path: &Path) -> Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hash = Sha256::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut f = OpenOptions::new().write(true).create_new(true).open(path)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    Ok(())
}
impl Library {
    pub fn preferences(&self) -> Result<Vec<BookPreferences>> {
        let db = self.connection()?;
        let values = db
            .prepare("SELECT book_id,tags,archived FROM book_preferences ORDER BY book_id")?
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, bool>(2)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        values
            .into_iter()
            .map(|(book_id, tags, archived)| {
                Ok(BookPreferences {
                    book_id,
                    tags: serde_json::from_str(&tags)?,
                    archived,
                })
            })
            .collect()
    }
    pub fn organize(
        &self,
        book_id: &str,
        tags: &[String],
        archived: bool,
    ) -> Result<BookPreferences> {
        if tags.len() > 20 {
            return Err(Error::Invalid("每本最多 20 个标签".into()));
        }
        let mut clean = Vec::new();
        for tag in tags {
            let tag = tag.trim();
            if tag.is_empty() || tag.chars().count() > 40 || tag.chars().any(|c| c.is_control()) {
                return Err(Error::Invalid("标签须为 1～40 个可见字符".into()));
            }
            if !clean
                .iter()
                .any(|s: &String| s.to_lowercase() == tag.to_lowercase())
            {
                clean.push(tag.to_owned());
            }
        }
        let db = self.connection()?;
        let exists: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM books WHERE id=?1)",
            [book_id],
            |r| r.get(0),
        )?;
        if !exists {
            return Err(Error::Invalid("作品不存在".into()));
        }
        db.execute("INSERT INTO book_preferences(book_id,tags,archived) VALUES(?1,?2,?3) ON CONFLICT(book_id) DO UPDATE SET tags=excluded.tags,archived=excluded.archived",params![book_id,serde_json::to_string(&clean)?,archived])?;
        Ok(BookPreferences {
            book_id: book_id.into(),
            tags: clean,
            archived,
        })
    }
    /// A bundle is database + verified manifest. Export records remain, generated files can be
    /// regenerated from saved chapters; credentials and external export files are never bundled.
    pub fn backup(&self, directory: impl AsRef<Path>) -> Result<BackupManifest> {
        let dir = directory.as_ref();
        std::fs::create_dir(dir)?;
        let path = dir.join("library.db");
        let db = self.connection()?;
        db.backup(rusqlite::MAIN_DB, &path, None)?;
        let saved = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        check(&saved)?;
        let schema = saved.pragma_query_value(None, "user_version", |r| r.get(0))?;
        let books = count(&saved, "books")?;
        let chapters = count(&saved, "chapters")?;
        drop(saved);
        let manifest=BackupManifest{format:1,application:"OpenNovel Downloader".into(),version:env!("CARGO_PKG_VERSION").into(),schema,books,chapters,sha256:hash_file(&path)?,created_at:chrono::Utc::now().to_rfc3339(),export_policy:"Database and export records included; exported files are not copied. Regenerate in the restored library. Credentials are excluded.".into()};
        write_new(
            &dir.join("manifest.json"),
            &serde_json::to_vec_pretty(&manifest)?,
        )?;
        Ok(manifest)
    }
    /// Restore only into a newly created directory, never into the current user library.
    pub fn restore(backup: impl AsRef<Path>, target: impl AsRef<Path>) -> Result<Self> {
        let dir = backup.as_ref();
        let target = target.as_ref();
        let manifest_path = dir.join("manifest.json");
        if std::fs::metadata(&manifest_path)?.len() > 16384 {
            return Err(Error::Invalid("备份清单过大".into()));
        }
        let manifest: BackupManifest = serde_json::from_slice(&std::fs::read(manifest_path)?)?;
        if manifest.format != 1
            || manifest.application != "OpenNovel Downloader"
            || !(1..=6).contains(&manifest.schema)
        {
            return Err(Error::Invalid("备份格式或版本不受支持".into()));
        }
        let source = dir.join("library.db");
        if std::fs::metadata(&source)?.len() > 1024 * 1024 * 1024 {
            return Err(Error::Invalid("备份超过 1 GiB 恢复预算".into()));
        }
        // create_dir is the no-overwrite boundary, including nonempty directories and symlinks.
        std::fs::create_dir(target)?;
        let path = target.join("library.db");
        let mut from = std::fs::File::open(source)?;
        let mut to = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        std::io::copy(&mut from, &mut to)?;
        to.sync_all()?;
        drop(to);
        if hash_file(&path)? != manifest.sha256 {
            return Err(Error::Invalid("备份哈希不符，目标副本未打开".into()));
        }
        let db = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        check(&db)?;
        let schema: u32 = db.pragma_query_value(None, "user_version", |r| r.get(0))?;
        if schema != manifest.schema
            || count(&db, "books")? != manifest.books
            || count(&db, "chapters")? != manifest.chapters
        {
            return Err(Error::Invalid("备份清单与数据库不一致".into()));
        }
        drop(db);
        Self::open(path)
    }
    pub fn diagnostics(&self) -> Result<DiagnosticReport> {
        let db = self.connection()?;
        let mut states = BTreeMap::new();
        for row in db
            .prepare("SELECT state,COUNT(*) FROM jobs GROUP BY state")?
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, u64>(1)?)))?
        {
            let (state, n) = row?;
            let label = if [
                "queued",
                "running",
                "paused",
                "cancelled",
                "failed",
                "partial",
                "blocked",
                "completed",
            ]
            .contains(&state.as_str())
            {
                state
            } else {
                "unknown".into()
            };
            *states.entry(label).or_insert(0) += n;
        }
        let integrity: String = db.query_row("PRAGMA quick_check", [], |r| r.get(0))?;
        let foreign: Option<String> = db
            .query_row("PRAGMA foreign_key_check", [], |r| r.get(0))
            .optional()?;
        Ok(DiagnosticReport {
            version: env!("CARGO_PKG_VERSION").into(),
            operating_system: std::env::consts::OS.into(),
            architecture: std::env::consts::ARCH.into(),
            schema: db.pragma_query_value(None, "user_version", |r| r.get(0))?,
            books: count(&db, "books")?,
            saved_chapters: count(&db, "chapters")?,
            jobs: count(&db, "jobs")?,
            job_states: states,
            integrity_ok: integrity == "ok",
            foreign_keys_ok: foreign.is_none(),
        })
    }
    pub fn export_diagnostics(&self, path: impl AsRef<Path>) -> Result<DiagnosticReport> {
        let report = self.diagnostics()?;
        write_new(path.as_ref(), &serde_json::to_vec_pretty(&report)?)?;
        Ok(report)
    }
}
