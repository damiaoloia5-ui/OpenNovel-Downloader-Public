//! Fixed-repository, signed Windows updates. Never accepts a frontend URL or executable path.
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tauri::{Manager, State};
use tauri_plugin_updater::{Update, UpdaterExt};

pub const ENDPOINT: &str = "https://github.com/damiaoloia5-ui/OpenNovel-Downloader-Public/releases/latest/download/latest.json";
const RELEASES: &str = "https://github.com/damiaoloia5-ui/OpenNovel-Downloader-Public/releases";
const MAX_MANIFEST: usize = 128 * 1024;
const MAX_INSTALLER: usize = 768 * 1024 * 1024;

#[derive(Clone, serde::Serialize)]
pub struct Status {
    phase: String,
    current_version: String,
    version: Option<String>,
    notes: String,
    downloaded: usize,
    total: Option<u64>,
    message: String,
    backup_directory: Option<String>,
    portable: bool,
}
impl Default for Status {
    fn default() -> Self {
        Self {
            phase: "idle".into(),
            current_version: env!("CARGO_PKG_VERSION").into(),
            version: None,
            notes: String::new(),
            downloaded: 0,
            total: None,
            message: "更新来自 OpenNovel 公开 GitHub 仓库".into(),
            backup_directory: None,
            portable: tauri::utils::platform::bundle_type()
                != Some(tauri::utils::config::BundleType::Nsis),
        }
    }
}
#[derive(Default)]
struct Cache {
    status: Status,
    update: Option<Update>,
    bytes: Option<Arc<Vec<u8>>>,
    last_check: Option<Instant>,
}
#[derive(Default)]
pub struct UpdateState {
    cache: Mutex<Cache>,
    busy: AtomicBool,
    cancel: AtomicBool,
}
impl UpdateState {
    fn status(&self) -> Result<Status, String> {
        Ok(self
            .cache
            .lock()
            .map_err(|_| "更新状态不可用")?
            .status
            .clone())
    }
    fn set(&self, phase: &str, message: impl Into<String>) {
        if let Ok(mut c) = self.cache.lock() {
            c.status.phase = phase.into();
            c.status.message = message.into();
        }
    }
    fn enter(&self) -> Result<Busy<'_>, String> {
        if self.busy.swap(true, Ordering::AcqRel) {
            return Err("更新操作正在进行，请稍候".into());
        }
        self.cancel.store(false, Ordering::Release);
        Ok(Busy(self))
    }
}
struct Busy<'a>(&'a UpdateState);
impl Drop for Busy<'_> {
    fn drop(&mut self) {
        self.0.busy.store(false, Ordering::Release);
    }
}

fn transport_allowed(url: &reqwest::Url) -> bool {
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port_or_known_default() == Some(443)
        && matches!(
            url.host_str(),
            Some(
                "github.com"
                    | "release-assets.githubusercontent.com"
                    | "objects.githubusercontent.com"
            )
        )
}
fn validate_release(raw: &serde_json::Value) -> Result<(), String> {
    let value = raw
        .get("version")
        .and_then(|v| v.as_str())
        .ok_or("更新清单缺少版本号")?;
    let version = semver::Version::parse(value).map_err(|_| "更新版本号无效")?;
    if !version.pre.is_empty() || !version.build.is_empty() {
        return Err("仅接受正式版本号的更新清单".into());
    }
    let platform = &raw["platforms"]["windows-x86_64"];
    let expected =
        format!("{RELEASES}/download/v{version}/OpenNovel-{version}-Windows-x64-setup.exe");
    if platform["url"].as_str() != Some(expected.as_str())
        || platform["signature"]
            .as_str()
            .is_none_or(|s| s.is_empty() || s.len() > 8192)
    {
        return Err("更新地址或签名不符合固定公开仓库规则".into());
    }
    Ok(())
}

