use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const MAX_IMPORT_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_CHAPTERS: usize = 10_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportDocument {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub author: String,
    pub chapters: Vec<ImportChapter>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportChapter {
    pub id: String,
    pub title: String,
    pub paragraphs: Vec<String>,
}

impl ImportDocument {
    pub fn parse(json: &str) -> Result<Self> {
        if json.len() > MAX_IMPORT_BYTES {
            return Err(Error::Invalid("文件超过 16 MiB".into()));
        }
        let mut document: Self = serde_json::from_str(json)?;
        document.id = document.id.trim().to_owned();
        document.title = document.title.trim().to_owned();
        document.author = document.author.trim().to_owned();
        validate_label(&document.id, "作品 ID", 256)?;
        validate_label(&document.title, "书名", 500)?;
        if document.author.chars().count() > 500 || invalid_xml(&document.author) {
            return Err(Error::Invalid("作者字段过长或含无效字符".into()));
        }
        if document.chapters.is_empty() || document.chapters.len() > MAX_CHAPTERS {
            return Err(Error::Invalid("章节数量必须在 1～10000 之间".into()));
        }
        let mut identities = HashSet::new();
        for chapter in &mut document.chapters {
            chapter.id = chapter.id.trim().to_owned();
            chapter.title = chapter.title.trim().to_owned();
            validate_label(&chapter.id, "章节 ID", 256)?;
            validate_label(&chapter.title, "章节标题", 500)?;
            if !identities.insert(chapter.id.clone()) {
                return Err(Error::Invalid(format!("章节 ID 重复：{}", chapter.id)));
            }
            chapter.paragraphs = chapter
                .paragraphs
                .iter()
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty())
                .collect();
            if chapter.paragraphs.is_empty()
                || chapter.paragraphs.iter().any(|value| invalid_xml(value))
            {
                return Err(Error::Invalid(format!(
                    "章节正文为空或含无效字符：{}",
                    chapter.id
                )));
            }
        }
        Ok(document)
    }
}

fn validate_label(value: &str, name: &str, max: usize) -> Result<()> {
    if value.is_empty() || value.chars().count() > max || invalid_xml(value) {
        return Err(Error::Invalid(format!("{name}为空、过长或含无效字符")));
    }
    Ok(())
}

fn invalid_xml(value: &str) -> bool {
    value.chars().any(|ch| {
        !matches!(ch as u32, 0x9 | 0xA | 0xD | 0x20..=0xD7FF | 0xE000..=0xFFFD | 0x10000..=0x10FFFF)
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookSummary {
    pub id: String,
    pub title: String,
    pub author: String,
    pub source: String,
    pub remote_id: String,
    pub saved_chapters: u32,
    pub total_chapters: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobSnapshot {
    pub id: String,
    pub book_id: String,
    pub title: String,
    pub state: String,
    pub saved: u32,
    pub total: u32,
    pub error: Option<String>,
    pub source: String,
    pub blocked: u32,
    pub failed: u32,
}

impl BookSummary {
    pub fn source_label(&self) -> &str {
        match self.source.as_str() {
            "fanqie-web" => "番茄公开网页",
            "local-json" => "本地 JSON 导入",
            _ => &self.source,
        }
    }
    pub fn source_url(&self) -> Option<String> {
        (self.source == "fanqie-web")
            .then(|| format!("https://fanqienovel.com/page/{}", self.remote_id))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibrarySnapshot {
    pub books: Vec<BookSummary>,
    pub jobs: Vec<JobSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportReceipt {
    pub path: String,
    pub saved_chapters: u32,
    pub total_chapters: u32,
    pub partial: bool,
}

pub(crate) struct BookContent {
    pub summary: BookSummary,
    pub chapters: Vec<ImportChapter>,
}
