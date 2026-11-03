use crate::{BookSummary, Error, Library, Result};
use rusqlite::OptionalExtension;
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Serialize)]
pub struct ExportDestination {
    pub directory: String,
    pub custom: bool,
    pub filename: String,
}

// Limit each part in UTF-16 units as well as removing Windows-invalid characters.
// The platform prefix means reserved bare device names can never be filenames.
pub(crate) fn filename_part(value: &str, fallback: &str, limit: usize) -> String {
    let mut result = String::new();
    let mut units = 0;
    for c in value.trim().chars() {
        if c.is_control() || "<>:\"/\\|?*".contains(c) {
            continue;
        }
        units += c.len_utf16();
        if units > limit || result.len() + c.len_utf8() > limit * 2 {
            break;
        }
        result.push(c);
    }
    let result = result.trim().trim_end_matches([' ', '.']);
    if result.is_empty() {
        fallback.into()
    } else {
        result.into()
    }
}

pub(crate) fn book_filename(book: &BookSummary) -> String {
    format!(
        "{}－{}－{}",
        filename_part(book.platform_name(), "未知平台", 16),
        filename_part(&book.title, "未命名作品", 64),
        filename_part(&book.author, "佚名", 24)
    )
}

impl Library {
    pub fn export_destination(
        &self,
        book_id: &str,
        default_root: impl AsRef<Path>,
    ) -> Result<ExportDestination> {
        let book = self.book_summary(book_id)?;
        let filename = book_filename(&book);
        let directory: Option<String> = self
            .connection()?
            .query_row(
                "SELECT directory FROM export_directories WHERE book_id=?1",
                [book_id],
                |r| r.get(0),
            )
            .optional()?;
        let custom = directory.is_some();
        let directory = directory
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| default_root.as_ref().join(&filename));
        Ok(ExportDestination {
            directory: std::path::absolute(directory)?
                .to_string_lossy()
                .into_owned(),
            custom,
            filename: format!("{filename}.txt"),
        })
    }

    pub fn set_export_directory(&self, book_id: &str, directory: Option<&Path>) -> Result<()> {
        self.book_summary(book_id)?;
        if let Some(directory) = directory {
            if !directory.is_absolute() || !directory.is_dir() {
                return Err(Error::Invalid(
                    "请选择已存在的文件夹，或填写文件夹的完整路径".into(),
                ));
            }
            // Prove write access before persisting the selection; old preference
            // and exports survive a failure. The probe is removed on drop.
            let _probe = tempfile::NamedTempFile::new_in(directory)?;
            let path = std::path::absolute(directory)?;
            self.connection()?.execute("INSERT INTO export_directories(book_id,directory) VALUES (?1,?2) ON CONFLICT(book_id) DO UPDATE SET directory=excluded.directory", rusqlite::params![book_id,path.to_string_lossy()])?;
        } else {
            self.connection()?
                .execute("DELETE FROM export_directories WHERE book_id=?1", [book_id])?;
        }
        Ok(())
    }
}
