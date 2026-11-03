use opennovel_core::{Error, ExportFormat, ImportDocument, Library};
use std::{
    io::Read,
    sync::atomic::{AtomicBool, Ordering},
};
use tempfile::tempdir;

const EXAMPLE: &str = include_str!("../../../examples/local-book.json");

#[test]
fn import_persists_identity_order_and_offline_exports() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("library.db");
    let library = Library::open(&path).unwrap();
    let job = library.create_import(EXAMPLE).unwrap();
    let result = library.run_import(&job, &AtomicBool::new(false)).unwrap();
    assert_eq!(
        (result.state.as_str(), result.saved, result.total),
        ("completed", 2, 2)
    );
    assert!(library.create_import(EXAMPLE).is_err());
    let receipt = library
        .export(&result.book_id, directory.path(), ExportFormat::Txt)
        .unwrap();
    let txt = std::fs::read_to_string(&receipt.path).unwrap();
    assert!(txt.find("第一章").unwrap() < txt.find("第二章").unwrap());
    assert!(!receipt.partial);
    let repeated = library
        .export(&result.book_id, directory.path(), ExportFormat::Txt)
        .unwrap_err();
    assert!(matches!(&repeated, Error::Conflict(message) if message.contains("导出文件已存在")));
    assert_eq!(std::fs::read_to_string(&receipt.path).unwrap(), txt);
    assert_eq!(
        library.book_details(&result.book_id).unwrap().exports.len(),
        1
    );
    drop(library);
    let reopened = Library::open(&path).unwrap();
    let snapshot = reopened.snapshot().unwrap();
    assert_eq!(snapshot.books[0].saved_chapters, 2);
    let connection = rusqlite::Connection::open(path).unwrap();
    let identities: Vec<String> = connection
        .prepare("SELECT chapter_id FROM chapters ORDER BY position")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(identities, ["001", "010"]);
}

#[test]
fn cancel_preserves_partial_content_and_resumes_without_duplicates() {
    let directory = tempdir().unwrap();
    let library = Library::open(directory.path().join("library.db")).unwrap();
    let job = library.create_import(EXAMPLE).unwrap();
    let token = AtomicBool::new(false);
    let result = library
        .run_import_observed(&job, &token, |_| token.store(true, Ordering::Release))
        .unwrap();
    assert_eq!(
        (result.state.as_str(), result.saved, result.total),
        ("cancelled", 1, 2)
    );
    let receipt = library
        .export(&result.book_id, directory.path(), ExportFormat::Txt)
        .unwrap();
    assert!(receipt.partial);
    assert!(receipt.path.ends_with(".txt"));
    assert!(!receipt.path.contains(&result.book_id));
    assert!(
        std::fs::read_to_string(receipt.path)
            .unwrap()
            .contains("1 / 2 章（部分内容）")
    );
    let resumed = library.run_import(&job, &AtomicBool::new(false)).unwrap();
    assert_eq!((resumed.state.as_str(), resumed.saved), ("completed", 2));
    assert!(library.run_import(&job, &AtomicBool::new(false)).is_err());
}

#[test]
fn cancelled_before_first_chapter_does_not_save_or_export() {
    let directory = tempdir().unwrap();
    let library = Library::open(directory.path().join("library.db")).unwrap();
    let job = library.create_import(EXAMPLE).unwrap();
    let result = library.run_import(&job, &AtomicBool::new(true)).unwrap();
    assert_eq!((result.state.as_str(), result.saved), ("cancelled", 0));
    assert!(
        library
            .export(&result.book_id, directory.path(), ExportFormat::Epub)
            .is_err()
    );
}

#[test]
fn interrupted_worker_reopens_as_paused_and_retains_committed_chapter() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("library.db");
    let library = Library::open(&path).unwrap();
    let job = library.create_import(EXAMPLE).unwrap();
    let interrupted = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        library.run_import_observed(&job, &AtomicBool::new(false), |_| {
            panic!("simulated process interruption")
        })
    }));
    assert!(interrupted.is_err());
    drop(library);
    let reopened = Library::open(path).unwrap();
    let state = reopened.job(&job).unwrap();
    assert_eq!((state.state.as_str(), state.saved), ("paused", 1));
    assert_eq!(
        reopened
            .run_import(&job, &AtomicBool::new(false))
            .unwrap()
            .saved,
        2
    );
}

#[test]
fn invalid_input_never_creates_a_book_or_job() {
    let directory = tempdir().unwrap();
    let library = Library::open(directory.path().join("library.db")).unwrap();
    let mut document: serde_json::Value = serde_json::from_str(EXAMPLE).unwrap();
    document["chapters"][1]["id"] = document["chapters"][0]["id"].clone();
    assert!(library.create_import(&document.to_string()).is_err());
    document["chapters"][1]["id"] = "002".into();
    document["chapters"][0]["paragraphs"] = serde_json::json!([" "]);
    assert!(library.create_import(&document.to_string()).is_err());
    assert!(library.snapshot().unwrap().books.is_empty());
    assert!(library.snapshot().unwrap().jobs.is_empty());
    assert!(ImportDocument::parse(&" ".repeat(16 * 1024 * 1024 + 1)).is_err());
}

