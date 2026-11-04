use opennovel_core::{ExportFormat, Library};
use std::sync::atomic::AtomicBool;

#[test]
#[ignore = "explicit local performance measurement, never a timing gate"]
fn benchmark_unchanged_library_refresh() {
    let dir = tempfile::tempdir().unwrap();
    let library = Library::open(dir.path().join("library.db")).unwrap();
    for index in 0..100 {
        let mut value: serde_json::Value =
            serde_json::from_str(include_str!("../../../examples/local-book.json")).unwrap();
        value["id"] = format!("original-{index}").into();
        let job = library.create_import(&value.to_string()).unwrap();
        library.run_import(&job, &AtomicBool::new(false)).unwrap();
    }
    let initial = library.snapshot_if_changed(None).unwrap().unwrap();
    let start = std::time::Instant::now();
    for _ in 0..200 {
        std::hint::black_box(library.snapshot().unwrap());
    }
    let full = start.elapsed();
    let start = std::time::Instant::now();
    for _ in 0..200 {
        assert!(
            library
                .snapshot_if_changed(Some(&initial.revision))
                .unwrap()
                .is_none()
        );
    }
    println!(
        "original_books=100 reads=200 full_snapshot_us={} unchanged_check_us={}",
        full.as_micros(),
        start.elapsed().as_micros()
    );
}

#[test]
fn revisions_detect_writes_but_not_reads_and_do_not_survive_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.db");
    let library = Library::open(&path).unwrap();
    let first = library.snapshot_if_changed(None).unwrap().unwrap();
    library.snapshot().unwrap();
    assert!(
        library
            .snapshot_if_changed(Some(&first.revision))
            .unwrap()
            .is_none()
    );
    let job = library
        .create_import(include_str!("../../../examples/local-book.json"))
        .unwrap();
    let queued = library
        .snapshot_if_changed(Some(&first.revision))
        .unwrap()
        .unwrap();
    assert_eq!(queued.snapshot.jobs[0].saved, 0);
    library.run_import(&job, &AtomicBool::new(false)).unwrap();
    let saved = library
        .snapshot_if_changed(Some(&queued.revision))
        .unwrap()
        .unwrap();
    assert_eq!(saved.snapshot.books[0].saved_chapters, 2);
    assert!(
        library
            .snapshot_if_changed(Some(&saved.revision))
            .unwrap()
            .is_none()
    );
    drop(library);
    let reopened = Library::open(path).unwrap();
    assert!(
        reopened
            .snapshot_if_changed(Some(&saved.revision))
            .unwrap()
            .is_some()
    );
}

#[test]
fn selected_export_does_not_decode_unselected_bodies() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.db");
    let library = Library::open(&path).unwrap();
    let job = library
        .create_import(include_str!("../../../examples/local-book.json"))
        .unwrap();
    let book = library
        .run_import(&job, &AtomicBool::new(false))
        .unwrap()
        .book_id;
    // Original fixture only: make an unrelated body undecodable to prove it is not loaded.
    let db = rusqlite::Connection::open(&path).unwrap();
    assert_eq!(db.execute("UPDATE chapters SET paragraphs='invalid json' WHERE book_id=?1 AND position=(SELECT MAX(position) FROM chapters WHERE book_id=?1)", [&book]).unwrap(), 1);
    let export = library
        .export_selected(&book, "1", dir.path().join("out"), ExportFormat::Txt)
        .unwrap();
    assert_eq!(export.saved_chapters, 1);
    assert_eq!(export.total_chapters, 1);
    assert!(
        library
            .export(&book, dir.path().join("all"), ExportFormat::Txt)
            .is_err()
    );
}
