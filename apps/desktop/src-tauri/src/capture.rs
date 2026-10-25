use crate::{AppState, accounts::window_label};
use opennovel_core::{SessionCatalog, SessionChapter, SessionSaveReceipt};
use serde::Serialize;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tauri::{Manager, State};

pub enum CapturedPage {
    Catalog(SessionCatalog),
    Chapter(SessionChapter),
}
pub struct PendingCapture {
    pub account: String,
    pub generation: String,
    created: Instant,
    page: CapturedPage,
}
impl PendingCapture {
    fn valid_for(&self, account: &str, generation: &str) -> bool {
        self.account == account
            && self.generation == generation
            && self.created.elapsed() < Duration::from_secs(600)
    }
}
#[derive(Serialize)]
pub struct CapturePreview {
    token: String,
    kind: String,
    catalog: Option<opennovel_core::sources::Catalog>,
    chapter: Option<SessionChapter>,
}
#[derive(Serialize)]
pub struct CaptureReceipt {
    kind: &'static str,
    book_id: String,
    save: Option<SessionSaveReceipt>,
}
struct ReadingGuard(Arc<AtomicBool>);
impl Drop for ReadingGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

fn clean_page_url(window: &tauri::WebviewWindow) -> Result<String, String> {
    let mut url = window.url().map_err(|_| "无法确认官网页面地址")?;
    url.set_query(None);
    url.set_fragment(None);
    let value = url.to_string();
    opennovel_core::qidian_capture_url(&value).map_err(|e| e.to_string())?;
    Ok(value)
}