#[test]
fn failed_chapter_transaction_rolls_back_both_text_and_completion() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("library.db");
    let library = Library::open(&path).unwrap();
    let job = library.create_import(EXAMPLE).unwrap();
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection.execute_batch("CREATE TRIGGER fail_progress BEFORE UPDATE ON job_items BEGIN SELECT RAISE(ABORT, 'injected failure'); END;").unwrap();
    assert!(library.run_import(&job, &AtomicBool::new(false)).is_err());
    assert_eq!(library.job(&job).unwrap().state, "failed");
    assert_eq!(library.snapshot().unwrap().books[0].saved_chapters, 0);
    let completed: u32 = connection
        .query_row(
            "SELECT COUNT(*) FROM job_items WHERE state='completed'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(completed, 0);
    connection
        .execute_batch("DROP TRIGGER fail_progress;")
        .unwrap();
    assert_eq!(
        library
            .run_import(&job, &AtomicBool::new(false))
            .unwrap()
            .saved,
        2
    );
}

#[test]
fn epub_has_stored_mimetype_valid_xml_and_escaped_text() {
    let directory = tempdir().unwrap();
    let library = Library::open(directory.path().join("library.db")).unwrap();
    let mut document: serde_json::Value = serde_json::from_str(EXAMPLE).unwrap();
    document["title"] = "书 < & >".into();
    document["chapters"][0]["paragraphs"] =
        serde_json::json!(["<script>alert('x')</script> & 文字"]);
    let job = library.create_import(&document.to_string()).unwrap();
    let result = library.run_import(&job, &AtomicBool::new(false)).unwrap();
    let receipt = library
        .export(&result.book_id, directory.path(), ExportFormat::Epub)
        .unwrap();
    let mut archive = zip::ZipArchive::new(std::fs::File::open(receipt.path).unwrap()).unwrap();
    let mut mimetype = String::new();
    {
        let mut first = archive.by_index(0).unwrap();
        assert_eq!(first.name(), "mimetype");
        assert_eq!(first.compression(), zip::CompressionMethod::Stored);
        first.read_to_string(&mut mimetype).unwrap();
    }
    assert_eq!(mimetype, "application/epub+zip");
    for index in 1..archive.len() {
        let mut file = archive.by_index(index).unwrap();
        let mut xml = String::new();
        file.read_to_string(&mut xml).unwrap();
        let parsed = roxmltree::Document::parse(&xml).unwrap();
        if file.name() == "EPUB/chapter-0.xhtml" {
            assert!(!parsed.descendants().any(|node| node.has_tag_name("script")));
            assert!(
                parsed
                    .descendants()
                    .any(|node| node.text() == Some("<script>alert('x')</script> & 文字"))
            );
        }
    }
}

#[test]
fn epub_without_an_author_omits_optional_creator_metadata() {
    let directory = tempdir().unwrap();
    let library = Library::open(directory.path().join("library.db")).unwrap();
    let mut document: serde_json::Value = serde_json::from_str(EXAMPLE).unwrap();
    document.as_object_mut().unwrap().remove("author");
    let job = library.create_import(&document.to_string()).unwrap();
    let result = library.run_import(&job, &AtomicBool::new(false)).unwrap();
    let receipt = library
        .export(&result.book_id, directory.path(), ExportFormat::Epub)
        .unwrap();
    let mut archive = zip::ZipArchive::new(std::fs::File::open(receipt.path).unwrap()).unwrap();
    let mut xml = String::new();
    archive
        .by_name("EPUB/package.opf")
        .unwrap()
        .read_to_string(&mut xml)
        .unwrap();
    let parsed = roxmltree::Document::parse(&xml).unwrap();
    assert!(
        !parsed
            .descendants()
            .any(|node| node.tag_name().name() == "creator")
    );
}

#[test]
fn another_library_process_cannot_acquire_write_ownership() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("library.db");
    let first = Library::open(&path).unwrap();
    assert!(matches!(
        Library::open(&path),
        Err(opennovel_core::Error::Locked)
    ));
    drop(first);
    assert!(Library::open(path).is_ok());
}

#[test]
fn newer_schema_is_rejected_without_migration_or_version_change() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("future.db");
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection.pragma_update(None, "user_version", 11).unwrap();
    assert!(Library::open(&path).is_err());
    let version: u32 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(version, 11);
    let tables: u32 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(tables, 0);
}
