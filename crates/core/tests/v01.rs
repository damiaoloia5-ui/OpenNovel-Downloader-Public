use opennovel_core::{
    Error, ExportFormat, ImportChapter, Library, parse_selection,
    sources::{Catalog, ChapterRef, SourceAdapter},
};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use tempfile::tempdir;

fn catalog() -> Catalog {
    Catalog {
        source: "fanqie-web".into(),
        remote_id: "80000000000".into(),
        title: "原创首版验收".into(),
        author: "OpenNovel".into(),
        description: "本项目原创的受控验收内容。".into(),
        chapters: (1..=5)
            .map(|i| ChapterRef {
                id: format!("8000000000{i}"),
                title: format!("原创第{i}章"),
                restricted: false,
            })
            .collect(),
    }
}
struct Source;
impl SourceAdapter for Source {
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
            paragraphs: vec![format!("原创正文_{}", ch.id)],
        })
    }
}

#[test]
fn discrete_selection_validates_and_task_export_never_leaks_other_cached_chapters() {
    assert_eq!(parse_selection(" 1,3,5 ", 5).unwrap(), [1, 3, 5]);
    assert_eq!(parse_selection("2-4，5", 5).unwrap(), [2, 3, 4, 5]);
    for s in ["", "0", "6", "4-2", "1,1", "1-3,3", "1-2-3", "a", "1,"] {
        assert!(parse_selection(s, 5).is_err(), "{s}");
    }
    let d = tempdir().unwrap();
    let l = Library::open(d.path().join("library.db")).unwrap();
    let c = catalog();
    let full = l.create_download(&c, 1, 5).unwrap();
    let job = l.run_job(&full, &Source, &AtomicBool::new(false)).unwrap();
    let selected = l.create_download_selected(&c, &[4, 2]).unwrap();
    let result = l
        .run_job(&selected, &Source, &AtomicBool::new(false))
        .unwrap();
    assert_eq!((result.saved, result.total), (2, 2));
    let receipt = l
        .export_job(&selected, d.path(), ExportFormat::Txt)
        .unwrap();
    let text = std::fs::read_to_string(&receipt.path).unwrap();
    assert!(text.contains("80000000002") && text.contains("80000000004"));
    for id in ["80000000001", "80000000003", "80000000005"] {
        assert!(!text.contains(id));
    }
    assert!(text.contains("任务选章"));
    assert_eq!(
        (
            receipt.saved_chapters,
            receipt.total_chapters,
            receipt.partial
        ),
        (2, 2, false)
    );
    let details = l.book_details(&job.book_id).unwrap();
    assert_eq!(details.description, c.description);
    assert_eq!(details.exports.len(), 1);
    assert_eq!(
        details.exports[0].job_id.as_deref(),
        Some(selected.as_str())
    );
    assert_eq!(
        l.export_location(&details.exports[0].id).unwrap(),
        receipt.path
    );
    assert!(
        l.export_job(&selected, d.path(), ExportFormat::Txt)
            .is_err()
    );
    assert_eq!(l.book_details(&job.book_id).unwrap().exports.len(), 1);
    assert!(l.read_chapter(&job.book_id, "missing").is_err());
    let read = l.read_chapter(&job.book_id, "80000000002").unwrap();
    assert_eq!(read.paragraphs, ["原创正文_80000000002"]);
}

#[test]
fn pause_during_fetch_discards_late_text_preserves_previous_commits_and_resumes() {
    struct Pausing<'a> {
        library: &'a Library,
        job: &'a str,
        calls: AtomicUsize,
    }
    impl SourceAdapter for Pausing<'_> {
        fn source_id(&self) -> &str {
            "fanqie-web"
        }
        fn fetch_chapter(
            &self,
            b: &str,
            ch: &ChapterRef,
            token: &AtomicBool,
        ) -> opennovel_core::Result<ImportChapter> {
            if self.calls.fetch_add(1, Ordering::SeqCst) == 1 {
                self.library.request_stop(self.job, true, Some(token))?;
            }
            Source.fetch_chapter(b, ch, token)
        }
    }
    let d = tempdir().unwrap();
    let l = Library::open(d.path().join("library.db")).unwrap();
    let job = l.create_download(&catalog(), 1, 5).unwrap();
    let source = Pausing {
        library: &l,
        job: &job,
        calls: AtomicUsize::new(0),
    };
    let result = l.run_job(&job, &source, &AtomicBool::new(false)).unwrap();
    assert_eq!((result.state.as_str(), result.saved), ("paused", 1));
    assert!(l.read_chapter(&result.book_id, "80000000002").is_err());
    let result = l.run_job(&job, &Source, &AtomicBool::new(false)).unwrap();
    assert_eq!((result.state.as_str(), result.saved), ("completed", 5));
}

