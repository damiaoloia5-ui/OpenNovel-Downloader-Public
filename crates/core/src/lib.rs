mod accounts;
mod availability;
mod batch;
mod browser_sources;
pub use browser_sources::{
    clean_session_url, session_chapter_url, session_platform, session_source,
};
mod domain;
mod downloads;
mod export;
mod export_settings;
mod maintenance;
mod official_reader;
mod reading;
mod revisions;
mod session_capture;
pub mod sources;
mod storage;
mod txt_supplement;
pub use txt_supplement::TxtSupplementPreview;

pub use accounts::{
    AccountRecord, account_entry_url, account_login_url, account_navigation_allowed,
};
pub use batch::{BatchItem, BatchSnapshot, MAX_BATCH_ITEMS};
pub use domain::*;
pub use export::ExportFormat;
pub use export_settings::ExportDestination;
pub use maintenance::{BackupManifest, BookPreferences, DiagnosticReport};
pub use official_reader::{
    OfficialPage, official_book_link, official_login_url, official_page_url, official_platform_name,
};
pub use revisions::{
    CatalogChange, CatalogFieldChange, CatalogUpdate, HistoryEntry, RefreshReceipt,
};
pub use session_capture::{
    MAX_CAPTURE_BYTES, QIDIAN_SESSION_SOURCE, SessionCatalog, SessionChapter, SessionSaveReceipt,
    qidian_capture_url,
};
pub use storage::Library;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("访问受限：{0}")]
    Restricted(String),
    #[error("官网访问需处理：{0}")]
    AccessChallenge(String),
    #[error("来源返回内容不符合预期：{0}")]
    SourceChanged(String),
    #[error("网络请求失败：{0}")]
    Network(String),
    #[error("任务已取消")]
    Cancelled,
    #[error("输入无效：{0}")]
    Invalid(String),
    #[error("状态冲突：{0}")]
    Conflict(String),
    #[error("书库正在被另一个程序使用")]
    Locked,
    #[error("数据库错误：{0}")]
    Database(#[from] rusqlite::Error),
    #[error("文件错误：{0}")]
    Io(#[from] std::io::Error),
    #[error("JSON 格式错误：{0}")]
    Json(#[from] serde_json::Error),
    #[error("EPUB 文件错误：{0}")]
    Zip(#[from] zip::result::ZipError),
}

pub type Result<T> = std::result::Result<T, Error>;
