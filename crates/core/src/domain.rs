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
        let document: Self = serde_json::from_str(json)?;
        document.normalize()
    }

    pub(crate) fn validated(self) -> Result<Self> {
        // Preserve the serialized 16 MiB budget without allocating and parsing
        // another full copy of an already typed chapter/document.
        struct Budget(usize);
        impl std::io::Write for Budget {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                self.0 = self.0.saturating_add(bytes.len());
                if self.0 > MAX_IMPORT_BYTES {
                    return Err(std::io::Error::other("size budget"));
                }
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        serde_json::to_writer(&mut Budget(0), &self)
            .map_err(|_| Error::Invalid("文件超过 16 MiB".into()))?;
        self.normalize()
    }

    fn normalize(mut self) -> Result<Self> {
        let document = &mut self;
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
        Ok(self)
    }
}

pub(crate) fn validate_label(value: &str, name: &str, max: usize) -> Result<()> {
    if value.is_empty() || value.chars().count() > max || invalid_xml(value) {
        return Err(Error::Invalid(format!("{name}为空、过长或含无效字符")));
    }
    Ok(())
}

#[cfg(test)]
mod typed_validation_tests {
    use super::*;
    #[test]
    fn typed_validation_matches_json_normalization_and_rejections() {
        let base = ImportDocument {
            id: " original ".into(),
            title: " 原创 ".into(),
            author: " 作者 ".into(),
            chapters: vec![ImportChapter {
                id: " c1 ".into(),
                title: " 一章 ".into(),
                paragraphs: vec![" ".into(), " 原创\\\"正文\n ".into()],
            }],
        };
        let typed = base.clone().validated().unwrap();
        let parsed = ImportDocument::parse(&serde_json::to_string(&base).unwrap()).unwrap();
        assert_eq!(
            serde_json::to_value(typed).unwrap(),
            serde_json::to_value(parsed).unwrap()
        );
        for paragraphs in [vec![], vec![" ".into()], vec!["正文\u{0001}".into()]] {
            let mut doc = base.clone();
            doc.chapters[0].paragraphs = paragraphs;
            assert!(doc.clone().validated().is_err());
            assert!(ImportDocument::parse(&serde_json::to_string(&doc).unwrap()).is_err());
        }
    }
    #[test]
    fn typed_validation_keeps_serialized_size_budget() {
        let d = ImportDocument {
            id: "original".into(),
            title: "原创".into(),
            author: String::new(),
            chapters: vec![ImportChapter {
                id: "1".into(),
                title: "原创章".into(),
                paragraphs: vec!["x".repeat(MAX_IMPORT_BYTES)],
            }],
        };
        assert!(d.validated().is_err());
    }
}

