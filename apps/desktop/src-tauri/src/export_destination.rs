use opennovel_core::ExportDestination;
use std::path::Path;

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