/// Offline release verification using the public key embedded in the real executable.
/// Does not initialize a profile, download anything, or execute the installer.
pub fn verify_local(manifest: &Path, installer: &Path) -> Result<(), String> {
    fn bounded(path: &Path, limit: usize) -> Result<Vec<u8>, String> {
        use std::io::Read;
        let file = std::fs::File::open(path).map_err(|_| "无法打开校验文件")?;
        let mut bytes = Vec::new();
        file.take(limit as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "无法读取校验文件")?;
        if bytes.len() > limit {
            return Err("校验文件过大".into());
        }
        Ok(bytes)
    }
    let raw =
        serde_json::from_slice(&bounded(manifest, MAX_MANIFEST)?).map_err(|_| "更新清单无效")?;
    validate_release(&raw)?;
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .build(tauri::generate_context!())
        .map_err(|_| "无法初始化签名校验")?;
    // The verifier can test the current release; the normal updater never overrides comparison.
    let update = app
        .updater_builder()
        .version_comparator(|_, _| true)
        .build()
        .map_err(|_| "签名配置无效")?
        .restore_update(raw)
        .map_err(|_| "更新清单无效")?
        .ok_or("更新清单无安装包")?;
    let bytes = bounded(installer, MAX_INSTALLER)?;
    update.verify(&bytes).map_err(|_| "签名或版本校验失败")?;
    if !bytes.starts_with(b"MZ") {
        return Err("不是 Windows 安装器".into());
    }
    Ok(())
}

async fn fetch(
    url: &str,
    limit: usize,
    state: &UpdateState,
    progress: bool,
) -> Result<Option<Vec<u8>>, String> {
    let url = reqwest::Url::parse(url).map_err(|_| "更新地址无效")?;
    if !transport_allowed(&url) {
        return Err("更新地址不受信任".into());
    }
    let client = reqwest::Client::builder()
        .user_agent("OpenNovel-Updater")
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(Duration::from_secs(40))
        .timeout(Duration::from_secs(if progress { 600 } else { 30 }))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() < 5 && transport_allowed(attempt.url()) {
                attempt.follow()
            } else {
                attempt.error("untrusted updater redirect")
            }
        }))
        .build()
        .map_err(|_| "无法创建更新连接")?;
    let mut response = client
        .get(url)
        .send()
        .await
        .map_err(|_| "GitHub 连接失败，请稍后重试；本地数据未改动")?;
    if !progress && response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !response.status().is_success() {
        return Err(format!(
            "GitHub 返回 HTTP {}，请稍后重试",
            response.status().as_u16()
        ));
    }
    let length = response.content_length();
    if length.is_some_and(|n| n > limit as u64) {
        return Err("更新文件超过安全大小限制".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "更新下载中断，请重试；旧版本仍可使用")?
    {
        if state.cancel.load(Ordering::Acquire) {
            return Err("已取消更新下载，旧版本和书库保持不变".into());
        }
        if bytes.len().saturating_add(chunk.len()) > limit {
            return Err("更新文件超过安全大小限制".into());
        }
        bytes.extend_from_slice(&chunk);
        if progress && let Ok(mut c) = state.cache.lock() {
            c.status.downloaded = bytes.len();
            c.status.total = length;
        }
    }
    if length.is_some_and(|n| n != bytes.len() as u64) {
        return Err("更新文件未下载完整".into());
    }
    Ok(Some(bytes))
}

#[tauri::command]
pub fn software_update_status(state: State<'_, UpdateState>) -> Result<Status, String> {
    state.status()
}