pub(crate) fn invalid_xml(value: &str) -> bool {
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
    pub fn platform_name(&self) -> &str {
        match self.source.as_str() {
            "fanqie-web" | "fanqie-app-local" => "番茄小说",
            "qimao-web" | "qimao-session" => "七猫小说",
            "shuqi-web" | "shuqi-session" => "书旗小说",
            "jinjiang-web" | "jinjiang-session" => "晋江文学城",
            "zongheng-web" | "zongheng-session" => "纵横中文网",
            "17k-web" | "17k-session" => "17K小说网",
            "qidian-session" => "起点中文网",
            "ciweimao-web" | "ciweimao-session" => "刺猬猫",
            "faloo-web" | "faloo-session" => "飞卢",
            "local-json" => "本地导入",
            "migu-web" => "咪咕悦看",
            _ => &self.source,
        }
    }
    pub fn source_label(&self) -> &str {
        if crate::session_platform(&self.source).is_some() {
            return "官网会话内可读正文（实验性）";
        }
        match self.source.as_str() {
            "migu-web" => "咪咕官网明确免费章节",
            "fanqie-web" => "番茄公开网页",
            "fanqie-app-local" => "番茄本地引擎（免费内容）",
            "qimao-web" | "qimao-session" => "七猫公开网页",
            "shuqi-web" | "shuqi-session" => "书旗官网免费章节",
            "jinjiang-web" | "jinjiang-session" => "晋江官网免费章节",
            "local-json" => "本地 JSON 导入",
            "zongheng-web" | "zongheng-session" => "纵横官网免费章节",
            "17k-web" | "17k-session" => "17K 官网免费章节",
            "qidian-session" => "起点当前可读章节（实验性）",
            _ => &self.source,
        }
    }
    pub fn source_url(&self) -> Option<String> {
        match self.source.as_str() {
            "migu-web" => Some(format!(
                "https://n.cmread.com/nap/p/wap_detail_new.jsp?bid={}",
                self.remote_id
            )),
            "zongheng-web" | "zongheng-session" => Some(format!(
                "https://www.zongheng.com/detail/{}",
                self.remote_id
            )),
            "17k-web" | "17k-session" => {
                Some(format!("https://www.17k.com/book/{}.html", self.remote_id))
            }
            "qidian-session" => Some(format!("https://www.qidian.com/book/{}/", self.remote_id)),
            "fanqie-web" | "fanqie-app-local" => {
                Some(format!("https://fanqienovel.com/page/{}", self.remote_id))
            }
            "qimao-web" | "qimao-session" => {
                Some(format!("https://www.qimao.com/shuku/{}/", self.remote_id))
            }
            "jinjiang-web" | "jinjiang-session" => Some(format!(
                "https://www.jjwxc.net/onebook.php?novelid={}",
                self.remote_id
            )),
            "shuqi-web" | "shuqi-session" => Some(format!(
                "https://www.shuqi.com/book/{}.html",
                self.remote_id
            )),
            "ciweimao-web" | "ciweimao-session" => {
                Some(format!("https://www.ciweimao.com/book/{}", self.remote_id))
            }
            "faloo-web" | "faloo-session" => {
                Some(format!("https://b.faloo.com/{}.html", self.remote_id))
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobItemDetail {
    pub chapter_id: String,
    pub position: u32,
    pub title: String,
    pub state: String,
    pub error: Option<String>,
    pub error_kind: Option<String>,
    pub retryable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobDetails {
    pub job: JobSnapshot,
    pub items: Vec<JobItemDetail>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibrarySnapshot {
    pub books: Vec<BookSummary>,
    pub jobs: Vec<JobSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionedSnapshot {
    pub revision: String,
    pub snapshot: LibrarySnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportReceipt {
    pub path: String,
    pub saved_chapters: u32,
    pub total_chapters: u32,
    pub partial: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportRecord {
    pub id: String,
    pub book_id: String,
    pub job_id: Option<String>,
    pub path: String,
    pub format: String,
    pub saved: u32,
    pub total: u32,
    pub partial: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadingChapter {
    pub id: String,
    pub title: String,
    pub position: u32,
    pub saved: bool,
    #[serde(default)]
    pub availability: Option<ChapterAvailability>,
}

/// Evidence about this chapter, not a promise of remote reading entitlement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChapterAvailability {
    pub state: String,
    pub reason: String,
    pub job_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookDetails {
    pub book: BookSummary,
    pub description: String,
    pub acquired_at: String,
    pub chapters: Vec<ReadingChapter>,
    pub exports: Vec<ExportRecord>,
}

pub(crate) struct BookContent {
    pub summary: BookSummary,
    pub description: String,
    pub chapters: Vec<ImportChapter>,
    pub modified: String,
}

pub fn parse_selection(input: &str, total: usize) -> Result<Vec<usize>> {
    let mut positions = std::collections::BTreeSet::new();
    if input.trim().is_empty() {
        return Err(Error::Invalid("请输入章节序号，例如 1,3,5-8".into()));
    }
    for part in input.split([',', '，']) {
        let bounds = part
            .trim()
            .split('-')
            .map(|v| {
                v.trim()
                    .parse::<usize>()
                    .map_err(|_| Error::Invalid("选章格式应为 1,3,5-8".into()))
            })
            .collect::<Result<Vec<_>>>()?;
        let (first, last) = match bounds.as_slice() {
            [one] => (*one, *one),
            [first, last] => (*first, *last),
            _ => return Err(Error::Invalid("选章范围格式错误".into())),
        };
        if first == 0 || first > last || last > total || total > MAX_CHAPTERS {
            return Err(Error::Invalid("选章范围超出目录".into()));
        }
        for p in first..=last {
            if !positions.insert(p) {
                return Err(Error::Invalid("章节选择重复".into()));
            }
        }
    }
    Ok(positions.into_iter().collect())
}
