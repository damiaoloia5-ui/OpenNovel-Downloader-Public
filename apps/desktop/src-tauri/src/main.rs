#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use opennovel_core::sources::{Catalog, Fanqie};
use opennovel_core::{ExportFormat, ExportReceipt, Library, LibrarySnapshot};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
};
use tauri::{Manager, State};

struct AppState {
    library: Arc<Library>,
    source: Arc<OnceLock<Fanqie>>,
    cancellations: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
    exports: PathBuf,
}

fn source(handle: &OnceLock<Fanqie>) -> opennovel_core::Result<&Fanqie> {
    if let Some(client) = handle.get() {
        return Ok(client);
    }
    let client = Fanqie::new()?;
    let _ = handle.set(client);
    Ok(handle.get().expect("initialized source"))
}

#[tauri::command]
async fn inspect_book(input: String, state: State<'_, AppState>) -> Result<Catalog, String> {
    let handle = Arc::clone(&state.source);
    blocking(move || source(&handle)?.inspect(&input)).await
}

#[tauri::command]
async fn download_book(
    input: String,
    first: usize,
    last: usize,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let handle = Arc::clone(&state.source);
    let library = Arc::clone(&state.library);
    let job = blocking(move || {
        let catalog = source(&handle)?.inspect(&input)?;
        library.create_download(&catalog, first, last)
    })
    .await?;
    launch(&job, &state).await?;
    Ok(job)
}

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> opennovel_core::Result<T> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn library_snapshot(state: State<'_, AppState>) -> Result<LibrarySnapshot, String> {
    let library = Arc::clone(&state.library);
    blocking(move || library.snapshot()).await
}

#[tauri::command]
async fn import_book(json: String, state: State<'_, AppState>) -> Result<String, String> {
    let library = Arc::clone(&state.library);
    let job = blocking(move || library.create_import(&json)).await?;
    launch(&job, &state).await?;
    Ok(job)
}

#[tauri::command]
async fn resume_job(job_id: String, state: State<'_, AppState>) -> Result<(), String> {
    launch(&job_id, &state).await
}

async fn launch(job: &str, state: &AppState) -> Result<(), String> {
    let library = Arc::clone(&state.library);
    let requested = job.to_owned();
    let snapshot = blocking(move || library.job(&requested)).await?;
    if !matches!(
        snapshot.state.as_str(),
        "queued" | "paused" | "cancelled" | "failed" | "partial" | "blocked"
    ) {
        return Err("任务不能继续".into());
    }
    let mut registrations = state.cancellations.lock().map_err(|_| "任务管理器不可用")?;
    if registrations.contains_key(job) {
        return Err("任务正在执行".into());
    }
    let token = Arc::new(AtomicBool::new(false));
    registrations.insert(job.to_owned(), Arc::clone(&token));
    let library = Arc::clone(&state.library);
    let cancellations = Arc::clone(&state.cancellations);
    let job_id = job.to_owned();
    let handle = Arc::clone(&state.source);
    tauri::async_runtime::spawn_blocking(move || {
        // Errors are recorded by the core in the persisted task; never log the input text.
        let _result = if snapshot.source == "local-json" {
            library.run_import(&job_id, &token)
        } else {
            source(&handle).and_then(|client| library.run_job(&job_id, client, &token))
        };
        if let Ok(mut entries) = cancellations.lock() {
            entries.remove(&job_id);
        }
    });
    Ok(())
}

#[tauri::command]
fn cancel_job(job_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let registrations = state.cancellations.lock().map_err(|_| "任务管理器不可用")?;
    let token = registrations
        .get(&job_id)
        .ok_or("任务没有在运行，请刷新状态")?;
    token.store(true, Ordering::Release);
    Ok(())
}

#[tauri::command]
async fn export_book(
    book_id: String,
    format: String,
    state: State<'_, AppState>,
) -> Result<ExportReceipt, String> {
    let format = ExportFormat::parse(&format).map_err(|error| error.to_string())?;
    let library = Arc::clone(&state.library);
    let directory = state.exports.clone();
    blocking(move || library.export(&book_id, directory, format)).await
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let directory = app.path().app_data_dir()?;
            app.manage(AppState {
                library: Arc::new(Library::open(directory.join("library.db"))?),
                source: Arc::default(),
                cancellations: Arc::default(),
                exports: directory.join("exports"),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            library_snapshot,
            import_book,
            inspect_book,
            download_book,
            resume_job,
            cancel_job,
            export_book
        ])
        .run(tauri::generate_context!())
        .expect("无法启动 OpenNovel 桌面应用");
}