#[tauri::command]
pub async fn preview_account_page(
    id: String,
    kind: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<CapturePreview, String> {
    if !matches!(kind.as_str(), "catalog" | "chapter") {
        return Err("请选择目录或当前章".into());
    }
    let account = state.library.account(&id).map_err(|e| e.to_string())?;
    if account.platform != "qidian" {
        return Err("当前仅提供起点官网页面读取实验".into());
    }
    let window = app
        .get_webview_window(&window_label(&id))
        .ok_or("请先打开该账户的官网窗口")?;
    let before = clean_page_url(&window)?;
    let (book, chapter) = opennovel_core::qidian_capture_url(&before).map_err(|e| e.to_string())?;
    if (kind == "catalog") != chapter.is_none() {
        return Err("读取目录请打开作品完整目录；读取当前章请打开完整正文页".into());
    }
    // Require a confirmed directory before reading any chapter text.
    if kind == "chapter"
        && !state
            .library
            .snapshot()
            .map_err(|e| e.to_string())?
            .books
            .iter()
            .any(|b| b.source == opennovel_core::QIDIAN_SESSION_SOURCE && b.remote_id == book)
    {
        return Err("请先读取并确认保存该作品的完整目录".into());
    }
    let (generation, revision, reading) = {
        let sessions = state.sessions.lock().map_err(|_| "账户会话不可用")?;
        let session = sessions.get(&id).ok_or("官网会话已关闭，请重新打开")?;
        (
            session.generation.clone(),
            Arc::clone(&session.revision),
            Arc::clone(&session.reading),
        )
    };
    if reading.swap(true, Ordering::AcqRel) {
        return Err("该账户正在读取页面，请稍后重试".into());
    }
    let _guard = ReadingGuard(reading);
    let epoch = revision.load(Ordering::Acquire);
    let script = include_str!("capture_page.js").replace("__MODE__", &kind);
    let (sender, receiver) = std::sync::mpsc::channel();
    window
        .eval_with_callback(script, move |raw| {
            let _ = sender.send(raw);
        })
        .map_err(|_| "无法读取官网页面")?;
    let raw =
        tauri::async_runtime::spawn_blocking(move || receiver.recv_timeout(Duration::from_secs(8)))
            .await
            .map_err(|_| "页面读取任务中断")?
            .map_err(|_| "页面读取超时，请等待页面加载完成后重试")?;
    if raw.len() > opennovel_core::MAX_CAPTURE_BYTES {
        return Err("页面读取结果超过 2 MiB，未保存".into());
    }
    if revision.load(Ordering::Acquire) != epoch || clean_page_url(&window)? != before {
        return Err("读取期间页面已切换，请重新预览".into());
    }
    if state
        .sessions
        .lock()
        .map_err(|_| "账户会话不可用")?
        .get(&id)
        .is_none_or(|s| s.generation != generation)
    {
        return Err("官网会话已变化，请重新读取".into());
    }
    let data: serde_json::Value = serde_json::from_str(&raw).map_err(|_| "官网页面读取结果无效")?;
    if let Some(error) = data.get("error").and_then(|v| v.as_str()) {
        return Err(match error {
            "catalog" => "完整目录或官网总章数未识别；请展开目录并等待全部加载后重试",
            "restricted" => "页面包含订阅或试读提示，本次未保存",
            "protected" => "当前正文包含受保护的呈现方式，暂不支持保存",
            "large" => "页面内容超过读取上限，未保存",
            _ => "当前页面结构暂不支持；请确认已打开完整目录或完整正文，未保存任何内容",
        }
        .into());
    }
    let page = if kind == "catalog" {
        CapturedPage::Catalog(SessionCatalog::parse(&raw, &before).map_err(|e| e.to_string())?)
    } else {
        CapturedPage::Chapter(SessionChapter::parse(&raw, &before).map_err(|e| e.to_string())?)
    };
    let mut pending = state.captures.lock().map_err(|_| "页面预览不可用")?;
    pending.retain(|_, p| p.created.elapsed() < Duration::from_secs(600) && p.account != id);
    if pending.len() >= 8 {
        return Err("待确认预览过多，请稍后重试".into());
    }
    let token = uuid::Uuid::new_v4().to_string();
    let preview = match &page {
        CapturedPage::Catalog(c) => CapturePreview {
            token: token.clone(),
            kind: kind.clone(),
            catalog: Some(c.catalog().clone()),
            chapter: None,
        },
        CapturedPage::Chapter(c) => CapturePreview {
            token: token.clone(),
            kind: kind.clone(),
            catalog: None,
            chapter: Some(c.clone()),
        },
    };
    pending.insert(
        token,
        PendingCapture {
            account: id,
            generation,
            created: Instant::now(),
            page,
        },
    );
    Ok(preview)
}

#[tauri::command]
pub async fn confirm_account_page(
    id: String,
    token: String,
    state: State<'_, AppState>,
) -> Result<CaptureReceipt, String> {
    let account = state.library.account(&id).map_err(|e| e.to_string())?;
    if account.platform != "qidian" {
        return Err("来源不匹配".into());
    }
    let capture = {
        let mut pending = state.captures.lock().map_err(|_| "页面预览不可用")?;
        if pending.get(&token).is_none_or(|p| p.account != id) {
            return Err("预览不存在或属于其他账户，请重新读取".into());
        }
        pending.remove(&token).expect("checked preview")
    };
    if !state
        .sessions
        .lock()
        .map_err(|_| "账户会话不可用")?
        .get(&id)
        .is_some_and(|s| capture.valid_for(&id, &s.generation))
    {
        return Err("预览已过期或会话已关闭，请重新读取".into());
    }
    let library = Arc::clone(&state.library);
    tauri::async_runtime::spawn_blocking(move || match capture.page {
        CapturedPage::Catalog(c) => {
            library
                .register_session_catalog(&c)
                .map(|book_id| CaptureReceipt {
                    kind: "catalog",
                    book_id,
                    save: None,
                })
        }
        CapturedPage::Chapter(c) => library.save_session_chapter(&c).map(|save| CaptureReceipt {
            kind: "chapter",
            book_id: save.book_id.clone(),
            save: Some(save),
        }),
    })
    .await
    .map_err(|_| "保存任务中断")?
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn previews_expire_and_cannot_cross_accounts_or_reopened_sessions() {
        let mut preview = PendingCapture {
            account: "account-a".into(),
            generation: "session-1".into(),
            created: Instant::now(),
            page: CapturedPage::Chapter(SessionChapter {
                remote_book_id: "1".into(),
                chapter: opennovel_core::ImportChapter {
                    id: "2".into(),
                    title: "受控测试".into(),
                    paragraphs: vec!["原创".into()],
                },
                declared_words: 2,
            }),
        };
        assert!(preview.valid_for("account-a", "session-1"));
        assert!(!preview.valid_for("account-b", "session-1"));
        assert!(!preview.valid_for("account-a", "session-2"));
        preview.created = Instant::now() - Duration::from_secs(601);
        assert!(!preview.valid_for("account-a", "session-1"));
    }
    #[test]
    fn reading_guard_releases_after_error_or_completion() {
        let busy = Arc::new(AtomicBool::new(true));
        {
            let _guard = ReadingGuard(Arc::clone(&busy));
            assert!(busy.load(Ordering::Acquire));
        }
        assert!(!busy.load(Ordering::Acquire));
    }
}