#[test]
fn queued_pause_cannot_be_overwritten_by_a_late_worker_start() {
    let d = tempdir().unwrap();
    let l = Library::open(d.path().join("library.db")).unwrap();
    let job = l.create_download(&catalog(), 1, 5).unwrap();
    let stop = AtomicBool::new(false);
    l.request_stop(&job, true, Some(&stop)).unwrap();
    assert_eq!(l.run_job(&job, &Source, &stop).unwrap().state, "paused");
    l.cancel_job(&job).unwrap();
    assert_eq!(l.run_job(&job, &Source, &stop).unwrap().state, "cancelled");
    let input = include_str!("../../../examples/local-book.json");
    let local = l.create_import(input).unwrap();
    let stop = AtomicBool::new(false);
    l.request_stop(&local, true, Some(&stop)).unwrap();
    assert_eq!(l.run_import(&local, &stop).unwrap().state, "paused");
    assert_eq!(
        l.run_import(&local, &AtomicBool::new(false)).unwrap().saved,
        2
    );
}

#[test]
fn local_pause_and_failed_export_leave_offline_content_usable() {
    let d = tempdir().unwrap();
    let path = d.path().join("library.db");
    let l = Library::open(&path).unwrap();
    let id = l
        .create_import(include_str!("../../../examples/local-book.json"))
        .unwrap();
    let result = l
        .run_import_observed(&id, &AtomicBool::new(false), |_| l.pause_job(&id).unwrap())
        .unwrap();
    assert_eq!((result.state.as_str(), result.saved), ("paused", 1));
    drop(l);
    let l = Library::open(&path).unwrap();
    let result = l.run_import(&id, &AtomicBool::new(false)).unwrap();
    let details = l.book_details(&result.book_id).unwrap();
    assert_eq!(details.chapters.len(), 2);
    assert!(details.chapters.iter().all(|c| c.saved));
    let blocked = d.path().join("not-a-directory");
    std::fs::write(&blocked, "原创占位").unwrap();
    assert!(
        l.export(&result.book_id, &blocked, ExportFormat::Epub)
            .is_err()
    );
    assert!(l.book_details(&result.book_id).unwrap().exports.is_empty());
    assert_eq!(
        l.read_chapter(&result.book_id, &details.chapters[0].id)
            .unwrap()
            .paragraphs
            .len(),
        2
    );
}

#[test]
fn migration_failure_keeps_original_and_backup_and_does_not_advance_version() {
    let d = tempdir().unwrap();
    let path = d.path().join("library.db");
    let l = Library::open(&path).unwrap();
    let id = l
        .create_import(include_str!("../../../examples/local-book.json"))
        .unwrap();
    l.run_import(&id, &AtomicBool::new(false)).unwrap();
    drop(l);
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("PRAGMA user_version=3;").unwrap();
    drop(db);
    assert!(matches!(Library::open(&path), Err(Error::Database(_))));
    let db = rusqlite::Connection::open(&path).unwrap();
    assert_eq!(
        db.pragma_query_value(None, "user_version", |r| r.get::<_, u32>(0))
            .unwrap(),
        3
    );
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM chapters", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        2
    );
    let backup = std::fs::read_dir(d.path())
        .unwrap()
        .map(Result::unwrap)
        .find(|p| {
            p.file_name()
                .to_string_lossy()
                .starts_with("library-v3-backup-")
        })
        .unwrap();
    let saved = rusqlite::Connection::open(backup.path()).unwrap();
    assert_eq!(
        saved
            .query_row("SELECT COUNT(*) FROM chapters", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        2
    );
}
