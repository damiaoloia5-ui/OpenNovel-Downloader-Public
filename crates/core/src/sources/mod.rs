mod fanqie;
mod jinjiang;
mod page;
mod qimao;
mod registry;
mod shuqi;
mod web;
pub use fanqie::Fanqie;
pub use jinjiang::Jinjiang;
pub use qimao::Qimao;
pub use registry::{SourceHub, SourceInfo, download_supported, identify, source_infos};
pub use shuqi::Shuqi;

use crate::{Error, ImportChapter, Result};
use serde::{Deserialize, Serialize};
use std::sync::atomic::AtomicBool;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChapterRef {
    pub id: String,
    pub title: String,
    pub restricted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Catalog {
    pub source: String,
    pub remote_id: String,
    pub title: String,
    pub author: String,
    #[serde(default)]
    pub description: String,
    pub chapters: Vec<ChapterRef>,
}

impl Catalog {
    pub fn validate(&self) -> Result<()> {
        let mut identities = std::collections::HashSet::new();
        if self.title.trim().is_empty()
            || self.chapters.is_empty()
            || self.chapters.len() > crate::MAX_CHAPTERS
        {
            return Err(Error::SourceChanged(
                "书名或目录缺失，或者目录超过 10000 章限制".into(),
            ));
        }
        for chapter in &self.chapters {
            if chapter.id.is_empty()
                || chapter.title.trim().is_empty()
                || !identities.insert(&chapter.id)
            {
                return Err(Error::SourceChanged("章节身份重复或标题缺失".into()));
            }
        }
        Ok(())
    }
}

pub trait SourceAdapter: Send + Sync {
    fn source_id(&self) -> &str;
    fn fetch_chapter(
        &self,
        book_id: &str,
        chapter: &ChapterRef,
        cancelled: &AtomicBool,
    ) -> Result<ImportChapter>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub source: String,
    pub remote_id: String,
    pub title: String,
    pub author: String,
    pub input: String,
}

pub trait SourceProvider: Send + Sync {
    fn inspect(&self, input: &str) -> Result<Catalog>;
    fn adapter(&self, source: &str) -> Result<&dyn SourceAdapter>;
}
impl SourceProvider for SourceHub {
    fn inspect(&self, input: &str) -> Result<Catalog> {
        SourceHub::inspect(self, input)
    }
    fn adapter(&self, source: &str) -> Result<&dyn SourceAdapter> {
        SourceHub::adapter(self, source)
    }
}
