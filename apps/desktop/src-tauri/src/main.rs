#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use opennovel_core::sources::{Catalog, SourceHub, SourceInfo, identify, source_infos};
use opennovel_core::{
    BookDetails, ExportFormat, ExportReceipt, ImportChapter, JobDetails, Library, LibrarySnapshot,
};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex, atomic::AtomicBool},
};
use tauri::{Manager, State};

struct AppState {
    library: Arc<Library>,
    source: Arc<SourceHub>,
    cancellations: Arc<Mutex<HashMap<String, ActiveTask>>>,
    exports: PathBuf,
}

struct ActiveTask {
    book_id: String,
    token: Arc<AtomicBool>,
}

#[tauri::command]
fn source_capabilities() -> &'static [SourceInfo] {
    source_infos()
}

#[tauri::command]
async fn inspect_book(input: String, state: State<'_, AppState>) -> Result<Catalog, String> {
    let handle = Arc::clone(&state.source);
    blocking(move || handle.inspect(&input)).await
}

#[tauri::command]
async fn download_book(
    input: String,
    first: usize,
    last: usize,
    selection: Option<String>,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let handle = Arc::clone(&state.source);
    let library = Arc::clone(&state.library);
    let job = blocking(move || {
        handle.adapter(identify(&input)?.0)?;
        let catalog = handle.inspect(&input)?;
        if let Some(selection) = selection.filter(|s| !s.trim().is_empty()) {
            library.create_download_selected(
                &catalog,
                &opennovel_core::parse_selection(&selection, catalog.chapters.len())?,
            )
        } else {
            library.create_download(&catalog, first, last)
        }
    })
    .await?;
    launch(&job, &state, None).await?;
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
async fn book_details(book_id: String, state: State<'_, AppState>) -> Result<BookDetails, String> {
    let library = Arc::clone(&state.library);
    blocking(move || library.book_details(&book_id)).await
}

#[tauri::command]
async fn read_chapter(
    book_id: String,
    chapter_id: String,
    state: State<'_, AppState>,
) -> Result<ImportChapter, String> {
    let library = Arc::clone(&state.library);
    blocking(move || library.read_chapter(&book_id, &chapter_id)).await
}

#[tauri::command]
async fn export_job(
    job_id: String,
    format: String,
    state: State<'_, AppState>,
) -> Result<ExportReceipt, String> {
    let format = ExportFormat::parse(&format).map_err(|e| e.to_string())?;
    let library = Arc::clone(&state.library);
    let directory = state.exports.clone();
    blocking(move || library.export_job(&job_id, directory, format)).await
}

#[tauri::command]
async fn reveal_export(id: String, state: State<'_, AppState>) -> Result<(), String> {
    let library = Arc::clone(&state.library);
    blocking(move || {
        let path = PathBuf::from(library.export_location(&id)?);
        if !path.exists() {
            return Err(opennovel_core::Error::Invalid(
                "导出文件已移动或删除".into(),
            ));
        }
        #[cfg(target_os = "windows")]
        {
            std::process::Command::new("explorer.exe")
                .arg(
                    path.parent()
                        .ok_or_else(|| opennovel_core::Error::Invalid("导出位置无效".into()))?,
                )
                .spawn()?;
        }
        Ok(())
    })
    .await
}

#[tauri::command]
async fn job_details(job_id: String, state: State<'_, AppState>) -> Result<JobDetails, String> {
    let library = Arc::clone(&state.library);
    blocking(move || library.job_details(&job_id)).await
}

#[tauri::command]
async fn retry_chapters(
    job_id: String,
    chapters: Vec<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    launch(&job_id, &state, Some(chapters)).await
}

#[tauri::command]
async fn update_book(book_id: String, state: State<'_, AppState>) -> Result<String, String> {
    let library = Arc::clone(&state.library);
    let handle = Arc::clone(&state.source);
    let job = blocking(move || {
        let book = library
            .snapshot()?
            .books
            .into_iter()
            .find(|book| book.id == book_id)
            .ok_or_else(|| opennovel_core::Error::Invalid("作品不存在".into()))?;
        let url = book
            .source_url()
            .ok_or_else(|| opennovel_core::Error::Invalid("该来源不支持在线更新".into()))?;
        let catalog = handle.inspect(&url)?;
        library.create_download(&catalog, 1, catalog.chapters.len())
    })
    .await?;
    launch(&job, &state, None).await?;
    Ok(job)
}

#[tauri::command]
async fn import_book(json: String, state: State<'_, AppState>) -> Result<String, String> {
    let library = Arc::clone(&state.library);
    let job = blocking(move || library.create_import(&json)).await?;
    launch(&job, &state, None).await?;
    Ok(job)
}

#[tauri::command]
async fn resume_job(job_id: String, state: State<'_, AppState>) -> Result<(), String> {
    launch(&job_id, &state, None).await
}

async fn launch(job: &str, state: &AppState, selection: Option<Vec<String>>) -> Result<(), String> {
    let library = Arc::clone(&state.library);
    let requested = job.to_owned();
    let snapshot = blocking(move || library.job(&requested)).await?;
    if let Some(chapters) = &selection {
        let library = Arc::clone(&state.library);
        let requested = job.to_owned();
        let details = blocking(move || library.job_details(&requested)).await?;
        if chapters.is_empty()
            || chapters
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len()
                != chapters.len()
            || chapters.iter().any(|id| {
                !details
                    .items
                    .iter()
                    .any(|item| item.chapter_id == *id && item.retryable)
            })
        {
            return Err("请选择当前任务的失败章节；受限章节请先检查目录更新".into());
        }
    }
    if !matches!(
        snapshot.state.as_str(),
        "queued" | "paused" | "cancelled" | "failed" | "partial" | "blocked"
    ) {
        return Err("任务不能继续".into());
    }
    let mut registrations = state.cancellations.lock().map_err(|_| "任务管理器不可用")?;
    if registrations.contains_key(job)
        || registrations
            .values()
            .any(|run| run.book_id == snapshot.book_id)
    {
        return Err("该作品的任务仍在执行或停止中，请稍候".into());
    }
    let token = Arc::new(AtomicBool::new(false));
    registrations.insert(
        job.to_owned(),
        ActiveTask {
            book_id: snapshot.book_id.clone(),
            token: Arc::clone(&token),
        },
    );
    let library = Arc::clone(&state.library);
    let cancellations = Arc::clone(&state.cancellations);
    let job_id = job.to_owned();
    let handle = Arc::clone(&state.source);
    tauri::async_runtime::spawn_blocking(move || {
        // Errors are recorded by the core in the persisted task; never log the input text.
        let result = if snapshot.source == "local-json" {
            library.run_import(&job_id, &token)
        } else {
            handle.adapter(&snapshot.source).and_then(|client| {
                if let Some(chapters) = selection {
                    library.retry_chapters(&job_id, client, &token, &chapters)
                } else {
                    library.run_job(&job_id, client, &token)
                }
            })
        };
        if let Err(error) = result {
            let _ = library.record_worker_failure(&job_id, &error.to_string());
        }
        if let Ok(mut entries) = cancellations.lock() {
            entries.remove(&job_id);
        }
    });
    Ok(())
}

#[tauri::command]
fn cancel_job(job_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let registrations = state.cancellations.lock().map_err(|_| "任务管理器不可用")?;
    state
        .library
        .request_stop(
            &job_id,
            false,
            registrations.get(&job_id).map(|t| t.token.as_ref()),
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn pause_job(job_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let registrations = state.cancellations.lock().map_err(|_| "任务管理器不可用")?;
    state
        .library
        .request_stop(
            &job_id,
            true,
            registrations.get(&job_id).map(|t| t.token.as_ref()),
        )
        .map_err(|e| e.to_string())?;
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
            source_capabilities,
            library_snapshot,
            book_details,
            read_chapter,
            export_job,
            reveal_export,
            pause_job,
            job_details,
            retry_chapters,
            update_book,
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
