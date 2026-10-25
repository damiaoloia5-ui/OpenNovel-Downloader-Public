use crate::AppState;
use opennovel_core::{
    AccountRecord, account_entry_url, account_login_url, account_navigation_allowed,
};
use serde::Serialize;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use tauri::{Manager, State, WebviewUrl, WebviewWindowBuilder};

pub struct BrowserSession {
    pub _directory: tempfile::TempDir,
    pub blocked_navigation: Arc<AtomicBool>,
    pub generation: String,
    pub revision: Arc<AtomicU64>,
    pub reading: Arc<AtomicBool>,
    pub last_automatic_navigation: Arc<std::sync::Mutex<Option<std::time::Instant>>>,
}
#[derive(Serialize)]
pub struct AccountStatus {
    account: AccountRecord,
    session: &'static str,
    blocked_navigation: bool,
    can_download: bool,
}
pub(crate) fn window_label(id: &str) -> String {
    format!("account-{id}")
}

#[tauri::command]
pub async fn account_list(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<AccountStatus>, String> {
    let accounts = state.library.accounts().map_err(|e| e.to_string())?;
    let sessions = state.sessions.lock().map_err(|_| "账户会话不可用")?;
    Ok(accounts
        .into_iter()
        .map(|account| {
            let open = app.get_webview_window(&window_label(&account.id)).is_some();
            let blocked_navigation = sessions
                .get(&account.id)
                .is_some_and(|s| s.blocked_navigation.load(Ordering::Acquire));
            AccountStatus {
                account,
                session: if open { "open_unverified" } else { "closed" },
                blocked_navigation,
                can_download: false,
            }
        })
        .collect())
}
#[tauri::command]
pub async fn create_account(
    platform: String,
    label: String,
    state: State<'_, AppState>,
) -> Result<AccountRecord, String> {
    state
        .library
        .create_account(&platform, &label)
        .map_err(|e| e.to_string())
}
#[tauri::command]
pub async fn open_account(
    id: String,
    book_url: String,
    login: Option<bool>,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    if std::env::var_os("WEBVIEW2_USER_DATA_FOLDER").is_some() {
        return Err("检测到全局 WebView2 数据目录覆盖，无法保证账户隔离。请移除 WEBVIEW2_USER_DATA_FOLDER 环境变量后重启软件。".into());
    }
    let account = state.library.account(&id).map_err(|e| e.to_string())?;
    if state
        .sessions
        .lock()
        .map_err(|_| "账户会话不可用")?
        .get(&id)
        .is_some_and(|s| s.reading.load(Ordering::Acquire))
    {
        return Err("官网正在读取或自动保存，请先暂停任务后再打开其他页面".into());
    }
    let url = if login.unwrap_or(false) {
        account_login_url(&account.platform)
    } else {
        account_entry_url(&account.platform, &book_url)
    }
    .map_err(|e| e.to_string())?;
    let label = window_label(&id);
    if let Some(window) = app.get_webview_window(&label) {
        window.navigate(url).map_err(|_| "官网窗口导航失败")?;
        window.show().map_err(|_| "无法显示官网窗口")?;
        window.unminimize().map_err(|_| "无法恢复官网窗口")?;
        return window.set_focus().map_err(|_| "无法聚焦官网窗口".into());
    }
    std::fs::create_dir_all(&state.session_root).map_err(|_| "无法建立独立会话目录")?;
    let directory = tempfile::Builder::new()
        .prefix("session-")
        .tempdir_in(&state.session_root)
        .map_err(|_| "无法建立独立会话目录")?;
    let blocked = Arc::new(AtomicBool::new(false));
    let navigation_blocked = Arc::clone(&blocked);
    let revision = Arc::new(AtomicU64::new(0));
    let navigation_revision = Arc::clone(&revision);
    let popup_blocked = Arc::clone(&blocked);
    let platform = account.platform.clone();
    let popup_platform = platform.clone();
    let popup_app = app.clone();
    let popup_label = label.clone();
    let title = format!(
        "{} · {} · 官网独立会话",
        if platform == "qidian" {
            "起点"
        } else {
            "书旗"
        },
        account.label
    );
    let window = WebviewWindowBuilder::new(&app, &label, WebviewUrl::External(url))
        .title(title)
        .inner_size(1100.0, 800.0)
        .data_directory(directory.path().join("webview"))
        .incognito(true)
        .devtools(false)
        .on_navigation(move |url| {
            navigation_revision.fetch_add(1, Ordering::AcqRel);
            let allowed = account_navigation_allowed(&platform, url);
            if !allowed {
                navigation_blocked.store(true, Ordering::Release);
            }
            allowed
        })
        .on_new_window(move |url, _| {
            if account_navigation_allowed(&popup_platform, &url) {
                if let Some(window) = popup_app.get_webview_window(&popup_label) {
                    let _ = window.navigate(url);
                }
            } else {
                popup_blocked.store(true, Ordering::Release);
            }
            tauri::webview::NewWindowResponse::Deny
        })
        .on_download(|_, _| false)
        .build()
        .map_err(|_| "官网会话窗口创建失败")?;
    state.sessions.lock().map_err(|_| "账户会话不可用")?.insert(
        id.clone(),
        BrowserSession {
            _directory: directory,
            blocked_navigation: blocked,
            generation: uuid::Uuid::new_v4().to_string(),
            revision,
            reading: Arc::new(AtomicBool::new(false)),
            last_automatic_navigation: Arc::new(std::sync::Mutex::new(None)),
        },
    );
    let handle = app.clone();
    window.on_window_event(move |event| {
        if matches!(event, tauri::WindowEvent::Destroyed)
            && let Some(state) = handle.try_state::<AppState>()
            && let Ok(mut sessions) = state.sessions.lock()
        {
            sessions.remove(&id);
            drop(sessions);
            if let Ok(mut previews) = state.captures.lock() {
                previews.retain(|_, p| p.account != id);
            }
        }
    });
    window.set_focus().map_err(|_| "无法聚焦官网窗口".into())
}
#[tauri::command]
pub async fn close_account(
    id: String,
    remove: bool,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.library.account(&id).map_err(|e| e.to_string())?;
    if let Some(window) = app.get_webview_window(&window_label(&id)) {
        window
            .clear_all_browsing_data()
            .map_err(|_| "官网会话清理未成功，请重试；尚未移除记录")?;
        window.destroy().map_err(|_| "官网窗口关闭失败")?;
    }
    state
        .sessions
        .lock()
        .map_err(|_| "账户会话不可用")?
        .remove(&id);
    state
        .captures
        .lock()
        .map_err(|_| "页面预览不可用")?
        .retain(|_, p| p.account != id);
    if remove {
        state
            .library
            .remove_account(&id)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn local_ipc_allowed(label: &str, url: &tauri::Url) -> bool {
    if label != "main" || !url.username().is_empty() || url.password().is_some() {
        return false;
    }
    matches!(
        (url.scheme(), url.host_str(), url.port()),
        ("tauri", Some("localhost"), None) | ("http" | "https", Some("tauri.localhost"), None)
    ) || (cfg!(debug_assertions)
        && url.scheme() == "http"
        && url.host_str() == Some("127.0.0.1")
        && url.port() == Some(1420))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn remote_windows_and_changed_main_origins_cannot_dispatch_local_commands() {
        for (label, url) in [
            ("account-example", "https://www.qidian.com/"),
            ("account-example", "http://tauri.localhost/"),
            ("main", "https://tauri.localhost.evil/"),
            ("main", "https://www.shuqi.com/"),
            ("main", "http://tauri.localhost:1234/"),
            ("main", "https://user@tauri.localhost/"),
        ] {
            assert!(!local_ipc_allowed(label, &url.parse().unwrap()));
        }
        assert!(local_ipc_allowed(
            "main",
            &"http://tauri.localhost/".parse().unwrap()
        ));
    }
}
