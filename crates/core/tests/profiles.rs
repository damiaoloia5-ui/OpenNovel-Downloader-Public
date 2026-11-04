use opennovel_core::{ExportFormat, Library, profiles::ProfileStore};
use std::sync::atomic::AtomicBool;

#[test]
fn new_users_have_independent_books_accounts_preferences_and_exports() {
    let temp = tempfile::tempdir().unwrap();
    let store = ProfileStore::new(temp.path()).unwrap();
    let a = store.resolve(None).unwrap();
    let second = store.create("第二位用户").unwrap();
    let b = store.resolve(Some(&second.id)).unwrap();
    assert_ne!(a.database, b.database);
    assert_ne!(a.exports, b.exports);
    assert_ne!(a.webview, b.webview);
    assert_ne!(a.sessions, b.sessions);
    let la = Library::open(&a.database).unwrap();
    assert_eq!(store.adopt(&a.database).unwrap().id, a.profile.id);
    let job = la
        .create_import(include_str!("../../../examples/local-book.json"))
        .unwrap();
    let book = la
        .run_import(&job, &AtomicBool::new(false))
        .unwrap()
        .book_id;
    la.create_account("qidian", "用户甲的账号").unwrap();
    let export = la.export(&book, &a.exports, ExportFormat::Txt).unwrap();
    std::fs::write(
        a.directory.join("local-engine-choice.json"),
        "original preference",
    )
    .unwrap();
    let lb = Library::open(&b.database).unwrap();
    assert!(lb.snapshot().unwrap().books.is_empty());
    assert!(lb.snapshot().unwrap().jobs.is_empty());
    assert!(lb.accounts().unwrap().is_empty());
    assert!(!b.exports.exists());
    assert!(!b.directory.join("local-engine-choice.json").exists());
    assert!(lb.read_chapter(&book, "1").is_err());
    assert!(std::path::Path::new(&export.path).starts_with(&a.exports));
    store.select(&b.profile.id).unwrap();
    assert_eq!(
        ProfileStore::new(temp.path())
            .unwrap()
            .resolve(None)
            .unwrap()
            .database,
        b.database
    );
    assert_eq!(la.snapshot().unwrap().books.len(), 1);
    assert!(Library::check_available(&a.database).is_err());
}

#[test]
fn legacy_adoption_does_not_copy_or_open_the_original_and_other_os_roots_are_empty() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("user-a");
    std::fs::create_dir(&root).unwrap();
    let old = root.join("library.db");
    std::fs::write(&old, "original bytes, intentionally not a SQLite database").unwrap();
    let before = std::fs::read(&old).unwrap();
    let a = ProfileStore::new(&root).unwrap();
    let legacy = a.resolve(None).unwrap();
    assert_eq!(legacy.database, old);
    assert_eq!(legacy.profile.name, "原有书库");
    assert_eq!(a.adopt(&old).unwrap().id, legacy.profile.id);
    assert_eq!(std::fs::read(&old).unwrap(), before);
    let second = a.create("空白用户").unwrap();
    assert!(!a.resolve(Some(&second.id)).unwrap().database.exists());
    let b = ProfileStore::new(temp.path().join("user-b")).unwrap();
    assert!(!b.resolve(None).unwrap().database.exists());
    assert!(b.resolve(Some(&legacy.profile.id)).is_err());
    assert_eq!(std::fs::read(&old).unwrap(), before);
}

#[test]
fn invalid_or_corrupt_profiles_do_not_fall_back_to_another_users_library() {
    let temp = tempfile::tempdir().unwrap();
    let store = ProfileStore::new(temp.path()).unwrap();
    let current = store.resolve(None).unwrap();
    for name in ["", "  ", "bad\nname"] {
        assert!(store.create(name).is_err());
    }
    assert!(store.create(&"甲".repeat(41)).is_err());
    assert!(store.create(&current.profile.name).is_err());
    assert!(store.resolve(Some("../../library.db")).is_err());
    assert!(store.select("unknown").is_err());
    assert!(store.startup_marker("../../library.db").is_err());
    let path = temp.path().join("profiles.json");
    std::fs::write(&path, "broken user registry").unwrap();
    assert!(store.resolve(None).is_err());
    assert_eq!(
        std::fs::read_to_string(path).unwrap(),
        "broken user registry"
    );
}

#[test]
fn parallel_profile_creation_does_not_lose_users() {
    let temp = tempfile::tempdir().unwrap();
    let store = ProfileStore::new(temp.path()).unwrap();
    store.resolve(None).unwrap();
    let threads: Vec<_> = (0..8)
        .map(|n| {
            let s = store.clone();
            std::thread::spawn(move || s.create(&format!("用户 {n}")).unwrap())
        })
        .collect();
    for t in threads {
        t.join().unwrap();
    }
    assert_eq!(store.list().unwrap().len(), 9);
}
