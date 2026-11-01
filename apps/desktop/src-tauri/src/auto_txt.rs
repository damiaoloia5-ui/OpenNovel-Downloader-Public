use opennovel_core::{ExportReceipt, Library};
use serde::Serialize;
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Serialize)]
pub struct AutoTxtStatus {
    pub book_id: String,
    pub state: String,
    pub receipt: Option<ExportReceipt>,
    pub export_id: Option<String>,
    pub error: Option<String>,
}
#[derive(Clone)]
pub struct AutoTxtService {
    statuses: Arc<Mutex<HashMap<String, AutoTxtStatus>>>,
    sender: mpsc::Sender<String>,
}
impl AutoTxtService {
    pub fn start(library: Arc<Library>, directory: PathBuf) -> std::io::Result<Self> {
        let statuses = Arc::new(Mutex::new(HashMap::new()));
        let (sender, receiver) = mpsc::channel::<String>();
        let output = Arc::clone(&statuses);
        std::thread::Builder::new()
            .name("automatic-txt".into())
            .spawn(move || {
                let mut observed = HashMap::new();
                let mut forced = HashSet::new();
                let mut next_file_check = Instant::now() + Duration::from_secs(5);
                loop {
                    let check_files = Instant::now() >= next_file_check;
                    if check_files {
                        next_file_check = Instant::now() + Duration::from_secs(5);
                    }
                    if let Ok(snapshot) = library.snapshot() {
                        let mut by_book: HashMap<&str, Vec<_>> = HashMap::new();
                        for job in &snapshot.jobs {
                            by_book.entry(&job.book_id).or_default().push(job);
                        }
                        for book in &snapshot.books {
                            let own = by_book.get(book.id.as_str()).cloned().unwrap_or_default();
                            if book.saved_chapters == 0 {
                                set(
                                    &output,
                                    AutoTxtStatus {
                                        book_id: book.id.clone(),
                                        state: "empty".into(),
                                        receipt: None,
                                        export_id: None,
                                        error: None,
                                    },
                                );
                                continue;
                            }
                            if own
                                .iter()
                                .any(|j| matches!(j.state.as_str(), "running" | "queued"))
                            {
                                continue;
                            }
                            let signature = serde_json::to_string(&(book, own)).unwrap_or_default();
                            if !forced.remove(&book.id)
                                && observed.get(&book.id) == Some(&signature)
                                && !(check_files && ready_file_missing(&output, &book.id))
                            {
                                continue;
                            }
                            observed.insert(book.id.clone(), signature);
                            process(&library, &directory, &book.id, &output);
                        }
                    }
                    match receiver.recv_timeout(Duration::from_secs(1)) {
                        Ok(id) => {
                            forced.insert(id);
                            while let Ok(id) = receiver.try_recv() {
                                forced.insert(id);
                            }
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                }
            })?;
        Ok(Self { statuses, sender })
    }
    pub fn request(&self, book: &str) -> Result<(), String> {
        self.sender
            .send(book.into())
            .map_err(|_| "自动 TXT 后台已停止，请重新打开软件".into())
    }
    pub fn snapshot(&self) -> Result<Vec<AutoTxtStatus>, String> {
        Ok(self
            .statuses
            .lock()
            .map_err(|_| "自动 TXT 状态不可用")?
            .values()
            .cloned()
            .collect())
    }
}
fn ready_file_missing(statuses: &Mutex<HashMap<String, AutoTxtStatus>>, book: &str) -> bool {
    let path = statuses.lock().ok().and_then(|values| {
        values
            .get(book)
            .filter(|s| s.state == "ready")
            .and_then(|s| s.receipt.as_ref())
            .map(|r| PathBuf::from(&r.path))
    });
    // Missing files can be rebuilt safely. Other filesystem errors require an
    // explicit retry instead of a background loop that repeatedly fails.
    path.is_some_and(|path| path.try_exists().is_ok_and(|exists| !exists))
}
fn set(statuses: &Mutex<HashMap<String, AutoTxtStatus>>, status: AutoTxtStatus) {
    if let Ok(mut values) = statuses.lock() {
        values.insert(status.book_id.clone(), status);
    }
}
fn process(
    library: &Library,
    directory: &std::path::Path,
    book: &str,
    statuses: &Mutex<HashMap<String, AutoTxtStatus>>,
) {
    let mut status = AutoTxtStatus {
        book_id: book.into(),
        state: "generating".into(),
        receipt: None,
        export_id: None,
        error: None,
    };
    set(statuses, status.clone());
    match library.export_automatic_txt(book, directory) {
        Ok(receipt) => {
            status.export_id = library.export_record_id(book, &receipt.path).ok();
            status.receipt = Some(receipt);
            status.state = "ready".into();
        }
        Err(error) => {
            status.state = "failed".into();
            status.error = Some(error.to_string());
        }
    }
    set(statuses, status);
}

#[tauri::command]
pub fn automatic_txt_snapshot(
    state: tauri::State<'_, crate::AppState>,
) -> Result<Vec<AutoTxtStatus>, String> {
    state.auto_txt.snapshot()
}
#[tauri::command]
pub fn retry_automatic_txt(
    book_id: String,
    state: tauri::State<'_, crate::AppState>,
) -> Result<(), String> {
    let book = state
        .library
        .book_details(&book_id)
        .map_err(|e| e.to_string())?
        .book;
    if book.saved_chapters == 0 {
        return Err("尚无正文，无法生成 TXT".into());
    }
    state.auto_txt.request(&book_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::atomic::AtomicBool, time::Instant};
    fn wait(service: &AutoTxtService, book: &str, state: &str) -> AutoTxtStatus {
        let until = Instant::now() + Duration::from_secs(12);
        loop {
            if let Some(s) = service.snapshot().unwrap().into_iter().find(|s| {
                s.book_id == book
                    && s.state == state
                    && (state != "ready"
                        || s.receipt
                            .as_ref()
                            .is_some_and(|r| std::path::Path::new(&r.path).is_file()))
            }) {
                return s;
            }
            assert!(
                Instant::now() < until,
                "automatic TXT state timeout: {state}"
            );
            std::thread::sleep(Duration::from_millis(30));
        }
    }
    #[test]
    fn deleted_ready_file_is_rebuilt_without_another_request_or_download() {
        let d = tempfile::tempdir().unwrap();
        let l = Arc::new(Library::open(d.path().join("library.db")).unwrap());
        let job = l
            .create_import(include_str!("../../../../examples/local-book.json"))
            .unwrap();
        let book = l.run_import(&job, &AtomicBool::new(false)).unwrap().book_id;
        let service = AutoTxtService::start(Arc::clone(&l), d.path().join("exports")).unwrap();
        let receipt = wait(&service, &book, "ready").receipt.unwrap();
        let original = std::fs::read(&receipt.path).unwrap();
        std::fs::remove_file(&receipt.path).unwrap();
        let repaired = wait(&service, &book, "ready").receipt.unwrap();
        assert_eq!(repaired.path, receipt.path);
        assert_eq!(std::fs::read(&repaired.path).unwrap(), original);
        assert_eq!(l.book_details(&book).unwrap().exports.len(), 1);
        assert_eq!(l.job(&job).unwrap().state, "completed");
        drop(service);
        std::thread::sleep(Duration::from_millis(100));
    }

    #[test]
    fn explicit_regeneration_preserves_user_edits_and_does_not_loop_on_failure() {
        let d = tempfile::tempdir().unwrap();
        let l = Arc::new(Library::open(d.path().join("library.db")).unwrap());
        let job = l
            .create_import(include_str!("../../../../examples/local-book.json"))
            .unwrap();
        let book = l.run_import(&job, &AtomicBool::new(false)).unwrap().book_id;
        let service = AutoTxtService::start(Arc::clone(&l), d.path().join("exports")).unwrap();
        let receipt = wait(&service, &book, "ready").receipt.unwrap();
        std::fs::write(&receipt.path, "用户原创批注，请保留").unwrap();
        service.request(&book).unwrap();
        assert!(
            wait(&service, &book, "failed")
                .error
                .unwrap()
                .contains("已被修改")
        );
        std::thread::sleep(Duration::from_millis(1200));
        assert_eq!(
            service
                .snapshot()
                .unwrap()
                .iter()
                .find(|s| s.book_id == book)
                .unwrap()
                .state,
            "failed"
        );
        assert_eq!(
            std::fs::read_to_string(&receipt.path).unwrap(),
            "用户原创批注，请保留"
        );
        assert_eq!(l.snapshot().unwrap().books[0].saved_chapters, 2);
        assert_eq!(l.book_details(&book).unwrap().exports.len(), 1);
        drop(service);
        std::thread::sleep(Duration::from_millis(100));
    }
    #[test]
    fn background_detects_completion_without_any_frontend_and_recovers_on_restart() {
        let d = tempfile::tempdir().unwrap();
        let l = Arc::new(Library::open(d.path().join("library.db")).unwrap());
        let service = AutoTxtService::start(Arc::clone(&l), d.path().join("exports")).unwrap();
        let job = l
            .create_import(include_str!("../../../../examples/local-book.json"))
            .unwrap();
        let book = l.job(&job).unwrap().book_id;
        wait(&service, &book, "empty");
        assert!(!d.path().join("exports").exists());
        l.run_import(&job, &AtomicBool::new(false)).unwrap();
        let ready = wait(&service, &book, "ready");
        let receipt = ready.receipt.unwrap();
        assert!(!receipt.partial);
        assert_eq!(receipt.saved_chapters, 2);
        assert!(ready.export_id.is_some());
        service.request(&book).unwrap();
        std::thread::sleep(Duration::from_millis(200));
        assert_eq!(l.book_details(&book).unwrap().exports.len(), 1);
        drop(service);
        std::thread::sleep(Duration::from_millis(100));
        let restarted = AutoTxtService::start(Arc::clone(&l), d.path().join("exports")).unwrap();
        assert_eq!(
            wait(&restarted, &book, "ready").receipt.unwrap().path,
            receipt.path
        );
        drop(restarted);
        std::thread::sleep(Duration::from_millis(100));
    }
    #[test]
    fn background_failure_is_separate_from_successful_import_and_explicit_retry_recovers() {
        let d = tempfile::tempdir().unwrap();
        let l = Arc::new(Library::open(d.path().join("library.db")).unwrap());
        let out = d.path().join("exports");
        std::fs::write(&out, "original obstacle").unwrap();
        let job = l
            .create_import(include_str!("../../../../examples/local-book.json"))
            .unwrap();
        let book = l.run_import(&job, &AtomicBool::new(false)).unwrap().book_id;
        let service = AutoTxtService::start(Arc::clone(&l), out.clone()).unwrap();
        let failure = wait(&service, &book, "failed");
        assert!(failure.error.is_some());
        assert_eq!(l.job(&job).unwrap().state, "completed");
        assert_eq!(l.snapshot().unwrap().books[0].saved_chapters, 2);
        std::fs::remove_file(&out).unwrap();
        service.request(&book).unwrap();
        assert!(wait(&service, &book, "ready").receipt.is_some());
        drop(service);
        std::thread::sleep(Duration::from_millis(100));
    }
}
