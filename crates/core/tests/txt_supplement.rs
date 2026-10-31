use opennovel_core::{
    Library,
    sources::{Catalog, ChapterRef},
};
use std::sync::atomic::AtomicBool;

fn setup() -> (tempfile::TempDir, Library, String) {
    let dir = tempfile::tempdir().unwrap();
    let library = Library::open(dir.path().join("library.db")).unwrap();
    let catalog = Catalog {
        source: "fanqie-web".into(),
        remote_id: "original".into(),
        title: "原创测试".into(),
        author: "原创作者".into(),
        description: String::new(),
        chapters: (1..=3)
            .map(|i| ChapterRef {
                id: format!("c{i}"),
                title: format!("第{i}章 原创{i}"),
                restricted: true,
            })
            .collect(),
    };
    let job = library.create_download(&catalog, 1, 3).unwrap();
    library.cancel_job(&job).unwrap();
    let book = library.snapshot().unwrap().books[0].id.clone();
    (dir, library, book)
}
fn text() -> String {
    (1..=3)
        .map(|i| format!("第{i}章 原创{i}\n原创正文{i}。\n\n"))
        .collect()
}

#[test]
fn preview_does_not_save_confirmation_is_atomic_and_idempotent() {
    let (dir, library, book) = setup();
    let preview = library.preview_txt_supplement(&book, &text()).unwrap();
    assert_eq!((preview.total, preview.already_saved), (3, 0));
    assert_eq!(library.book_details(&book).unwrap().book.saved_chapters, 0);
    let before = library.snapshot().unwrap().jobs.len();
    library
        .confirm_txt_supplement(&book, &text(), &preview.fingerprint)
        .unwrap();
    assert_eq!(library.book_details(&book).unwrap().book.saved_chapters, 3);
    let snapshot = library.snapshot().unwrap();
    assert_eq!(snapshot.jobs.len(), before + 1);
    assert_eq!(
        (
            snapshot.jobs[0].saved,
            snapshot.jobs[0].total,
            snapshot.jobs[0].state.as_str()
        ),
        (3, 3, "completed")
    );
    library
        .confirm_txt_supplement(&book, &text(), &preview.fingerprint)
        .unwrap();
    assert_eq!(library.snapshot().unwrap().jobs.len(), before + 1);
    let altered = text().replace("原创正文1。", "另一份原创正文。");
    let next = library.preview_txt_supplement(&book, &altered).unwrap();
    library
        .confirm_txt_supplement(&book, &altered, &next.fingerprint)
        .unwrap();
    assert_eq!(
        library.read_chapter(&book, "c1").unwrap().paragraphs,
        vec!["原创正文1。"]
    );
    let receipt = library
        .export_automatic_txt(&book, dir.path().join("exports"))
        .unwrap();
    assert!(!receipt.partial);
    assert_eq!(receipt.saved_chapters, 3);
    // A completed local supplement is never queued as a network download.
    assert!(
        library
            .run_job(&snapshot.jobs[0].id, &NoNetwork, &AtomicBool::new(false))
            .is_err()
    );
}
struct NoNetwork;
impl opennovel_core::sources::SourceAdapter for NoNetwork {
    fn source_id(&self) -> &str {
        "fanqie-web"
    }
    fn fetch_chapter(
        &self,
        _: &str,
        _: &ChapterRef,
        _: &AtomicBool,
    ) -> opennovel_core::Result<opennovel_core::ImportChapter> {
        panic!("no network")
    }
}

#[test]
fn database_failure_rolls_back_bodies_and_completion_together() {
    let (dir, library, book) = setup();
    let db = rusqlite::Connection::open(dir.path().join("library.db")).unwrap();
    db.execute_batch("CREATE TRIGGER reject_second BEFORE INSERT ON chapters WHEN NEW.chapter_id='c2' BEGIN SELECT RAISE(ABORT,'controlled failure'); END;").unwrap();
    let preview = library.preview_txt_supplement(&book, &text()).unwrap();
    let jobs = library.snapshot().unwrap().jobs.len();
    assert!(
        library
            .confirm_txt_supplement(&book, &text(), &preview.fingerprint)
            .is_err()
    );
    assert_eq!(library.book_details(&book).unwrap().book.saved_chapters, 0);
    assert_eq!(library.snapshot().unwrap().jobs.len(), jobs);
}

#[test]
fn wrong_incomplete_duplicate_empty_and_changed_text_never_write() {
    let (_dir, library, book) = setup();
    for invalid in [
        "别的书\n原创正文".into(),
        text().replace("第2章 原创2\n原创正文2。\n\n", ""),
        text() + "第1章 原创1\n重复",
        text() + "第4章 目录外新增\n原创正文",
        text().replace("原创正文2。", ""),
        text().replace("第1章 原创1", "第9章 别的标题"),
        "x".repeat(opennovel_core::MAX_IMPORT_BYTES + 1),
    ] {
        assert!(library.preview_txt_supplement(&book, &invalid).is_err());
    }
    let preview = library.preview_txt_supplement(&book, &text()).unwrap();
    assert!(
        library
            .confirm_txt_supplement(&book, &(text() + "正文改变"), &preview.fingerprint)
            .is_err()
    );
    assert!(
        library
            .confirm_txt_supplement("another", &text(), &preview.fingerprint)
            .is_err()
    );
    assert_eq!(library.book_details(&book).unwrap().book.saved_chapters, 0);
}

#[test]
fn active_task_blocks_confirmation_and_catalog_changes_invalidate_preview() {
    let (_dir, library, book) = setup();
    let preview = library.preview_txt_supplement(&book, &text()).unwrap();
    let catalog = Catalog {
        source: "fanqie-web".into(),
        remote_id: "original".into(),
        title: "原创修订".into(),
        author: String::new(),
        description: String::new(),
        chapters: (1..=3)
            .map(|i| ChapterRef {
                id: format!("c{i}"),
                title: format!("第{i}章 原创{i}"),
                restricted: true,
            })
            .collect(),
    };
    let job = library.create_download(&catalog, 1, 3).unwrap();
    assert!(
        library
            .confirm_txt_supplement(&book, &text(), &preview.fingerprint)
            .is_err()
    );
    let refreshed = library.preview_txt_supplement(&book, &text()).unwrap();
    assert!(
        library
            .confirm_txt_supplement(&book, &text(), &refreshed.fingerprint)
            .is_err()
    );
    library.cancel_job(&job).unwrap();
    library
        .confirm_txt_supplement(&book, &text(), &refreshed.fingerprint)
        .unwrap();
}
