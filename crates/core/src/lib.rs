mod batch;
mod domain;
mod downloads;
mod export;
mod maintenance;
mod reading;
mod revisions;
pub mod sources;
mod storage;

pub use batch::{BatchItem, BatchSnapshot, MAX_BATCH_ITEMS};
pub use domain::*;
pub use export::ExportFormat;
pub use maintenance::{BackupManifest, BookPreferences, DiagnosticReport};
pub use revisions::{
    CatalogChange, CatalogFieldChange, CatalogUpdate, HistoryEntry, RefreshReceipt,
};
pub use storage::Library;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("访问受限：{0}")]
    Restricted(String),
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
