use opennovel_core::{
    Error, ImportChapter, Library,
    sources::{Catalog, ChapterRef, SourceAdapter},
};
use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
};
use tempfile::tempdir;

fn catalog() -> Catalog {
    Catalog {
        source: "fanqie-web".into(),
        remote_id: "10000000001".into(),
        title: "原创网络测试".into(),
        author: "测试".into(),
        chapters: (0..5)
            .map(|i| ChapterRef {
                id: format!("1000000001{i}"),
                title: format!("第{i}章"),
                restricted: false,
            })
            .collect(),
    }
}
struct Fake {
    calls: Mutex<Vec<String>>,
    fail: Option<String>,
    cancel_at: Option<String>,
}
impl Fake {
    fn new() -> Self {
        Self {
            calls: Mutex::new(vec![]),
            fail: None,
            cancel_at: None,
        }
    }
}
impl SourceAdapter for Fake {
    fn source_id(&self) -> &str {
        "fanqie-web"
    }
    fn fetch_chapter(
        &self,
        _: &str,
        ch: &ChapterRef,
        cancel: &AtomicBool,
    ) -> opennovel_core::Result<ImportChapter> {
        self.calls.lock().unwrap().push(ch.id.clone());
        if self.fail.as_ref() == Some(&ch.id) {
            return Err(Error::Network("受控临时失败".into()));
        }
        if self.cancel_at.as_ref() == Some(&ch.id) {
            cancel.store(true, Ordering::Release);
        }
        Ok(ImportChapter {
            id: ch.id.clone(),
            title: ch.title.clone(),
            paragraphs: vec!["这是原创测试正文。".into()],
        })
    }
}

#[test]
fn selected_range_is_distinct_from_book_completeness_and_reuses_saved_chapters() {
    let dir = tempdir().unwrap();
    let library = Library::open(dir.path().join("library.db")).unwrap();
    let catalog = catalog();
    let fake = Fake::new();
    let cancel = AtomicBool::new(false);
    let id = library.create_download(&catalog, 2, 4).unwrap();
    let job = library.run_job(&id, &fake, &cancel).unwrap();
    assert_eq!(
        (job.state.as_str(), job.saved, job.total),
        ("completed", 3, 3)
    );
    let book = &library.snapshot().unwrap().books[0];
    assert_eq!((book.saved_chapters, book.total_chapters), (3, 5));
    let id = library.create_download(&catalog, 1, 5).unwrap();
    let job = library.run_job(&id, &fake, &cancel).unwrap();
    assert_eq!((job.saved, job.total), (5, 5));
    assert_eq!(fake.calls.lock().unwrap().len(), 5);
    let mut changed = catalog.clone();
    changed.chapters.reverse();
    assert!(library.create_download(&changed, 1, 5).is_err());
    assert_eq!(library.snapshot().unwrap().books[0].saved_chapters, 5);
}

#[test]
fn restricted_chapters_are_not_fetched_and_never_count_as_complete() {
    let dir = tempdir().unwrap();
    let library = Library::open(dir.path().join("library.db")).unwrap();
    let mut catalog = catalog();
    catalog.chapters[1].restricted = true;
    let fake = Fake::new();
    let id = library.create_download(&catalog, 1, 3).unwrap();
    let job = library
        .run_job(&id, &fake, &AtomicBool::new(false))
        .unwrap();
    assert_eq!(
        (job.state.as_str(), job.saved, job.blocked, job.failed),
        ("partial", 2, 1, 0)
    );
    assert_eq!(fake.calls.lock().unwrap().len(), 2);
    let id = library.create_download(&catalog, 2, 2).unwrap();
    let job = library
        .run_job(&id, &fake, &AtomicBool::new(false))
        .unwrap();
    assert_eq!(job.state, "blocked");
    assert_eq!(library.snapshot().unwrap().books[0].saved_chapters, 2);
}

#[test]
fn temporary_failure_stops_requests_and_resume_fetches_only_missing_content() {
    let dir = tempdir().unwrap();
    let library = Library::open(dir.path().join("library.db")).unwrap();
    let catalog = catalog();
    let mut fake = Fake::new();
    fake.fail = Some(catalog.chapters[1].id.clone());
    let id = library.create_download(&catalog, 1, 5).unwrap();
    let job = library
        .run_job(&id, &fake, &AtomicBool::new(false))
        .unwrap();
    assert_eq!(
        (job.state.as_str(), job.saved, job.failed),
        ("partial", 1, 1)
    );
    assert_eq!(fake.calls.lock().unwrap().len(), 2);
    let recovery = Fake::new();
    let job = library
        .run_job(&id, &recovery, &AtomicBool::new(false))
        .unwrap();
    assert_eq!((job.saved, job.failed), (5, 0));
    assert_eq!(recovery.calls.lock().unwrap().len(), 4);
}