#[tauri::command]
pub async fn check_software_update(
    app: tauri::AppHandle,
    state: State<'_, UpdateState>,
) -> Result<Status, String> {
    let _busy = state.enter()?;
    {
        let mut c = state.cache.lock().map_err(|_| "更新状态不可用")?;
        if c.last_check
            .is_some_and(|last| last.elapsed() < Duration::from_secs(30))
        {
            return Ok(c.status.clone());
        }
        if c.bytes.is_some() {
            return Ok(c.status.clone());
        }
        c.last_check = Some(Instant::now());
        c.update = None;
        c.status.version = None;
    }
    state.set("checking", "正在检查 GitHub 新版本…");
    let result = async {
        let Some(bytes) = fetch(ENDPOINT, MAX_MANIFEST, &state, false).await? else {
            state.set(
                "unavailable",
                "发布者尚未提供自动更新清单，当前版本可继续使用",
            );
            return state.status();
        };
        let raw: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|_| "更新清单格式无效")?;
        validate_release(&raw)?;
        let update = app
            .updater()
            .map_err(|_| "更新签名配置不可用")?
            .restore_update(raw)
            .map_err(|_| "无法识别更新清单")?;
        let mut c = state.cache.lock().map_err(|_| "更新状态不可用")?;
        if let Some(update) = update {
            c.status.version = Some(update.version.clone());
            c.status.notes = update
                .body
                .as_deref()
                .unwrap_or_default()
                .chars()
                .take(4000)
                .collect();
            c.status.phase = "available".into();
            c.status.message = "发现新版本，更新前会备份本地书库".into();
            c.update = Some(update);
        } else {
            c.status.phase = "current".into();
            c.status.message = "当前已是最新版本".into();
        }
        Ok(c.status.clone())
    }
    .await;
    if let Err(error) = &result {
        state.set("error", error);
    }
    result
}

#[tauri::command]
pub fn cancel_software_download(state: State<'_, UpdateState>) {
    state.cancel.store(true, Ordering::Release);
}

#[tauri::command]
pub async fn download_software_update(state: State<'_, UpdateState>) -> Result<Status, String> {
    let _busy = state.enter()?;
    let update = {
        let c = state.cache.lock().map_err(|_| "更新状态不可用")?;
        if c.bytes.is_some() {
            return Ok(c.status.clone());
        }
        c.update.clone().ok_or("请先检查新版本")?
    };
    state.set("downloading", "正在下载新版安装包，当前书库可正常使用…");
    let result = async {
        let bytes = fetch(update.download_url.as_str(), MAX_INSTALLER, &state, true)
            .await?
            .ok_or("下载内容为空")?;
        state.set("verifying", "正在核对安装包签名和版本…");
        let update_copy = update.clone();
        let bytes = tauri::async_runtime::spawn_blocking(move || {
            update_copy
                .verify(&bytes)
                .map_err(|_| "更新签名或版本校验失败，拒绝安装".to_string())?;
            if !bytes.starts_with(b"MZ") {
                return Err("更新内容不是 Windows 安装器".to_string());
            }
            Ok::<_, String>(bytes)
        })
        .await
        .map_err(|_| "更新校验线程失败")??;
        state.cache.lock().map_err(|_| "更新状态不可用")?.bytes = Some(Arc::new(bytes));
        state.set("ready", "安装包已校验，准备备份书库并安装");
        state.status()
    }
    .await;
    if let Err(error) = &result {
        state.set("error", error);
    }
    result
}

