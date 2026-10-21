use opennovel_core::{Error, Library};
use std::sync::atomic::AtomicBool;
use tempfile::tempdir;
fn imported(path: &std::path::Path) -> (Library, String) {
    let l = Library::open(path).unwrap();
    let j = l
        .create_import(include_str!("../../../examples/local-book.json"))
        .unwrap();
    let s = l.run_import(&j, &AtomicBool::new(false)).unwrap();
    (l, s.book_id)
}
#[test]
fn preferences_persist_without_affecting_content_or_accepting_invalid_tags() {
    let d = tempdir().unwrap();
    let path = d.path().join("main.db");
    let (l, b) = imported(&path);
    let before = l.read_chapter(&b, "001").unwrap();
    let p = l
        .organize(
            &b,
            &[
                " 喜欢 ".into(),
                "喜欢".into(),
                "Reader".into(),
                "reader".into(),
            ],
            true,
        )
        .unwrap();
    assert_eq!(p.tags, ["喜欢", "Reader"]);
    assert!(l.organize("missing", &[], false).is_err());
    assert!(l.organize(&b, &["bad\nlabel".into()], false).is_err());
    assert_eq!(
        before.paragraphs,
        l.read_chapter(&b, "001").unwrap().paragraphs
    );
    drop(l);
    let l = Library::open(path).unwrap();
    assert!(l.preferences().unwrap()[0].archived);
}
#[test]
fn online_backup_restores_content_tags_and_exports_in_a_new_directory_only() {
    let d = tempdir().unwrap();
    let (l, b) = imported(&d.path().join("main.db"));
    l.organize(&b, &["原创".into()], true).unwrap();
    let original = l
        .export(&b, d.path(), opennovel_core::ExportFormat::Epub)
        .unwrap();
    let backup = d.path().join("bundle");
    let m = l.backup(&backup).unwrap();
    assert_eq!((m.schema, m.books, m.chapters), (5, 1, 2));
    assert!(l.backup(&backup).is_err());
    let restored = Library::restore(&backup, d.path().join("restored")).unwrap();
    assert_eq!(restored.preferences().unwrap(), l.preferences().unwrap());
    for c in l.book_details(&b).unwrap().chapters {
        assert_eq!(
            l.read_chapter(&b, &c.id).unwrap().paragraphs,
            restored.read_chapter(&b, &c.id).unwrap().paragraphs
        );
    }
    let export = d.path().join("restored-exports");
    std::fs::create_dir(&export).unwrap();
    let next = restored
        .export(&b, &export, opennovel_core::ExportFormat::Epub)
        .unwrap();
    assert_eq!(
        std::fs::read(original.path).unwrap(),
        std::fs::read(next.path).unwrap()
    );
    assert!(Library::restore(&backup, d.path()).is_err());
    let db = std::fs::OpenOptions::new()
        .append(true)
        .open(backup.join("library.db"))
        .unwrap();
    use std::io::Write;
    let mut db = db;
    db.write_all(b"tampered").unwrap();
    assert!(matches!(
        Library::restore(&backup, d.path().join("bad")),
        Err(Error::Invalid(_))
    ));
}
#[test]
fn diagnostic_preview_contains_only_allowlisted_statistics_and_cannot_overwrite_files() {
    let d = tempdir().unwrap();
    let (l, b) = imported(&d.path().join("private-library.db"));
    l.organize(&b, &["private-tag".into()], false).unwrap();
    let report = l.diagnostics().unwrap();
    let text = serde_json::to_string(&report).unwrap();
    assert!(report.integrity_ok && report.foreign_keys_ok);
    for secret in [
        "private-library",
        "private-tag",
        b.as_str(),
        "paragraphs",
        "remote_id",
        d.path().to_str().unwrap(),
    ] {
        assert!(!text.contains(secret));
    }
    let path = d.path().join("diagnostics.json");
    report.save(&path).unwrap();
    assert!(report.save(&path).is_err());
}
#[test]
fn schema_four_migrates_with_backup_and_keeps_book_data() {
    let d = tempdir().unwrap();
    let path = d.path().join("old.db");
    let (l, _) = imported(&path);
    drop(l);
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("DROP TABLE book_preferences; PRAGMA user_version=4;")
        .unwrap();
    drop(db);
    let l = Library::open(&path).unwrap();
    assert_eq!(l.diagnostics().unwrap().schema, 5);
    assert_eq!(l.diagnostics().unwrap().saved_chapters, 2);
    assert!(std::fs::read_dir(d.path()).unwrap().any(|e| {
        e.unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("library-v4-backup-")
    }));
}
