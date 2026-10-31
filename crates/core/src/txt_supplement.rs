//! Explicit local TXT supplementation, matched to a previously confirmed catalog.
use crate::{Error, ImportChapter, ImportDocument, Library, Result, sources::Catalog};
use rusqlite::{Connection, params};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize)]
pub struct TxtSupplementPreview {
    pub fingerprint: String,
    pub title: String,
    pub total: usize,
    pub already_saved: usize,
    pub first_title: String,
    pub last_title: String,
}

fn title_key(value: &str) -> String {
    value.split_whitespace().collect()
}

fn looks_like_heading(line: &str) -> bool {
    line.strip_prefix('第')
        .and_then(|s| s.split_once('章'))
        .is_some_and(|(number, _)| {
            !number.is_empty()
                && number.chars().count() <= 12
                && number
                    .chars()
                    .all(|c| c.is_ascii_digit() || "零〇一二三四五六七八九十百千万两".contains(c))
        })
}

fn prepare(
    db: &Connection,
    book_id: &str,
    text: &str,
) -> Result<(TxtSupplementPreview, Vec<ImportChapter>)> {
    if text.len() > crate::MAX_IMPORT_BYTES {
        return Err(Error::Invalid("TXT 超过 16 MiB".into()));
    }
    let (raw, remote_id): (String, String) = db.query_row(
        "SELECT c.catalog,b.remote_id FROM source_catalogs c JOIN books b ON b.id=c.book_id WHERE b.id=?1 AND b.source='fanqie-web'",
        [book_id], |r| Ok((r.get(0)?, r.get(1)?)),
    ).map_err(|_| Error::Invalid("请先打开番茄作品的已确认目录".into()))?;
    let catalog: Catalog = serde_json::from_str(&raw)?;
    catalog.validate()?;
    if catalog.source != "fanqie-web" || catalog.remote_id != remote_id {
        return Err(Error::Conflict("目录与作品身份不一致，未导入".into()));
    }
    let mut headings = HashMap::new();
    for (position, chapter) in catalog.chapters.iter().enumerate() {
        if headings
            .insert(title_key(&chapter.title), position)
            .is_some()
        {
            return Err(Error::Conflict("目录标题重复，不能安全匹配 TXT".into()));
        }
    }
    let mut chapters: Vec<ImportChapter> = Vec::new();
    for line in text.trim_start_matches('\u{feff}').lines() {
        let line = line.trim();
        if let Some(&position) = headings.get(&title_key(line)) {
            if position != chapters.len() {
                return Err(Error::Invalid(
                    "TXT 章节缺失、重复或顺序与目录不同，未导入".into(),
                ));
            }
            let chapter = &catalog.chapters[position];
            chapters.push(ImportChapter {
                id: chapter.id.clone(),
                title: chapter.title.clone(),
                paragraphs: Vec::new(),
            });
        } else if looks_like_heading(line) {
            return Err(Error::Invalid(
                "TXT 含目录之外的章节标题，请更新目录或核对文件，未导入".into(),
            ));
        } else if !line.is_empty()
            && let Some(chapter) = chapters.last_mut()
        {
            chapter.paragraphs.push(line.to_owned());
        }
    }
    if chapters.len() != catalog.chapters.len() {
        return Err(Error::Invalid(format!(
            "TXT 仅匹配 {} / {} 章；需完整、顺序一致的目录标题，未导入",
            chapters.len(),
            catalog.chapters.len()
        )));
    }
    let document = ImportDocument {
        id: catalog.remote_id,
        title: catalog.title.clone(),
        author: catalog.author,
        chapters,
    }
    .validated()?;
    let already_saved = db.query_row(
        "SELECT COUNT(*) FROM chapters WHERE book_id=?1",
        [book_id],
        |r| r.get(0),
    )?;
    let fingerprint = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&(book_id, &raw, text))?)
    );
    let preview = TxtSupplementPreview {
        fingerprint,
        title: catalog.title,
        total: document.chapters.len(),
        already_saved,
        first_title: document.chapters[0].title.clone(),
        last_title: document
            .chapters
            .last()
            .expect("validated chapters")
            .title
            .clone(),
    };
    Ok((preview, document.chapters))
}

impl Library {
    /// Preview is read-only; a matching heading is not proof of provenance or completeness.
    pub fn preview_txt_supplement(
        &self,
        book_id: &str,
        text: &str,
    ) -> Result<TxtSupplementPreview> {
        let db = self.connection()?;
        prepare(&db, book_id, text).map(|(preview, _)| preview)
    }

    /// Recheck the exact book, catalog and text inside the transaction; never overwrite a body.
    pub fn confirm_txt_supplement(
        &self,
        book_id: &str,
        text: &str,
        fingerprint: &str,
    ) -> Result<TxtSupplementPreview> {
        let mut db = self.connection()?;
        let tx = db.transaction()?;
        let (preview, chapters) = prepare(&tx, book_id, text)?;
        if preview.fingerprint != fingerprint {
            return Err(Error::Conflict("文件或目录已变化，请重新预览确认".into()));
        }
        let active: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM jobs WHERE book_id=?1 AND state IN ('running','queued','paused'))", [book_id], |r| r.get(0))?;
        if active {
            return Err(Error::Conflict(
                "请先结束或取消本书正在运行或暂停的任务，再导入 TXT".into(),
            ));
        }
        if preview.already_saved < preview.total {
            let job = uuid::Uuid::new_v4().to_string();
            tx.execute(
                "INSERT INTO jobs(id,book_id,state) VALUES(?1,?2,'completed')",
                params![job, book_id],
            )?;
            for (position, chapter) in chapters.iter().enumerate() {
                let inserted = tx.execute("INSERT INTO chapters(book_id,chapter_id,position,title,paragraphs) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(book_id,chapter_id) DO NOTHING", params![book_id,chapter.id,position,chapter.title,serde_json::to_string(&chapter.paragraphs)?])?;
                let origin = if inserted == 1 {
                    "user-txt"
                } else {
                    "existing-cache"
                };
                tx.execute("INSERT INTO job_items(job_id,chapter_id,position,payload,state) VALUES(?1,?2,?3,?4,'completed')", params![job,chapter.id,position,serde_json::to_string(&serde_json::json!({"title":chapter.title,"origin":origin}))?])?;
            }
        }
        tx.commit()?;
        Ok(preview)
    }
}