pub struct ProfileBackup {
    directory: PathBuf,
    // Keep other libraries unavailable until the installer takes over.
    _leases: Vec<std::fs::File>,
}
pub fn backup_profiles(
    store: &opennovel_core::profiles::ProfileStore,
    current_id: &str,
    current: &opennovel_core::Library,
    root: &Path,
) -> Result<ProfileBackup, String> {
    std::fs::create_dir_all(root).map_err(|_| "无法创建更新备份根目录")?;
    let directory = root.join(format!("before-update-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&directory).map_err(|_| "无法创建独立更新备份目录")?;
    store
        .backup_registry(&directory.join("profiles.json"))
        .map_err(|e| e.to_string())?;
    let mut entries = Vec::new();
    let mut leases = Vec::new();
    for profile in store.list().map_err(|e| e.to_string())? {
        let paths = store
            .resolve(Some(&profile.id))
            .map_err(|e| e.to_string())?;
        if !paths.database.exists() {
            continue;
        }
        let destination = directory.join(&profile.id);
        let manifest = if profile.id == current_id {
            current.backup(&destination).map_err(|e| e.to_string())?
        } else {
            let (manifest, lease) =
                opennovel_core::Library::backup_inactive(&paths.database, &destination)
                    .map_err(|e| format!("书库备份失败，已停止安装：{e}"))?;
            leases.push(lease);
            manifest
        };
        entries.push(serde_json::json!({"profile": profile.id, "original_database": paths.database, "sha256": manifest.sha256}));
    }
    let receipt = serde_json::json!({"complete": true, "version_before": env!("CARGO_PKG_VERSION"), "profiles": entries,
        "policy": "Only library snapshots and registry; original exports, cookies and login files stay in place."});
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(directory.join("complete.json"))
        .map_err(|e| e.to_string())?;
    file.write_all(
        serde_json::to_string_pretty(&receipt)
            .map_err(|e| e.to_string())?
            .as_bytes(),
    )
    .map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    Ok(ProfileBackup {
        directory,
        _leases: leases,
    })
}

fn ensure_idle(app: &tauri::AppHandle, state: &crate::AppState) -> Result<(), String> {
    if !state
        .cancellations
        .lock()
        .map_err(|_| "任务状态不可用")?
        .is_empty()
        || !state
            .batches
            .lock()
            .map_err(|_| "批次状态不可用")?
            .is_empty()
        || state
            .library
            .snapshot()
            .map_err(|e| e.to_string())?
            .jobs
            .iter()
            .any(|j| matches!(j.state.as_str(), "running" | "queued"))
        || state
            .auto_txt
            .snapshot()?
            .iter()
            .any(|s| s.state == "generating")
    {
        return Err(
            "请先暂停保存任务并等待文件生成完成，再安装更新；已下载的安装包会保留到关闭软件前"
                .into(),
        );
    }
    if app.webview_windows().keys().any(|label| label != "main") {
        return Err("请先关闭软件内的官网阅读窗口，再安装更新；书库和账号记录不会删除".into());
    }
    Ok(())
}

#[tauri::command]
pub async fn install_software_update(
    convert_portable: bool,
    app: tauri::AppHandle,
    update_state: State<'_, UpdateState>,
    state: State<'_, crate::AppState>,
    profile: State<'_, crate::profiles::ProfileState>,
) -> Result<(), String> {
    let _busy = update_state.enter()?;
    if cfg!(debug_assertions) {
        return Err("开发运行仅支持检查和下载，请用打包后的软件安装更新".into());
    }
    if update_state.status()?.portable && !convert_portable {
        return Err("便携版更新将转为安装版，请先勾选确认；旧便携目录保留".into());
    }
    let (update, bytes) = {
        let c = update_state.cache.lock().map_err(|_| "更新状态不可用")?;
        (
            c.update.clone().ok_or("尚未检查更新")?,
            c.bytes.clone().ok_or("请先下载并校验安装包")?,
        )
    };
    let maintenance = crate::operation::Maintenance::begin()?;
    update_state.set("installing", "正在等待操作完成并备份书库，请勿关闭软件…");
    let result = async {
        let maintenance = tauri::async_runtime::spawn_blocking(move || {
            let deadline = Instant::now() + Duration::from_secs(30);
            while !maintenance.drained()? {
                if Instant::now() >= deadline {
                    return Err("还有操作尚未结束，请稍后安装".to_string());
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Ok::<_, String>(maintenance)
        })
        .await
        .map_err(|_| "等待操作失败")??;
        ensure_idle(&app, &state)?;
        let store = profile.store.clone();
        let id = profile.paths.profile.id.clone();
        let library = Arc::clone(&state.library);
        let backup_root = app
            .path()
            .app_data_dir()
            .map_err(|_| "无法确定更新备份位置")?
            .join("update-backups");
        let backup = tauri::async_runtime::spawn_blocking(move || {
            backup_profiles(&store, &id, &library, &backup_root)
        })
        .await
        .map_err(|_| "更新备份线程失败")??;
        update_state
            .cache
            .lock()
            .map_err(|_| "更新状态不可用")?
            .status
            .backup_directory = Some(backup.directory.to_string_lossy().into_owned());
        update_state.set("installing", "书库备份完成，正在启动新版安装器…");
        // Tauri verifies the bytes again, launches NSIS /UPDATE and exits only on successful launch.
        tauri::async_runtime::spawn_blocking(move || {
            let _maintenance = maintenance;
            let _backup = backup;
            update
                .install(bytes.as_slice())
                .map_err(|_| "安装器未能启动；旧版本和书库备份仍保留".to_string())
        })
        .await
        .map_err(|_| "安装线程失败")?
    }
    .await;
    if let Err(error) = &result {
        update_state.set("ready", error);
    }
    result
}

#[tauri::command]
pub fn open_software_releases() -> Result<(), String> {
    #[cfg(windows)]
    {
        std::process::Command::new("explorer.exe")
            .arg(RELEASES)
            .spawn()
            .map_err(|_| "无法打开浏览器")?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        Err("当前仅提供 Windows 下载入口".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn manifest() -> serde_json::Value {
        serde_json::json!({"version":"1.2.3", "platforms":{"windows-x86_64": {
        "url":format!("{RELEASES}/download/v1.2.3/OpenNovel-1.2.3-Windows-x64-setup.exe"), "signature":"fixture"}}})
    }
    #[test]
    fn only_the_exact_project_installer_and_stable_version_are_accepted() {
        let good = manifest();
        assert!(validate_release(&good).is_ok());
        for url in [
            "http://github.com/a.exe",
            "https://evil.example/a.exe",
            "https://github.com/another/repo/releases/download/v1.2.3/a.exe",
        ] {
            let mut bad = good.clone();
            bad["platforms"]["windows-x86_64"]["url"] = url.into();
            assert!(validate_release(&bad).is_err());
        }
        for version in ["2.0.0", "1.2.3-beta", "1.2.3+other", "../x"] {
            let mut bad = good.clone();
            bad["version"] = version.into();
            assert!(validate_release(&bad).is_err());
        }
        for url in [
            "https://github.com.evil.test/x",
            "https://user@github.com/x",
            "http://github.com/x",
            "https://github.com:444/x",
        ] {
            assert!(!transport_allowed(&reqwest::Url::parse(url).unwrap()));
        }
    }
    #[test]
    fn all_profile_backup_preserves_original_databases_and_exports_and_refuses_a_busy_library() {
        let root = tempfile::tempdir().unwrap();
        let store = opennovel_core::profiles::ProfileStore::new(root.path().join("users")).unwrap();
        let first = store.resolve(None).unwrap();
        let current = opennovel_core::Library::open(&first.database).unwrap();
        let job = current
            .create_import(include_str!("../../../../examples/local-book.json"))
            .unwrap();
        current.run_import(&job, &AtomicBool::new(false)).unwrap();
        let second = store.create("原创第二用户").unwrap();
        let next = store.resolve(Some(&second.id)).unwrap();
        let other = opennovel_core::Library::open(&next.database).unwrap();
        assert!(
            backup_profiles(
                &store,
                &first.profile.id,
                &current,
                &root.path().join("backups")
            )
            .is_err()
        );
        drop(other);
        let before = std::fs::read(&next.database).unwrap();
        std::fs::create_dir_all(&next.exports).unwrap();
        std::fs::write(next.exports.join("original.txt"), "原创导出").unwrap();
        let backup = backup_profiles(
            &store,
            &first.profile.id,
            &current,
            &root.path().join("backups"),
        )
        .unwrap();
        assert_eq!(before, std::fs::read(&next.database).unwrap());
        assert!(
            backup
                .directory
                .join(&first.profile.id)
                .join("library.db")
                .is_file()
        );
        assert!(
            backup
                .directory
                .join(&second.id)
                .join("library.db")
                .is_file()
        );
        assert!(backup.directory.join("complete.json").is_file());
        assert!(opennovel_core::Library::check_available(&next.database).is_err());
        drop(backup);
        assert!(opennovel_core::Library::check_available(&next.database).is_ok());
        assert_eq!(
            std::fs::read_to_string(next.exports.join("original.txt")).unwrap(),
            "原创导出"
        );
        assert_eq!(current.snapshot().unwrap().books[0].saved_chapters, 2);
    }
}
