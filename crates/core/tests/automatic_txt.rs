use opennovel_core::{ExportFormat, Library};
use std::{fs, sync::atomic::AtomicBool};
use tempfile::tempdir;
fn imported(l: &Library) -> String {
    let job = l
        .create_import(include_str!("../../../examples/local-book.json"))
        .unwrap();
    l.run_import(&job, &AtomicBool::new(false)).unwrap().book_id
}
#[test]
fn automatic_txt_is_idempotent_persistent_and_recovers_a_deleted_file() {
    let d = tempdir().unwrap();
    let db = d.path().join("library.db");
    let out = d.path().join("exports");
    let l = Library::open(&db).unwrap();
    let book = imported(&l);
    let r = l.export_automatic_txt(&book, &out).unwrap();
    let text = fs::read_to_string(&r.path).unwrap();
    assert!(!r.partial);
    assert_eq!(r.saved_chapters, 2);
    assert_eq!(l.export_automatic_txt(&book, &out).unwrap().path, r.path);
    assert_eq!(l.book_details(&book).unwrap().exports.len(), 1);
    assert!(l.export_record_id(&book, &r.path).is_ok());
    drop(l);
    let l = Library::open(&db).unwrap();
    assert_eq!(l.export_automatic_txt(&book, &out).unwrap().path, r.path);
    fs::remove_file(&r.path).unwrap();
    l.export_automatic_txt(&book, &out).unwrap();
    assert_eq!(fs::read_to_string(&r.path).unwrap(), text);
    assert_eq!(l.book_details(&book).unwrap().exports.len(), 1);
}
#[test]
fn auto_never_overwrites_edited_text_or_treats_it_as_current() {
    let d = tempdir().unwrap();
    let l = Library::open(d.path().join("library.db")).unwrap();
    let book = imported(&l);
    let r = l.export_automatic_txt(&book, d.path()).unwrap();
    fs::write(&r.path, "用户的原创批注，请保留").unwrap();
    assert!(l.export_automatic_txt(&book, d.path()).is_err());
    assert_eq!(
        fs::read_to_string(&r.path).unwrap(),
        "用户的原创批注，请保留"
    );
    assert_eq!(l.snapshot().unwrap().books[0].saved_chapters, 2);
    let manual = l.export(&book, d.path(), ExportFormat::Txt).unwrap();
    assert_ne!(manual.path, r.path);
    assert!(l.export(&book, d.path(), ExportFormat::Txt).is_err());
}
#[test]
fn empty_book_fails_without_an_empty_file_and_directory_failure_preserves_body() {
    let d = tempdir().unwrap();
    let l = Library::open(d.path().join("library.db")).unwrap();
    let job = l
        .create_import(include_str!("../../../examples/local-book.json"))
        .unwrap();
    let book = l.job(&job).unwrap().book_id;
    let out = d.path().join("not-yet");
    assert!(l.export_automatic_txt(&book, &out).is_err());
    assert!(!out.exists());
    l.run_import(&job, &AtomicBool::new(false)).unwrap();
    fs::write(&out, "不是文件夹").unwrap();
    assert!(l.export_automatic_txt(&book, &out).is_err());
    assert_eq!(l.snapshot().unwrap().books[0].saved_chapters, 2);
    fs::remove_file(&out).unwrap();
    assert!(l.export_automatic_txt(&book, &out).is_ok());
}
#[test]
fn orphan_file_after_record_failure_is_reconciled_without_overwrite() {
    let d = tempdir().unwrap();
    let db = d.path().join("library.db");
    let l = Library::open(&db).unwrap();
    let book = imported(&l);
    let conn = rusqlite::Connection::open(&db).unwrap();
    conn.execute_batch("CREATE TRIGGER fail_export BEFORE INSERT ON exports BEGIN SELECT RAISE(FAIL,'controlled'); END").unwrap();
    let out = d.path().join("exports");
    assert!(l.export_automatic_txt(&book, &out).is_err());
    let file = fs::read_dir(&out).unwrap().next().unwrap().unwrap().path();
    let before = fs::read(&file).unwrap();
    conn.execute_batch("DROP TRIGGER fail_export").unwrap();
    let r = l.export_automatic_txt(&book, &out).unwrap();
    assert_eq!(fs::read(&r.path).unwrap(), before);
    assert_eq!(fs::read_dir(out).unwrap().count(), 1);
    assert_eq!(l.book_details(&book).unwrap().exports.len(), 1);
}
