use opennovel_core::profiles::{LocalProfile, ProfilePaths, ProfileStore};
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{Manager, State};

pub struct ProfileState {
    pub store: ProfileStore,
    pub paths: ProfilePaths,
    pub switching: AtomicBool,
}
#[derive(serde::Serialize)]
pub struct ProfileOverview {
    current_id: String,
    profiles: Vec<LocalProfile>,
    data_directory: String,
}
#[tauri::command]
pub fn local_profiles(state: State<'_, ProfileState>) -> Result<ProfileOverview, String> {
    Ok(ProfileOverview {
        current_id: state.paths.profile.id.clone(),
        profiles: state.store.list().map_err(|e| e.to_string())?,
        data_directory: state.paths.directory.to_string_lossy().into_owned(),
    })
}
#[tauri::command]
pub fn create_local_profile(
    name: String,
    state: State<'_, ProfileState>,
) -> Result<LocalProfile, String> {
    state.store.create(&name).map_err(|e| e.to_string())
}
#[tauri::command]
pub async fn switch_local_profile(
    id: String,
    app: tauri::AppHandle,
    profile: State<'_, ProfileState>,
    state: State<'_, crate::AppState>,
) -> Result<(), String> {
    if id == profile.paths.profile.id {
        return Ok(());
    }
    let target = profile
        .store
        .resolve(Some(&id))
        .map_err(|e| e.to_string())?;
    if profile.switching.swap(true, Ordering::AcqRel) {
        return Err("正在切换用户，请稍等".into());
    }
    let checked = (|| {
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
                "请先暂停当前保存任务，等待文件生成完成，再切换用户。已保存内容会保留。".into(),
            );
        }
        Ok::<(), String>(())
    })();
    if let Err(error) = checked {
        profile.switching.store(false, Ordering::Release);
        return Err(error);
    }
    let store = profile.store.clone();
    let result = tauri::async_runtime::spawn_blocking(move || launch_profile(&store, &target))
        .await
        .map_err(|e| e.to_string())
        .and_then(|r| r);
    if result.is_err() {
        let _ = profile.store.select(&profile.paths.profile.id);
        profile.switching.store(false, Ordering::Release);
    } else {
        // Only a ready new window permits closing the old process and all its sessions.
        app.exit(0);
    }
    result
}

fn launch_profile(store: &ProfileStore, target: &ProfilePaths) -> Result<(), String> {
    let lease =
        opennovel_core::Library::check_available(&target.database).map_err(|e| e.to_string())?;
    let token = uuid::Uuid::new_v4().to_string();
    let marker = store.startup_marker(&token).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(marker.parent().ok_or("切换标记目录无效")?)
        .map_err(|e| e.to_string())?;
    let mut command =
        std::process::Command::new(std::env::current_exe().map_err(|e| e.to_string())?);
    command.args(["--profile", &target.profile.id, "--handoff", &token]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    drop(lease);
    let mut child = command
        .spawn()
        .map_err(|e| format!("无法打开另一用户，当前书库仍保留：{e}"))?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let result = loop {
        if child.try_wait().map_err(|e| e.to_string())?.is_some() {
            break Err("新用户启动失败，当前窗口和书库已保留，请检查该用户数据目录。".into());
        }
        if std::fs::read_to_string(&marker).is_ok_and(|s| s == target.profile.id) {
            break Ok(());
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            break Err("新用户启动超时，当前窗口和书库已保留，请稍后重试。".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    };
    if marker.is_file() {
        let _ = std::fs::remove_file(marker);
    }
    result
}

pub fn is_switching(app: &tauri::AppHandle) -> bool {
    app.try_state::<ProfileState>()
        .is_some_and(|s| s.switching.load(Ordering::Acquire))
}

#[cfg(test)]
mod tests {
    #[test]
    fn failed_child_start_preserves_current_profile_and_does_not_open_target_library() {
        let root = tempfile::tempdir().unwrap();
        let store = opennovel_core::profiles::ProfileStore::new(root.path()).unwrap();
        let original = store.resolve(None).unwrap();
        let next = store.create("原创第二用户").unwrap();
        let target = store.resolve(Some(&next.id)).unwrap();
        // A Rust test executable rejects --profile; this exercises the real child-exit path.
        assert!(
            super::launch_profile(&store, &target)
                .unwrap_err()
                .contains("启动失败")
        );
        assert_eq!(store.resolve(None).unwrap().profile.id, original.profile.id);
        assert!(!target.database.exists());
        assert_eq!(
            std::fs::read_dir(root.path().join("handoff"))
                .unwrap()
                .count(),
            0
        );
    }
}
