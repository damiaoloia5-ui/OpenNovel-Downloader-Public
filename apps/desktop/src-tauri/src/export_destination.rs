use opennovel_core::ExportDestination;
use std::path::Path;

pub fn preflight(
    destination: Option<&opennovel_core::SaveDestination>,
    default_root: &Path,
) -> opennovel_core::Result<()> {
    // Legacy callers with no choice keep their book preference. New callers
    // explicitly select either a custom directory or the application default.
    if let Some(destination) = destination {
        if destination.directory.is_some() {
            return destination.validate();
        }
        let check = || -> std::io::Result<()> {
            std::fs::create_dir_all(default_root)?;
            let _probe = tempfile::NamedTempFile::new_in(default_root)?;
            Ok(())
        };
        check().map_err(|e| {
            opennovel_core::Error::Invalid(format!(
                "默认保存位置不可写（{}）：{e}。请先选择其他文件夹，再开始保存。",
                default_root.display()
            ))
        })?;
    }
    Ok(())
}

#[tauri::command]
pub fn default_export_root(state: tauri::State<'_, crate::AppState>) -> String {
    state.exports.to_string_lossy().into_owned()
}

#[tauri::command]
pub async fn choose_save_directory(
    current: Option<String>,
    window: tauri::WebviewWindow,
    state: tauri::State<'_, crate::AppState>,
) -> Result<Option<String>, String> {
    static PICKING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    use std::sync::atomic::Ordering;
    if PICKING.swap(true, Ordering::AcqRel) {
        return Err("文件夹选择窗口已经打开".into());
    }
    struct Guard;
    impl Drop for Guard {
        fn drop(&mut self) {
            PICKING.store(false, Ordering::Release);
        }
    }
    let _guard = Guard;
    let initial = current.unwrap_or_else(|| state.exports.to_string_lossy().into_owned());
    #[cfg(windows)]
    let owner = window.hwnd().map_err(|e| e.to_string())?.0 as isize;
    #[cfg(not(windows))]
    let owner = {
        let _ = window;
        0
    };
    tauri::async_runtime::spawn_blocking(move || crate::folder_dialog::pick(owner, &initial))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn book_export_destination(
    book_id: String,
    state: tauri::State<'_, crate::AppState>,
) -> Result<ExportDestination, String> {
    let library = std::sync::Arc::clone(&state.library);
    let root = state.exports.clone();
    crate::blocking(move || library.export_destination(&book_id, root)).await
}

#[tauri::command]
pub async fn set_book_export_directory(
    book_id: String,
    directory: Option<String>,
    state: tauri::State<'_, crate::AppState>,
) -> Result<ExportDestination, String> {
    let library = std::sync::Arc::clone(&state.library);
    let root = state.exports.clone();
    let owner = book_id.clone();
    let result = crate::blocking(move || {
        library.set_export_directory(&book_id, directory.as_deref().map(Path::new))?;
        library.export_destination(&book_id, root)
    })
    .await?;
    state.auto_txt.request(&owner)?;
    Ok(result)
}

#[tauri::command]
pub async fn choose_book_export_directory(
    book_id: String,
    window: tauri::WebviewWindow,
    state: tauri::State<'_, crate::AppState>,
) -> Result<Option<ExportDestination>, String> {
    static PICKING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    use std::sync::atomic::Ordering;
    if PICKING.swap(true, Ordering::AcqRel) {
        return Err("文件夹选择窗口已经打开".into());
    }
    struct Guard;
    impl Drop for Guard {
        fn drop(&mut self) {
            PICKING.store(false, Ordering::Release);
        }
    }
    let _guard = Guard;
    let current = state
        .library
        .export_destination(&book_id, &state.exports)
        .map_err(|e| e.to_string())?;
    #[cfg(windows)]
    let owner = window.hwnd().map_err(|e| e.to_string())?.0 as isize;
    #[cfg(not(windows))]
    let owner = {
        let _ = window;
        0
    };
    let path = tauri::async_runtime::spawn_blocking(move || {
        crate::folder_dialog::pick(owner, &current.directory)
    })
    .await
    .map_err(|e| e.to_string())??;
    match path {
        Some(path) => set_book_export_directory(book_id, Some(path), state)
            .await
            .map(Some),
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opennovel_core::SaveDestination;

    #[test]
    fn preflight_rejects_unusable_paths() {
        let root = tempfile::tempdir().unwrap();
        let default = root.path().join("exports");
        preflight(
            Some(&SaveDestination {
                directory: None,
                ..Default::default()
            }),
            &default,
        )
        .unwrap();
        assert!(default.is_dir());
        assert_eq!(std::fs::read_dir(&default).unwrap().count(), 0);
        let occupied = root.path().join("file");
        std::fs::write(&occupied, "original").unwrap();
        assert!(
            preflight(
                Some(&SaveDestination {
                    directory: None,
                    ..Default::default()
                }),
                &occupied
            )
            .unwrap_err()
            .to_string()
            .contains("请先选择其他文件夹")
        );
        let custom = root.path().join("chosen");
        std::fs::create_dir(&custom).unwrap();
        // A valid custom path works even if the application's default is unwritable.
        preflight(
            Some(&SaveDestination {
                directory: Some(custom.clone()),
                ..Default::default()
            }),
            &occupied,
        )
        .unwrap();
        assert_eq!(std::fs::read_dir(custom).unwrap().count(), 0);
    }
}
