#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use crate::operation::OperationState as State;
mod account_download;
mod accounts;
mod auto_txt;
mod capture;
mod export_destination;
mod folder_dialog;
mod install_check;
mod local_engine;
mod official_reader;
mod operation;
mod profiles;
mod software_update;
use account_download::{account_download_jobs, resume_account_download, start_account_download};
use accounts::{account_list, close_account, create_account, open_account};
use capture::{confirm_account_page, preview_account_page, preview_platform_catalog};

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
use tauri::Manager;

struct AppState {
    engine: Arc<local_engine::EngineControl>,
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
    blocking(move || {
        let book = library.history_details(&id)?.book.id;
        let output = library.export_destination(&book, directory)?;
        library.export_history(&id, output.directory, format)
    })
    .await
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
    source: String,
    token: Arc<AtomicBool>,
    account_id: Option<String>,
}

#[tauri::command]
fn source_capabilities(state: State<'_, AppState>) -> Vec<SourceInfo> {
    let mut sources = source_infos().to_vec();
    sources.push(SourceInfo {
        id: opennovel_core::sources::LOCAL_ENGINE_SOURCE,
        name: "番茄本地引擎",
        can_inspect: true,
        can_download: state.engine.enabled() || state.engine.remembered(),
        can_search: false,
        note: "默认关闭的隔离实验组件，仅明确免费章；正文逐章验证，不保证全书。与网页书库独立，不合并旧任务。",
        example: "https://fanqienovel.com/page/作品ID",
    });
    sources
}

#[tauri::command]
async fn local_engine_status(
    state: State<'_, AppState>,
) -> Result<local_engine::EngineStatus, String> {
    let engine = Arc::clone(&state.engine);
    blocking(move || Ok(engine.status())).await
}

#[tauri::command]
async fn set_local_engine(
    enabled: bool,
    state: State<'_, AppState>,
) -> Result<local_engine::EngineStatus, String> {
    if !enabled {
        let snapshot = state.library.snapshot().map_err(|e| e.to_string())?;
        let active = state.cancellations.lock().map_err(|_| "任务管理器不可用")?;
        if snapshot.jobs.iter().any(|j| {
            j.source == opennovel_core::sources::LOCAL_ENGINE_SOURCE && active.contains_key(&j.id)
        }) {
            return Err("请先暂停本地引擎任务，待进程停止后再关闭组件".into());
        }
    }
    let engine = Arc::clone(&state.engine);
    blocking(move || engine.set(enabled)).await
}

#[tauri::command]
async fn inspect_book(input: String, state: State<'_, AppState>) -> Result<Catalog, String> {
    let handle = Arc::clone(&state.source);
    let engine = state.engine.enabled();
    blocking(move || inspect_channel(&handle, &input, engine)).await
}

fn inspect_channel(hub: &SourceHub, input: &str, engine: bool) -> opennovel_core::Result<Catalog> {
    let (source, id) = identify(input)?;
    if engine && source == "fanqie-web" {
        opennovel_core::sources::Fanqie::new()?.inspect_engine(&id)
    } else {
        hub.inspect(input)
    }
}

