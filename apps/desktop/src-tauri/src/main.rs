#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod account_download;
mod accounts;
mod auto_txt;
mod capture;
use account_download::{account_download_jobs, resume_account_download, start_account_download};
use accounts::{account_list, close_account, create_account, open_account};
use capture::{confirm_account_page, preview_account_page};

use opennovel_core::sources::{Catalog, SourceHub, SourceInfo, identify, source_infos};
use opennovel_core::{
    BookDetails, BookPreferences, DiagnosticReport, ExportFormat, ExportReceipt, ImportChapter,
    JobDetails, Library, LibrarySnapshot,
};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex, atomic::AtomicBool},
};
use tauri::{Manager, State};

struct AppState {
    auto_txt: auto_txt::AutoTxtService,
    library: Arc<Library>,
    source: Arc<SourceHub>,
    cancellations: Arc<Mutex<HashMap<String, ActiveTask>>>,
    exports: PathBuf,
    diagnostic_preview: Mutex<Option<DiagnosticReport>>,
    restored: Mutex<HashMap<String, PathBuf>>,
    batches: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
    proposals: Mutex<HashMap<String, PendingUpdate>>,
    sessions: Mutex<HashMap<String, accounts::BrowserSession>>,
    session_root: PathBuf,
    captures: Mutex<HashMap<String, capture::PendingCapture>>,
}
struct PendingUpdate {
    book_id: String,
    catalog: Catalog,
    previous_hash: String,
    created: std::time::Instant,
}
#[derive(serde::Serialize)]
struct UpdatePreview {
    token: String,
    diff: opennovel_core::CatalogUpdate,
}
#[tauri::command]
async fn preview_catalog_update(
    book_id: String,
    state: State<'_, AppState>,
) -> Result<UpdatePreview, String> {
    let library = Arc::clone(&state.library);
    let hub = Arc::clone(&state.source);
    let (catalog, diff) = blocking(move || {
        let book = library.book_details(&book_id)?.book;
        let catalog = hub.inspect(
            &book
                .source_url()
                .ok_or_else(|| opennovel_core::Error::Invalid("该来源没有在线目录".into()))?,
        )?;
        let diff = library.preview_catalog_update(&book_id, &catalog)?;
        Ok((catalog, diff))
    })
    .await?;
    let mut proposals = state.proposals.lock().map_err(|_| "目录预览不可用")?;
    proposals.retain(|_, p| p.created.elapsed() < std::time::Duration::from_secs(600));
    if proposals.len() >= 8 {
        return Err("待确认目录过多，请稍后重新预览".into());
    }
    let token = if diff.changed {
        uuid::Uuid::new_v4().to_string()
    } else {
        String::new()
    };
    if diff.changed {
        let book_id = state
            .library
            .snapshot()
            .map_err(|e| e.to_string())?
            .books
            .into_iter()
            .find(|b| b.source == catalog.source && b.remote_id == catalog.remote_id)
            .ok_or("作品不存在")?
            .id;
        proposals.insert(
            token.clone(),
            PendingUpdate {
                book_id,
                catalog,
                previous_hash: diff.previous_hash.clone(),
                created: std::time::Instant::now(),
            },
        );
    }
    Ok(UpdatePreview { token, diff })
}
#[tauri::command]
async fn accept_catalog_update(
    token: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let proposal = state
        .proposals
        .lock()
        .map_err(|_| "目录预览不可用")?
        .remove(&token)
        .ok_or("目录预览已失效，请重新读取")?;
    if proposal.created.elapsed() >= std::time::Duration::from_secs(600) {
        return Err("目录预览已过期，请重新读取".into());
    }
    let library = Arc::clone(&state.library);
    let automatic = state.auto_txt.clone();
    let changed_book = proposal.book_id.clone();
    let result = blocking(move || {
        library.accept_catalog_update(
            &proposal.book_id,
            &proposal.catalog,
            &proposal.previous_hash,
        )
    })
    .await;
    if result.is_ok() {
        let _ = automatic.request(&changed_book);
    }
    result
}
#[tauri::command]
async fn refresh_book_chapter(
    book_id: String,
    chapter_id: String,
    state: State<'_, AppState>,
) -> Result<opennovel_core::RefreshReceipt, String> {
    let library = Arc::clone(&state.library);
    let automatic = state.auto_txt.clone();
    let changed_book = book_id.clone();
    let hub = Arc::clone(&state.source);
    let result = blocking(move || {
        let book = library.book_details(&book_id)?.book;
        library.refresh_chapter(
            &book_id,
            &chapter_id,
            hub.adapter(&book.source)?,
            &AtomicBool::new(false),
        )
    })
    .await;
    if result.is_ok() {
        let _ = automatic.request(&changed_book);
    }
    result
}
#[tauri::command]
async fn book_history(
    book_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<opennovel_core::HistoryEntry>, String> {
    let library = Arc::clone(&state.library);
    blocking(move || library.history(&book_id)).await
}
#[tauri::command]
async fn history_details(id: String, state: State<'_, AppState>) -> Result<BookDetails, String> {
    let library = Arc::clone(&state.library);
    blocking(move || library.history_details(&id)).await
}
#[tauri::command]
async fn read_history(
    id: String,
    chapter_id: String,
    state: State<'_, AppState>,
) -> Result<ImportChapter, String> {
    let library = Arc::clone(&state.library);
    blocking(move || library.read_history(&id, &chapter_id)).await
}
#[tauri::command]
async fn export_history(
    id: String,
    format: String,
    state: State<'_, AppState>,
) -> Result<ExportReceipt, String> {
    let format = ExportFormat::parse(&format).map_err(|e| e.to_string())?;
    let library = Arc::clone(&state.library);
    let directory = state.exports.clone();
    blocking(move || library.export_history(&id, directory, format)).await
}

#[tauri::command]
async fn search_books(
    source: String,
    query: String,
    state: State<'_, AppState>,
) -> Result<Vec<opennovel_core::sources::SearchResult>, String> {
    let hub = Arc::clone(&state.source);
    blocking(move || hub.search(&source, &query)).await
}
#[tauri::command]
async fn batch_snapshot(
    state: State<'_, AppState>,
) -> Result<Vec<opennovel_core::BatchSnapshot>, String> {
    let library = Arc::clone(&state.library);
    blocking(move || library.batches()).await
}
#[tauri::command]
async fn create_batch(
    inputs: Vec<String>,
    selection: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let library = Arc::clone(&state.library);
    let id = blocking(move || library.create_batch(&inputs, &selection)).await?;
    launch_batch(&id, &state)?;
    Ok(id)
}
#[tauri::command]
fn resume_batch(id: String, state: State<'_, AppState>) -> Result<(), String> {
    launch_batch(&id, &state)
}
fn launch_batch(id: &str, state: &AppState) -> Result<(), String> {
    let mut active = state.batches.lock().map_err(|_| "批次管理器不可用")?;
    if !active.is_empty() {
        return Err("已有批次在执行或停止中，请稍后继续".into());
    }
    let batch = state.library.batch(id).map_err(|e| e.to_string())?;
    if !matches!(batch.state.as_str(), "queued" | "paused") {
        return Err("批次不在可继续状态".into());
    }
    let stop = Arc::new(AtomicBool::new(false));
    active.insert(id.into(), Arc::clone(&stop));
    let id = id.to_owned();
    let library = Arc::clone(&state.library);
    let hub = Arc::clone(&state.source);
    let active = Arc::clone(&state.batches);
    tauri::async_runtime::spawn_blocking(move || {
        if let Err(e) = library.run_batch(&id, hub.as_ref(), &stop) {
            let _ = library.record_batch_failure(&id, &e.to_string());
        }
        if let Ok(mut workers) = active.lock() {
            workers.remove(&id);
        }
    });
    Ok(())
}
#[tauri::command]
fn stop_batch(id: String, paused: bool, state: State<'_, AppState>) -> Result<(), String> {
    let active = state.batches.lock().map_err(|_| "批次管理器不可用")?;
    state
        .library
        .stop_batch(&id, paused, active.get(&id).map(|t| t.as_ref()))
        .map_err(|e| e.to_string())
}

#[derive(serde::Serialize)]
struct MaintenanceReceipt {
    id: String,
    path: String,
    books: u64,
    chapters: u64,
}

#[tauri::command]
async fn book_preferences(state: State<'_, AppState>) -> Result<Vec<BookPreferences>, String> {
    let library = Arc::clone(&state.library);
    blocking(move || library.preferences()).await
}
#[tauri::command]
async fn organize_book(
    book_id: String,
    tags: Vec<String>,
    archived: bool,
    state: State<'_, AppState>,
) -> Result<BookPreferences, String> {
    let library = Arc::clone(&state.library);
    blocking(move || library.organize(&book_id, &tags, archived)).await
}
#[tauri::command]
async fn backup_library(state: State<'_, AppState>) -> Result<MaintenanceReceipt, String> {
    let library = Arc::clone(&state.library);
    let id = uuid::Uuid::new_v4().to_string();
    let root = state
        .exports
        .parent()
        .expect("data directory")
        .join("backups");
    blocking(move || {
        std::fs::create_dir_all(&root)?;
        let target = root.join(format!("backup-{id}"));
        let manifest = library.backup(&target)?;
        Ok(MaintenanceReceipt {
            id,
            path: target.to_string_lossy().into_owned(),
            books: manifest.books,
            chapters: manifest.chapters,
        })
    })
    .await
}
#[tauri::command]
async fn restore_library(
    backup_path: String,
    state: State<'_, AppState>,
) -> Result<MaintenanceReceipt, String> {
    let id = uuid::Uuid::new_v4().to_string();
    let target = state
        .exports
        .parent()
        .expect("data directory")
        .join(format!("restored-{id}"));
    let receipt = blocking(move || {
        let library = Library::restore(backup_path, &target)?;
        let report = library.diagnostics()?;
        Ok(MaintenanceReceipt {
            id,
            path: target.join("library.db").to_string_lossy().into_owned(),
            books: report.books,
            chapters: report.saved_chapters,
        })
    })
    .await?;
    state
        .restored
        .lock()
        .map_err(|_| "恢复记录不可用")?
        .insert(receipt.id.clone(), PathBuf::from(&receipt.path));
    Ok(receipt)
}
#[tauri::command]
fn open_restored_library(id: String, state: State<'_, AppState>) -> Result<(), String> {
    let path = state
        .restored
        .lock()
        .map_err(|_| "恢复记录不可用")?
        .get(&id)
        .cloned()
        .ok_or("恢复记录不存在")?;
    // Only this executable and a successfully restored database are accepted; no shell command.
    std::process::Command::new(std::env::current_exe().map_err(|e| e.to_string())?)
        .arg("--database")
        .arg(path)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}
#[tauri::command]
async fn preview_diagnostics(state: State<'_, AppState>) -> Result<DiagnosticReport, String> {
    let library = Arc::clone(&state.library);
    let report = blocking(move || library.diagnostics()).await?;
    *state
        .diagnostic_preview
        .lock()
        .map_err(|_| "诊断预览不可用")? = Some(report.clone());
    Ok(report)
}
#[tauri::command]
async fn save_diagnostics(state: State<'_, AppState>) -> Result<String, String> {
    let report = state
        .diagnostic_preview
        .lock()
        .map_err(|_| "诊断预览不可用")?
        .clone()
        .ok_or("请先预览诊断信息")?;
    let root = state
        .exports
        .parent()
        .expect("data directory")
        .join("diagnostics");
    blocking(move || {
        std::fs::create_dir_all(&root)?;
        let path = root.join(format!("diagnostic-{}.json", uuid::Uuid::new_v4()));
        report.save(&path)?;
        Ok(path.to_string_lossy().into_owned())
    })
    .await
}

struct ActiveTask {
    book_id: String,
    token: Arc<AtomicBool>,
    account_id: Option<String>,
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
    if state
        .library
        .active_batch_for_job(job)
        .map_err(|e| e.to_string())?
        .is_some()
    {
        return Err("该任务由批次管理，请在批次面板继续或停止".into());
    }
    let library = Arc::clone(&state.library);
    let requested = job.to_owned();
    let snapshot = blocking(move || library.job(&requested)).await?;
    if snapshot.source == opennovel_core::QIDIAN_SESSION_SOURCE {
        return Err("起点自动任务请在平台账户选择官网会话后继续".into());
    }
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
            account_id: None,
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
            let args = std::env::args().skip(1).collect::<Vec<_>>();
            let database = match args.as_slice() {
                [] => app.path().app_data_dir()?.join("library.db"),
                [flag, path] if flag == "--database" => std::path::absolute(path)?,
                _ => return Err("参数无效；可使用 --database <path> 打开独立书库".into()),
            };
            let directory = database.parent().ok_or("书库目录无效")?;
            let library = Arc::new(Library::open(&database)?);
            let auto_txt = auto_txt::AutoTxtService::start(
                Arc::clone(&library),
                directory.join("exports").join("automatic-txt"),
            )?;
            app.manage(AppState {
                library,
                auto_txt,
                source: Arc::default(),
                cancellations: Arc::default(),
                exports: directory.join("exports"),
                diagnostic_preview: Mutex::new(None),
                restored: Mutex::default(),
                batches: Arc::default(),
                proposals: Mutex::default(),
                sessions: Mutex::default(),
                session_root: directory.join("session-cache"),
                captures: Mutex::default(),
            });
            tauri::WebviewWindowBuilder::from_config(app, &app.config().app.windows[0])?
                .data_directory(directory.join("ui-webview"))
                .on_navigation(|url| accounts::local_ipc_allowed("main", url))
                .build()?;
            Ok(())
        })
        .invoke_handler({
            let dispatch: Box<dyn Fn(tauri::ipc::Invoke<tauri::Wry>) -> bool + Send + Sync> =
                Box::new(tauri::generate_handler![
                    auto_txt::automatic_txt_snapshot,
                    auto_txt::retry_automatic_txt,
                    account_list,
                    preview_account_page,
                    confirm_account_page,
                    start_account_download,
                    resume_account_download,
                    account_download_jobs,
                    create_account,
                    open_account,
                    accounts::focus_account,
                    close_account,
                    preview_catalog_update,
                    accept_catalog_update,
                    refresh_book_chapter,
                    book_history,
                    history_details,
                    read_history,
                    export_history,
                    search_books,
                    batch_snapshot,
                    create_batch,
                    resume_batch,
                    stop_batch,
                    book_preferences,
                    organize_book,
                    backup_library,
                    restore_library,
                    open_restored_library,
                    preview_diagnostics,
                    save_diagnostics,
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
                ]);
            move |invoke: tauri::ipc::Invoke<tauri::Wry>| {
                let webview = invoke.message.webview_ref();
                if !webview
                    .url()
                    .is_ok_and(|url| accounts::local_ipc_allowed(webview.label(), &url))
                {
                    invoke.resolver.reject("远程页面不能调用本地书库命令");
                    return true;
                }
                dispatch(invoke)
            }
        })
        .run(tauri::generate_context!())
        .expect("无法启动 OpenNovel 桌面应用");
}
