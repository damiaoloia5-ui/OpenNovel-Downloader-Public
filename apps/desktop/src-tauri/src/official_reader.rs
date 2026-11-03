use crate::{
    AppState,
    accounts::{open_official_window, window_label},
};
use opennovel_core::{AccountRecord, OfficialPage};
use serde::Serialize;
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tauri::{Manager, State};

struct Preview {
    account: String,
    generation: String,
    created: Instant,
    page: OfficialPage,
}
#[derive(Default)]
pub struct OfficialReaderState {
    pending: Mutex<HashMap<String, Preview>>,
    exports: Mutex<HashMap<String, PathBuf>>,
    picking: AtomicBool,
}
#[derive(Serialize)]
pub struct PagePreview {
    token: String,
    page: OfficialPage,
}
#[derive(Serialize)]
pub struct PageExport {
    id: String,
    path: String,
}
fn session_id(platform: &str) -> Result<String, String> {
    if !["ciweimao", "faloo", "migu"].contains(&platform) {
        return Err("请选择刺猬猫、飞卢或咪咕".into());
    }
    Ok(format!("official-{platform}"))
}
struct Lease(Arc<AtomicBool>);
impl Drop for Lease {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

#[tauri::command]
pub async fn open_official_reader(
    platform: String,
    book_url: String,
    login: bool,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let id = session_id(&platform)?;
    let url = if login {
        opennovel_core::official_login_url(&platform)
    } else {
        opennovel_core::official_book_link(&platform, &book_url)
    }
    .map_err(|e| e.to_string())?;
    open_official_window(
        AccountRecord {
            id,
            platform,
            label: "临时阅读会话".into(),
        },
        url,
        app,
        &state,
    )
}
#[tauri::command]
pub async fn inspect_official_reader(
    platform: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    reader: State<'_, OfficialReaderState>,
) -> Result<PagePreview, String> {
    let id = session_id(&platform)?;
    let window = app
        .get_webview_window(&window_label(&id))
        .ok_or("请先打开该平台官网窗口并本人登录")?;
    let before = window.url().map_err(|_| "无法读取官网地址")?;
    if !opennovel_core::account_navigation_allowed(&platform, &before) {
        return Err("官网地址不属于当前平台".into());
    }
    let (generation, revision, reading) = {
        let sessions = state.sessions.lock().map_err(|_| "官网会话不可用")?;
        let s = sessions.get(&id).ok_or("官网会话已关闭")?;
        (
            s.generation.clone(),
            Arc::clone(&s.revision),
            Arc::clone(&s.reading),
        )
    };
    if reading.swap(true, Ordering::AcqRel) {
        return Err("官网正在读取，请稍后重试".into());
    }
    let _lease = Lease(reading);
    let epoch = revision.load(Ordering::Acquire);
    let script = include_str!("official_page.js").replace("__PLATFORM__", &platform);
    let (sender, receiver) = std::sync::mpsc::channel();
    window
        .eval_with_callback(script, move |raw| {
            let _ = sender.send(raw);
        })
        .map_err(|_| "官网识别未能启动")?;
    let raw =
        tauri::async_runtime::spawn_blocking(move || receiver.recv_timeout(Duration::from_secs(8)))
            .await
            .map_err(|_| "官网识别中断")?
            .map_err(|_| "官网识别超时，请等待页面加载完成后重试")?;
    if before != window.url().map_err(|_| "官网已关闭")?
        || epoch != revision.load(Ordering::Acquire)
        || state
            .sessions
            .lock()
            .map_err(|_| "官网会话不可用")?
            .get(&id)
            .is_none_or(|s| s.generation != generation)
    {
        return Err("官网页面或会话已变化，请重新识别".into());
    }
    let clean =
        opennovel_core::official_page_url(&platform, before.as_str()).map_err(|e| e.to_string())?;
    let page = OfficialPage::parse(&raw, &platform, clean.as_str()).map_err(|e| e.to_string())?;
    let token = uuid::Uuid::new_v4().to_string();
    let mut pending = reader.pending.lock().map_err(|_| "识别预览不可用")?;
    pending.retain(|_, p| p.account != id && p.created.elapsed() < Duration::from_secs(600));
    // Three fixed sessions, one current preview each. Login state is never inferred here.
    let preview = PagePreview {
        token: token.clone(),
        page: page.clone(),
    };
    pending.insert(
        token,
        Preview {
            account: id,
            generation,
            created: Instant::now(),
            page,
        },
    );
    Ok(preview)
}
#[tauri::command]
pub async fn save_official_reader(
    platform: String,
    token: String,
    acknowledged: bool,
    directory: Option<String>,
    state: State<'_, AppState>,
    reader: State<'_, OfficialReaderState>,
) -> Result<PageExport, String> {
    if !acknowledged {
        return Err("请先确认阅读权限与当前页采集范围".into());
    }
    let id = session_id(&platform)?;
    let directory = match directory.filter(|d| !d.trim().is_empty()) {
        Some(path) => {
            let path = PathBuf::from(path.trim());
            if !path.is_absolute() || !path.is_dir() {
                return Err("请选择已存在的文件夹，或填写完整路径".into());
            }
            tempfile::NamedTempFile::new_in(&path).map_err(|_| "所选文件夹不可写，请重新选择")?;
            path
        }
        None => state.exports.join("official-pages"),
    };
    let preview = {
        let mut pending = reader.pending.lock().map_err(|_| "识别预览不可用")?;
        if pending.get(&token).is_none_or(|p| p.account != id) {
            return Err("识别预览不属于当前平台，请重新识别".into());
        }
        pending.remove(&token).expect("checked preview")
    };
    if preview.created.elapsed() >= Duration::from_secs(600)
        || state
            .sessions
            .lock()
            .map_err(|_| "官网会话不可用")?
            .get(&id)
            .is_none_or(|s| s.generation != preview.generation)
    {
        return Err("官网会话已关闭或预览过期，请重新识别".into());
    }
    let path = tauri::async_runtime::spawn_blocking(move || preview.page.save_txt(&directory))
        .await
        .map_err(|_| "保存中断")?
        .map_err(|e| e.to_string())?;
    let receipt_id = uuid::Uuid::new_v4().to_string();
    let mut exports = reader.exports.lock().map_err(|_| "导出记录不可用")?;
    if exports.len() >= 100 {
        exports.clear();
    }
    exports.insert(receipt_id.clone(), PathBuf::from(&path));
    Ok(PageExport {
        id: receipt_id,
        path,
    })
}

#[tauri::command]
pub async fn choose_official_export_directory(
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
    reader: State<'_, OfficialReaderState>,
) -> Result<Option<String>, String> {
    if reader.picking.swap(true, Ordering::AcqRel) {
        return Err("文件夹选择窗口已经打开".into());
    }
    struct Guard<'a>(&'a AtomicBool);
    impl Drop for Guard<'_> {
        fn drop(&mut self) {
            self.0.store(false, Ordering::Release);
        }
    }
    let _guard = Guard(&reader.picking);
    #[cfg(windows)]
    let owner = window.hwnd().map_err(|e| e.to_string())?.0 as isize;
    #[cfg(not(windows))]
    let owner = {
        let _ = window;
        0
    };
    let initial = state.exports.to_string_lossy().into_owned();
    tauri::async_runtime::spawn_blocking(move || crate::folder_dialog::pick(owner, &initial))
        .await
        .map_err(|_| "文件夹选择中断")?
}
#[tauri::command]
pub async fn reveal_official_reader(
    id: String,
    reader: State<'_, OfficialReaderState>,
) -> Result<(), String> {
    let path = reader
        .exports
        .lock()
        .map_err(|_| "导出记录不可用")?
        .get(&id)
        .cloned()
        .ok_or("本次导出记录已失效；文件仍保留在导出目录")?;
    if !path.is_file() {
        return Err("TXT 已移动或删除".into());
    }
    #[cfg(target_os = "windows")]
    std::process::Command::new("explorer.exe")
        .arg(path.parent().ok_or("导出位置无效")?)
        .spawn()
        .map_err(|_| "无法打开文件夹")?;
    Ok(())
}
#[tauri::command]
pub async fn close_official_reader(
    platform: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    reader: State<'_, OfficialReaderState>,
) -> Result<(), String> {
    let id = session_id(&platform)?;
    if state
        .sessions
        .lock()
        .map_err(|_| "官网会话不可用")?
        .get(&id)
        .is_some_and(|s| s.reading.load(Ordering::Acquire))
    {
        return Err("官网正在读取，请完成后关闭".into());
    }
    if let Some(window) = app.get_webview_window(&window_label(&id)) {
        window
            .clear_all_browsing_data()
            .map_err(|_| "官网会话清理失败，请重试")?;
        window.destroy().map_err(|_| "官网窗口关闭失败")?;
    }
    state
        .sessions
        .lock()
        .map_err(|_| "官网会话不可用")?
        .remove(&id);
    reader
        .pending
        .lock()
        .map_err(|_| "识别预览不可用")?
        .retain(|_, p| p.account != id);
    Ok(())
}