#[tauri::command]
async fn download_book(
    input: String,
    first: usize,
    last: usize,
    selection: Option<String>,
    destination: Option<opennovel_core::SaveDestination>,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let handle = Arc::clone(&state.source);
    let library = Arc::clone(&state.library);
    let engine = state.engine.enabled();
    let root = state.exports.clone();
    let job = blocking(move || {
        export_destination::preflight(destination.as_ref(), &root)?;
        let catalog = inspect_channel(&handle, &input, engine)?;
        let positions = if let Some(selection) = selection.filter(|s| !s.trim().is_empty()) {
            opennovel_core::parse_selection(&selection, catalog.chapters.len())?
        } else {
            if first == 0 || last < first || last > catalog.chapters.len() {
                return Err(opennovel_core::Error::Invalid("章节范围超出目录".into()));
            }
            (first..=last).collect()
        };
        if catalog.source == opennovel_core::sources::LOCAL_ENGINE_SOURCE {
            return library.create_engine_download_with_destination(
                &catalog,
                &positions,
                destination.as_ref(),
            );
        }
        handle.adapter(&catalog.source)?;
        library.create_download_selected_with_destination(
            &catalog,
            &positions,
            destination.as_ref(),
        )
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
async fn library_snapshot_if_changed(
    known: Option<String>,
    state: State<'_, AppState>,
) -> Result<Option<opennovel_core::VersionedSnapshot>, String> {
    let library = Arc::clone(&state.library);
    blocking(move || library.snapshot_if_changed(known.as_deref())).await
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
    blocking(move || {
        let book = library.job(&job_id)?.book_id;
        let output = library.export_destination(&book, directory)?;
        library.export_job(&job_id, output.directory, format)
    })
    .await
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
    let engine = Arc::clone(&state.engine);
    let job = blocking(move || {
        let book = library
            .snapshot()?
            .books
            .into_iter()
            .find(|book| book.id == book_id)
            .ok_or_else(|| opennovel_core::Error::Invalid("作品不存在".into()))?;
        if book.source == opennovel_core::sources::LOCAL_ENGINE_SOURCE {
            engine.require_ready()?;
            let catalog =
                opennovel_core::sources::Fanqie::new()?.inspect_engine(&book.remote_id)?;
            return library.create_engine_download(
                &catalog,
                &(1..=catalog.chapters.len()).collect::<Vec<_>>(),
            );
        }
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
async fn preview_txt_supplement(
    book_id: String,
    text: String,
    state: State<'_, AppState>,
) -> Result<opennovel_core::TxtSupplementPreview, String> {
    let library = Arc::clone(&state.library);
    blocking(move || library.preview_txt_supplement(&book_id, &text)).await
}

#[tauri::command]
async fn confirm_txt_supplement(
    book_id: String,
    text: String,
    fingerprint: String,
    state: State<'_, AppState>,
) -> Result<opennovel_core::TxtSupplementPreview, String> {
    let library = Arc::clone(&state.library);
    blocking(move || library.confirm_txt_supplement(&book_id, &text, &fingerprint)).await
}

#[tauri::command]
async fn import_book(
    json: String,
    destination: Option<opennovel_core::SaveDestination>,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let library = Arc::clone(&state.library);
    let root = state.exports.clone();
    let job = blocking(move || {
        export_destination::preflight(destination.as_ref(), &root)?;
        library.create_import_with_destination(&json, destination.as_ref())
    })
    .await?;
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
    if opennovel_core::session_platform(&snapshot.source).is_some() {
        return Err("官网自动任务请在对应平台账户选择官网会话后继续".into());
    }
    if matches!(snapshot.state.as_str(), "partial" | "blocked")
        && snapshot.blocked > 0
        && snapshot.failed == 0
        && snapshot.saved + snapshot.blocked == snapshot.total
    {
        return Err("本次范围只剩网页受限章节，重复继续不会补齐。请检查官网目录更新；番茄作品也可在本书保存页导入已有完整 TXT。".into());
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
    let traffic = if snapshot.source == opennovel_core::sources::LOCAL_ENGINE_SOURCE {
        match opennovel_core::traffic::TrafficGate::for_user("fanqie").and_then(|g| g.acquire()) {
            Ok(lease) => Some(lease),
            Err(error) => {
                state
                    .library
                    .record_worker_failure(job, &error.to_string())
                    .map_err(|e| e.to_string())?;
                return Err(error.to_string());
            }
        }
    } else {
        None
    };
    let token = Arc::new(AtomicBool::new(false));
    {
        let mut registrations = state.cancellations.lock().map_err(|_| "任务管理器不可用")?;
        if registrations.contains_key(job)
            || registrations
                .values()
                .any(|run| run.book_id == snapshot.book_id)
        {
            return Err("该作品的任务仍在执行或停止中，请稍候".into());
        }
        if snapshot.source == opennovel_core::sources::LOCAL_ENGINE_SOURCE
            && registrations.values().any(|r| r.source == snapshot.source)
        {
            let message = "本地增强已有保存任务，请完成或暂停后再保存另一本，避免叠加平台请求";
            state
                .library
                .record_worker_failure(job, message)
                .map_err(|e| e.to_string())?;
            return Err(message.into());
        }
        registrations.insert(
            job.to_owned(),
            ActiveTask {
                book_id: snapshot.book_id.clone(),
                source: snapshot.source.clone(),
                token: Arc::clone(&token),
                account_id: None,
            },
        );
    }
    // Register preparation too, so cancel/disable cannot miss a pending start.
    let engine_runtime = if snapshot.source == opennovel_core::sources::LOCAL_ENGINE_SOURCE {
        let engine = Arc::clone(&state.engine);
        let prepared = blocking(move || engine.require_ready()).await;
        if token.load(std::sync::atomic::Ordering::Acquire) || prepared.is_err() {
            if let Ok(mut entries) = state.cancellations.lock() {
                entries.remove(job);
            }
            let message = if token.load(std::sync::atomic::Ordering::Acquire) {
                "本次准备已取消，原进度保留".into()
            } else {
                prepared.err().unwrap_or_else(|| "本地组件准备失败".into())
            };
            state
                .library
                .record_worker_failure(job, &message)
                .map_err(|e| e.to_string())?;
            return Err(message);
        }
        Some(prepared.map_err(|e| e.to_string())?)
    } else {
        None
    };
    let library = Arc::clone(&state.library);
    let cancellations = Arc::clone(&state.cancellations);
    let job_id = job.to_owned();
    let handle = Arc::clone(&state.source);
    let engine_control = Arc::clone(&state.engine);
    tauri::async_runtime::spawn_blocking(move || {
        // Errors are recorded by the core in the persisted task; never log the input text.
        let result = if snapshot.source == "local-json" {
            library.run_import(&job_id, &token)
        } else if let Some(runtime) = engine_runtime {
            (|| {
                let book = library.book_summary(&snapshot.book_id)?;
                let items = library.pending_download_ids(&job_id, selection.as_deref())?;
                let client = local_engine::EngineAdapter::new(runtime, book.remote_id, items)
                    .with_control(engine_control)
                    .with_traffic(traffic);
                if let Some(chapters) = selection {
                    library.retry_chapters(&job_id, &client, &token, &chapters)
                } else {
                    library.run_job(&job_id, &client, &token)
                }
            })()
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
    selection: Option<String>,
    state: State<'_, AppState>,
) -> Result<ExportReceipt, String> {
    let format = ExportFormat::parse(&format).map_err(|error| error.to_string())?;
    let library = Arc::clone(&state.library);
    let directory = state.exports.clone();
    blocking(move || {
        let output = library.export_destination(&book_id, directory)?;
        match selection {
            Some(selection) => {
                library.export_selected(&book_id, &selection, output.directory, format)
            }
            None => library.export(&book_id, output.directory, format),
        }
    })
    .await
}

fn main() {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if args.first().is_some_and(|a| a == "--verify-update") {
        let code = match args.as_slice() {
            [_, manifest, installer] => {
                if software_update::verify_local(
                    &PathBuf::from(manifest),
                    &PathBuf::from(installer),
                )
                .is_ok()
                {
                    0
                } else {
                    1
                }
            }
            _ => 2,
        };
        std::process::exit(code);
    }
    if args.first().is_some_and(|a| a == "--adopt-library") {
        let result = (|| -> opennovel_core::Result<()> {
            let [_, path] = args.as_slice() else {
                return Err(opennovel_core::Error::Invalid(
                    "需要一个原有书库路径".into(),
                ));
            };
            let path = PathBuf::from(path);
            if !path.is_absolute() || !path.is_file() {
                return Err(opennovel_core::Error::Invalid("原有书库文件不存在".into()));
            }
            let store = opennovel_core::profiles::ProfileStore::new(
                opennovel_core::profiles::user_data_root()?,
            )?;
            let profile = store.adopt(&path)?;
            store.select(&profile.id)
        })();
        std::process::exit(if result.is_ok() { 0 } else { 1 });
    }
    if args.first().is_some_and(|a| a == "--check-installation") {
        let code = match args.as_slice() {
            [_, output] => match install_check::run(&PathBuf::from(output)) {
                Ok(true) => 0,
                Ok(false) => 1,
                Err(_) => 2,
            },
            _ => 2,
        };
        std::process::exit(code);
    }
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(software_update::UpdateState::default())
        .setup(|app| {
            if std::env::var_os("WEBVIEW2_USER_DATA_FOLDER").is_some() {
                return Err("检测到全局 WebView2 数据目录覆盖，无法保证用户隔离。请移除 WEBVIEW2_USER_DATA_FOLDER 后重启。".into());
            }
            let args = std::env::args().skip(1).collect::<Vec<_>>();
            let store = opennovel_core::profiles::ProfileStore::new(app.path().app_data_dir()?)?;
            let (paths, marker) = match args.as_slice() {
                [] => (store.resolve(None)?, None),
                [flag, id] if flag == "--profile" => (store.resolve(Some(id))?, None),
                [flag, id, handoff, token] if flag == "--profile" && handoff == "--handoff" => {
                    (store.resolve(Some(id))?, Some(store.startup_marker(token)?))
                }
                [flag, path] if flag == "--database" => {
                    let profile = store.adopt(&std::path::absolute(path)?)?;
                    (store.resolve(Some(&profile.id))?, None)
                }
                _ => return Err("参数无效；可使用 --profile <id> 或 --database <path>".into()),
            };
            let library = Arc::new(Library::open(&paths.database)?);
            std::fs::create_dir_all(&paths.directory)?;
            // Only marked inactive sessions are eligible. Locked files remain for retry.
            let _ = opennovel_core::session_directory::cleanup(&paths.sessions);
            let exports = paths.exports.clone();
            let auto_txt = auto_txt::AutoTxtService::start(Arc::clone(&library), exports.clone())?;
            app.manage(official_reader::OfficialReaderState::default());
            app.manage(AppState {
                engine: Arc::new(local_engine::EngineControl::for_directory(&paths.directory)),
                library,
                auto_txt,
                source: Arc::default(),
                cancellations: Arc::default(),
                exports,
                diagnostic_preview: Mutex::new(None),
                restored: Mutex::default(),
                batches: Arc::default(),
                proposals: Mutex::default(),
                sessions: Mutex::default(),
                session_root: paths.sessions.clone(),
                captures: Mutex::default(),
            });
            let webview = paths.webview.clone();
            let current_profile = paths.profile.id.clone();
            app.manage(profiles::ProfileState {
                store: store.clone(),
                paths,
                switching: AtomicBool::new(false),
            });
            tauri::WebviewWindowBuilder::from_config(app, &app.config().app.windows[0])?
                .data_directory(webview)
                .on_navigation(|url| accounts::local_ipc_allowed("main", url))
                .build()?;
            store.select(&current_profile)?;
            if let Some(marker) = marker {
                use std::io::Write;
                let mut ready = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(marker)?;
                ready.write_all(current_profile.as_bytes())?;
                ready.sync_all()?;
            }
            Ok(())
        })
        .invoke_handler({
            let dispatch: Box<dyn Fn(tauri::ipc::Invoke<tauri::Wry>) -> bool + Send + Sync> =
                Box::new(tauri::generate_handler![
                    software_update::software_update_status,
                    software_update::check_software_update,
                    software_update::download_software_update,
                    software_update::cancel_software_download,
                    software_update::install_software_update,
                    software_update::open_software_releases,
                    profiles::local_profiles,
                    profiles::create_local_profile,
                    profiles::switch_local_profile,
                    official_reader::open_official_reader,
                    official_reader::inspect_official_reader,
                    official_reader::save_official_reader,
                    official_reader::choose_official_export_directory,
                    official_reader::reveal_official_reader,
                    official_reader::close_official_reader,
                    export_destination::default_export_root,
                    export_destination::choose_save_directory,
                    export_destination::book_export_destination,
                    export_destination::set_book_export_directory,
                    export_destination::choose_book_export_directory,
                    auto_txt::automatic_txt_snapshot,
                    auto_txt::retry_automatic_txt,
                    account_list,
                    preview_account_page,
                    confirm_account_page,
                    preview_platform_catalog,
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
                    local_engine_status,
                    set_local_engine,
                    library_snapshot,
                    library_snapshot_if_changed,
                    book_details,
                    read_chapter,
                    export_job,
                    reveal_export,
                    pause_job,
                    job_details,
                    retry_chapters,
                    update_book,
                    import_book,
                    preview_txt_supplement,
                    confirm_txt_supplement,
                    inspect_book,
                    download_book,
                    resume_job,
                    cancel_job,
                    export_book
                ]);
            move |invoke: tauri::ipc::Invoke<tauri::Wry>| {
                let webview = invoke.message.webview_ref();
                if operation::installing() && invoke.message.command() != "software_update_status" {
                    invoke.resolver.reject("正在准备安装更新，请稍候");
                    return true;
                }
                if profiles::is_switching(webview.app_handle()) {
                    invoke.resolver.reject("正在切换用户，请在新窗口继续操作");
                    return true;
                }
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
