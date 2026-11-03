use opennovel_core::{
    Error, ImportChapter, Library,
    sources::{Catalog, ChapterRef, SourceAdapter},
};
use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
};
use tempfile::tempdir;

#[test]
fn site_access_challenge_stops_the_queue_and_explicit_resume_keeps_saved_chapters() {
    struct GateReader {
        blocked: AtomicBool,
        calls: Mutex<Vec<String>>,
    }
    impl SourceAdapter for GateReader {
        fn source_id(&self) -> &str {
            "fanqie-web"
        }
        fn fetch_chapter(
            &self,
            _: &str,
            ch: &ChapterRef,
            _: &AtomicBool,
        ) -> opennovel_core::Result<ImportChapter> {
            self.calls.lock().unwrap().push(ch.id.clone());
            if ch.id.ends_with('1') && self.blocked.load(Ordering::Acquire) {
                return Err(Error::AccessChallenge("原创网站验证页".into()));
            }
            Ok(ImportChapter {
                id: ch.id.clone(),
                title: ch.title.clone(),
                paragraphs: vec!["原创稳定保存正文。".into()],
            })
        }
    }
    let dir = tempdir().unwrap();
    let path = dir.path().join("library.db");
    let library = Library::open(&path).unwrap();
    let catalog = catalog();
    let reader = GateReader {
        blocked: AtomicBool::new(true),
        calls: Mutex::new(Vec::new()),
    };
    let id = library.create_download(&catalog, 1, 5).unwrap();
    let stopped = library
        .run_job(&id, &reader, &AtomicBool::new(false))
        .unwrap();
    assert_eq!(
        (
            stopped.state.as_str(),
            stopped.saved,
            stopped.failed,
            stopped.blocked
        ),
        ("partial", 1, 1, 0)
    );
    assert_eq!(reader.calls.lock().unwrap().len(), 2);
    let detail = library.job_details(&id).unwrap();
    assert_eq!(
        detail.items[1].error_kind.as_deref(),
        Some("access_challenge")
    );
    assert_eq!(detail.items[2].state, "queued");
    let receipt = library
        .export_automatic_txt(&stopped.book_id, dir.path().join("partial"))
        .unwrap();
    assert!(receipt.partial);
    assert_eq!(receipt.saved_chapters, 1);
    drop(library);
    let library = Library::open(&path).unwrap();
    reader.blocked.store(false, Ordering::Release);
    let resumed = library
        .run_job(&id, &reader, &AtomicBool::new(false))
        .unwrap();
    assert_eq!((resumed.state.as_str(), resumed.saved), ("completed", 5));
    let calls = reader.calls.lock().unwrap();
    assert_eq!(calls.len(), 6);
    assert_eq!(
        calls
            .iter()
            .filter(|id| **id == catalog.chapters[0].id)
            .count(),
        1
    );
    let receipt = library
        .export_automatic_txt(&resumed.book_id, dir.path().join("complete"))
        .unwrap();
    assert!(!receipt.partial);
    assert_eq!(receipt.saved_chapters, 5);
}