#[test]
fn cancellation_before_commit_keeps_previous_chapters_and_resumes_after_reopen() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("library.db");
    let library = Library::open(&path).unwrap();
    let catalog = catalog();
    let mut fake = Fake::new();
    fake.cancel_at = Some(catalog.chapters[1].id.clone());
    let id = library.create_download(&catalog, 1, 5).unwrap();
    let job = library
        .run_job(&id, &fake, &AtomicBool::new(false))
        .unwrap();
    assert_eq!((job.state.as_str(), job.saved), ("cancelled", 1));
    drop(library);
    let library = Library::open(&path).unwrap();
    let recovery = Fake::new();
    let job = library
        .run_job(&id, &recovery, &AtomicBool::new(false))
        .unwrap();
    assert_eq!(job.saved, 5);
    assert_eq!(recovery.calls.lock().unwrap().len(), 4);
}

#[test]
fn migration_backs_up_v1_book_content_before_upgrading() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("library.db");
    let library = Library::open(&path).unwrap();
    let id = library
        .create_import(include_str!("../../../examples/local-book.json"))
        .unwrap();
    library.run_import(&id, &AtomicBool::new(false)).unwrap();
    drop(library);
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch(
        "DROP TABLE source_catalogs; DROP INDEX job_items_pending; PRAGMA user_version=1;",
    )
    .unwrap();
    drop(db);
    let library = Library::open(&path).unwrap();
    assert_eq!(library.snapshot().unwrap().books[0].saved_chapters, 2);
    let backups: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(Result::unwrap)
        .filter(|p| {
            p.file_name()
                .to_string_lossy()
                .starts_with("library-v1-backup-")
        })
        .collect();
    assert_eq!(backups.len(), 1);
    let db = rusqlite::Connection::open(backups[0].path()).unwrap();
    let version: u32 = db
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .unwrap();
    assert_eq!(version, 1);
    let chapters: u32 = db
        .query_row("SELECT COUNT(*) FROM chapters", [], |r| r.get(0))
        .unwrap();
    assert_eq!(chapters, 2);
}

#[test]
fn remote_chapter_and_completion_roll_back_together_on_database_failure() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("library.db");
    let library = Library::open(&path).unwrap();
    let id = library.create_download(&catalog(), 1, 2).unwrap();
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("CREATE TRIGGER fail_completion BEFORE UPDATE OF state ON job_items WHEN NEW.state='completed' BEGIN SELECT RAISE(ABORT, 'test-only failure'); END;").unwrap();
    assert!(
        library
            .run_job(&id, &Fake::new(), &AtomicBool::new(false))
            .is_err()
    );
    assert_eq!(library.snapshot().unwrap().books[0].saved_chapters, 0);
    assert_eq!(library.job(&id).unwrap().saved, 0);
    db.execute_batch("DROP TRIGGER fail_completion;").unwrap();
    assert_eq!(
        library
            .run_job(&id, &Fake::new(), &AtomicBool::new(false))
            .unwrap()
            .saved,
        2
    );
}

#[test]
fn interrupted_network_worker_reopens_as_paused_and_resumes_missing_chapters() {
    struct Interrupted;
    impl SourceAdapter for Interrupted {
        fn source_id(&self) -> &str {
            "fanqie-web"
        }
        fn fetch_chapter(
            &self,
            _: &str,
            ch: &ChapterRef,
            _: &AtomicBool,
        ) -> opennovel_core::Result<ImportChapter> {
            if ch.id == "10000000011" {
                panic!("test interruption outside transaction")
            }
            Ok(ImportChapter {
                id: ch.id.clone(),
                title: ch.title.clone(),
                paragraphs: vec!["原创内容".into()],
            })
        }
    }
    let dir = tempdir().unwrap();
    let path = dir.path().join("library.db");
    let library = Library::open(&path).unwrap();
    let id = library.create_download(&catalog(), 1, 5).unwrap();
    let stopped = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        library.run_job(&id, &Interrupted, &AtomicBool::new(false))
    }));
    assert!(stopped.is_err());
    drop(library);
    let library = Library::open(&path).unwrap();
    let before = library.job(&id).unwrap();
    assert_eq!((before.state.as_str(), before.saved), ("paused", 1));
    let fake = Fake::new();
    assert_eq!(
        library
            .run_job(&id, &fake, &AtomicBool::new(false))
            .unwrap()
            .saved,
        5
    );
    assert_eq!(fake.calls.lock().unwrap().len(), 4);
}