fn catalog() -> Catalog {
    Catalog {
        source: "fanqie-web".into(),
        remote_id: "10000000001".into(),
        title: "原创网络测试".into(),
        author: "测试".into(),
        description: String::new(),
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
        "DROP TABLE automatic_exports; DROP TABLE export_directories; DROP TABLE accounts; DROP TABLE job_history; DROP TABLE history_chapters; DROP TABLE catalog_history; DROP TABLE content_blobs; DROP TABLE batch_items; DROP TABLE batches; DROP TABLE book_preferences; DROP TABLE exports; DROP TABLE source_catalogs; DROP INDEX job_items_pending; ALTER TABLE job_items DROP COLUMN error; ALTER TABLE job_items DROP COLUMN error_kind; PRAGMA user_version=1;",
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

#[test]
fn details_persist_errors_and_targeted_retry_leaves_other_pending_items_untouched() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("library.db");
    let library = Library::open(&path).unwrap();
    let catalog = catalog();
    let mut fake = Fake::new();
    fake.fail = Some(catalog.chapters[1].id.clone());
    let id = library.create_download(&catalog, 1, 5).unwrap();
    library
        .run_job(&id, &fake, &AtomicBool::new(false))
        .unwrap();
    drop(library);
    let library = Library::open(&path).unwrap();
    let details = library.job_details(&id).unwrap();
    assert_eq!(details.items[1].position, 2);
    assert_eq!(details.items[1].error_kind.as_deref(), Some("network"));
    assert!(
        details.items[1]
            .error
            .as_ref()
            .unwrap()
            .contains("受控临时失败")
    );
    assert!(details.items[1].retryable);
    let recovery = Fake::new();
    let result = library
        .retry_chapters(
            &id,
            &recovery,
            &AtomicBool::new(false),
            &[catalog.chapters[1].id.clone()],
        )
        .unwrap();
    assert_eq!(
        (result.state.as_str(), result.saved, result.total),
        ("partial", 2, 5)
    );
    assert_eq!(
        *recovery.calls.lock().unwrap(),
        [catalog.chapters[1].id.clone()]
    );
    let details = library.job_details(&id).unwrap();
    assert!(details.items[1].error.is_none());
    assert_eq!(details.items[2].state, "queued");
    assert!(
        library
            .retry_chapters(
                &id,
                &recovery,
                &AtomicBool::new(false),
                &[catalog.chapters[0].id.clone()]
            )
            .is_err()
    );
    assert!(
        library
            .retry_chapters(&id, &recovery, &AtomicBool::new(false), &[])
            .is_err()
    );
    assert_eq!(
        library
            .run_job(&id, &recovery, &AtomicBool::new(false))
            .unwrap()
            .saved,
        5
    );
}

#[test]
fn restricted_detail_cannot_be_retried_without_catalog_refresh() {
    let dir = tempdir().unwrap();
    let library = Library::open(dir.path().join("library.db")).unwrap();
    let mut catalog = catalog();
    catalog.chapters[0].restricted = true;
    let fake = Fake::new();
    let id = library.create_download(&catalog, 1, 1).unwrap();
    library
        .run_job(&id, &fake, &AtomicBool::new(false))
        .unwrap();
    let detail = library.job_details(&id).unwrap();
    assert_eq!(detail.items[0].error_kind.as_deref(), Some("restricted"));
    assert!(!detail.items[0].retryable);
    assert!(
        library
            .retry_chapters(
                &id,
                &fake,
                &AtomicBool::new(false),
                &[catalog.chapters[0].id.clone()]
            )
            .is_err()
    );
    assert!(fake.calls.lock().unwrap().is_empty());
    catalog.chapters[0].restricted = false;
    let updated = library.create_download(&catalog, 1, 1).unwrap();
    assert_eq!(
        library
            .run_job(&updated, &fake, &AtomicBool::new(false))
            .unwrap()
            .saved,
        1
    );
}

#[test]
fn append_only_update_reuses_content_and_exports_new_snapshot_without_overwrite() {
    use opennovel_core::ExportFormat;
    let dir = tempdir().unwrap();
    let library = Library::open(dir.path().join("library.db")).unwrap();
    let mut catalog = catalog();
    catalog.chapters.truncate(2);
    let fake = Fake::new();
    let id = library.create_download(&catalog, 1, 2).unwrap();
    let job = library
        .run_job(&id, &fake, &AtomicBool::new(false))
        .unwrap();
    let before = library
        .export(&job.book_id, dir.path(), ExportFormat::Txt)
        .unwrap();
    let original = std::fs::read(&before.path).unwrap();
    let updated = catalog_with_new_chapter(&catalog);
    let id = library.create_download(&updated, 1, 3).unwrap();
    let result = library
        .run_job(&id, &fake, &AtomicBool::new(false))
        .unwrap();
    assert_eq!(result.book_id, job.book_id);
    assert_eq!((result.saved, result.total), (3, 3));
    assert_eq!(fake.calls.lock().unwrap().len(), 3);
    let book = &library.snapshot().unwrap().books[0];
    assert_eq!((book.saved_chapters, book.total_chapters), (3, 3));
    let after = library
        .export(&job.book_id, dir.path(), ExportFormat::Txt)
        .unwrap();
    assert_ne!(before.path, after.path);
    assert_eq!(std::fs::read(&before.path).unwrap(), original);
    let db = rusqlite::Connection::open(dir.path().join("library.db")).unwrap();
    let ids: Vec<String> = db
        .prepare("SELECT chapter_id FROM chapters ORDER BY position")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(
        ids,
        updated
            .chapters
            .iter()
            .map(|c| c.id.clone())
            .collect::<Vec<_>>()
    );
}

fn catalog_with_new_chapter(previous: &Catalog) -> Catalog {
    let mut updated = previous.clone();
    updated.chapters.push(ChapterRef {
        id: "10000000999".into(),
        title: "追加的原创章节".into(),
        restricted: false,
    });
    updated
}

#[test]
fn unsafe_catalog_changes_are_rejected_without_mutating_metadata_or_creating_jobs() {
    let dir = tempdir().unwrap();
    let library = Library::open(dir.path().join("library.db")).unwrap();
    let original = catalog();
    let id = library.create_download(&original, 1, 5).unwrap();
    library
        .run_job(&id, &Fake::new(), &AtomicBool::new(false))
        .unwrap();
    for variant in 0..3 {
        let mut changed = original.clone();
        changed.title = "不能保存的新标题".into();
        match variant {
            0 => {
                changed.chapters.remove(1);
            }
            1 => {
                changed.chapters.swap(0, 1);
            }
            _ => {
                changed.chapters.insert(
                    1,
                    ChapterRef {
                        id: "10000000999".into(),
                        title: "插章".into(),
                        restricted: false,
                    },
                );
            }
        }
        assert!(
            library
                .create_download(&changed, 1, changed.chapters.len())
                .is_err()
        );
    }
    let snapshot = library.snapshot().unwrap();
    assert_eq!(snapshot.books[0].title, original.title);
    assert_eq!(snapshot.jobs.len(), 1);
    assert_eq!(snapshot.books[0].saved_chapters, 5);
    assert!(
        library
            .create_download(&catalog_with_new_chapter(&original), 0, 6)
            .is_err()
    );
    assert_eq!(library.snapshot().unwrap().books[0].total_chapters, 5);
}

#[test]
fn migration_from_v2_preserves_jobs_and_creates_a_readable_original_backup() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("library.db");
    let library = Library::open(&path).unwrap();
    let catalog = catalog();
    let mut fake = Fake::new();
    fake.fail = Some(catalog.chapters[1].id.clone());
    let id = library.create_download(&catalog, 1, 5).unwrap();
    library
        .run_job(&id, &fake, &AtomicBool::new(false))
        .unwrap();
    drop(library);
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("DROP TABLE automatic_exports; DROP TABLE export_directories; DROP TABLE accounts; DROP TABLE job_history; DROP TABLE history_chapters; DROP TABLE catalog_history; DROP TABLE content_blobs; DROP TABLE batch_items; DROP TABLE batches; DROP TABLE book_preferences; DROP TABLE exports; ALTER TABLE job_items DROP COLUMN error; ALTER TABLE job_items DROP COLUMN error_kind; PRAGMA user_version=2;").unwrap();
    drop(db);
    let library = Library::open(&path).unwrap();
    let detail = library.job_details(&id).unwrap();
    assert_eq!(detail.job.saved, 1);
    assert!(detail.items[1].error.is_none());
    assert!(detail.items[1].retryable);
    let backup = std::fs::read_dir(dir.path())
        .unwrap()
        .map(Result::unwrap)
        .find(|p| {
            p.file_name()
                .to_string_lossy()
                .starts_with("library-v2-backup-")
        })
        .unwrap();
    let db = rusqlite::Connection::open(backup.path()).unwrap();
    let version: u32 = db
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .unwrap();
    assert_eq!(version, 2);
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM chapters", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        library
            .run_job(&id, &Fake::new(), &AtomicBool::new(false))
            .unwrap()
            .saved,
        5
    );
}

#[test]
fn invalid_returned_chapter_is_a_visible_item_failure_without_saved_text() {
    struct Invalid;
    impl SourceAdapter for Invalid {
        fn source_id(&self) -> &str {
            "fanqie-web"
        }
        fn fetch_chapter(
            &self,
            _: &str,
            ch: &ChapterRef,
            _: &AtomicBool,
        ) -> opennovel_core::Result<ImportChapter> {
            Ok(ImportChapter {
                id: ch.id.clone(),
                title: ch.title.clone(),
                paragraphs: vec![],
            })
        }
    }
    let dir = tempdir().unwrap();
    let library = Library::open(dir.path().join("library.db")).unwrap();
    let id = library.create_download(&catalog(), 1, 2).unwrap();
    let result = library
        .run_job(&id, &Invalid, &AtomicBool::new(false))
        .unwrap();
    assert_eq!((result.saved, result.failed), (0, 1));
    let detail = library.job_details(&id).unwrap();
    assert_eq!(
        detail.items[0].error_kind.as_deref(),
        Some("invalid_content")
    );
    assert!(detail.items[0].error.is_some());
    assert_eq!(detail.items[1].state, "queued");
}
