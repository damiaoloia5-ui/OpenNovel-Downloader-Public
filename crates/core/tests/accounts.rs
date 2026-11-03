use opennovel_core::{Library, account_entry_url, account_login_url, account_navigation_allowed};
use tempfile::tempdir;

#[test]
fn qidian_catalog_entry_keeps_the_same_strict_book_identity_boundary() {
    let url = account_entry_url("qidian", "https://www.qidian.com/book/123/catalog/").unwrap();
    assert_eq!(url.path(), "/book/123/catalog/");
    for input in [
        "https://www.qidian.com/book/123catalog/",
        "https://www.qidian.com/book/123/catalog/456/",
        "https://www.qidian.com/book/123/catalog/?token=test",
        "https://www.qidian.com/book/123/catalog/#token",
        "https://user@www.qidian.com/book/123/catalog/",
        "https://www.qidian.com.evil/book/123/catalog/",
    ] {
        assert!(account_entry_url("qidian", input).is_err(), "{input}");
    }
    assert!(account_entry_url("shuqi", "https://www.shuqi.com/book/123/catalog/").is_err());
}

#[test]
fn qidian_empty_entry_avoids_homepage_overlay_without_accepting_arbitrary_links() {
    let login = account_entry_url("qidian", "  ").unwrap();
    assert_eq!(login.scheme(), "https");
    assert_eq!(login.host_str(), Some("my.qidian.com"));
    assert_eq!(login.path(), "/bookcase/");
    assert!(login.query().is_none() && login.fragment().is_none());
    assert_eq!(account_login_url("qidian").unwrap(), login);
    assert_eq!(
        account_login_url("shuqi").unwrap().as_str(),
        "https://www.shuqi.com/"
    );
    assert!(account_login_url("unknown").is_err());
    assert!(account_navigation_allowed("qidian", &login));
    assert!(!account_navigation_allowed("shuqi", &login));
    assert_eq!(
        account_entry_url("shuqi", "").unwrap().as_str(),
        "https://www.shuqi.com/"
    );
    assert!(account_entry_url("qidian", login.as_str()).is_err());
    assert_eq!(
        account_entry_url("qidian", "https://www.qidian.com/book/1045749425/")
            .unwrap()
            .path(),
        "/book/1045749425/"
    );
}

#[test]
fn clean_platform_entries_and_navigation_never_cross_accounts_or_protocols() {
    let u = account_entry_url("qidian", "https://www.qidian.com/book/1045749425/").unwrap();
    assert!(account_navigation_allowed("qidian", &u));
    assert!(!account_navigation_allowed("shuqi", &u));
    assert!(account_entry_url("shuqi", "https://www.shuqi.com/book/123.html").is_ok());
    for url in [
        "https://www.qidian.com/book/1/?token=secret",
        "https://www.qidian.com/book/1/#token",
        "https://user@www.qidian.com/book/1/",
        "https://www.qidian.com.evil/book/1/",
        "http://www.qidian.com/book/1/",
        "https://www.qidian.com:1234/book/1/",
        "file:///book/1/",
        "https://www.qidian.com/book/a/",
    ] {
        assert!(account_entry_url("qidian", url).is_err(), "{url}");
    }
    for url in [
        "https://qidian.com.evil/",
        "https://qidian.com@evil/",
        "http://www.qidian.com/",
        "javascript:alert(1)",
        "file:///C:/secret",
        "https://www.shuqi.com/",
    ] {
        assert!(!account_navigation_allowed("qidian", &url.parse().unwrap()));
    }
    assert!(account_navigation_allowed(
        "qidian",
        &"https://passport.qidian.com/".parse().unwrap()
    ));
}
#[test]
fn local_account_records_are_bounded_and_restores_never_imply_active_sessions() {
    let d = tempdir().unwrap();
    let db = d.path().join("library.db");
    let l = Library::open(&db).unwrap();
    assert!(l.create_account("unknown", "alias").is_err());
    assert!(l.create_account("qidian", "\n").is_err());
    let a = l.create_account("qidian", "个人起点").unwrap();
    let b = l.create_account("shuqi", "个人书旗").unwrap();
    assert_ne!(a.id, b.id);
    assert_eq!(l.account(&a.id).unwrap(), a);
    for i in 2..10 {
        l.create_account("qidian", &format!("测试{i}")).unwrap();
    }
    assert!(l.create_account("qidian", "超出上限").is_err());
    let backup = d.path().join("backup");
    l.backup(&backup).unwrap();
    let restored = Library::restore(&backup, d.path().join("restored")).unwrap();
    assert_eq!(restored.accounts().unwrap().len(), 10);
    let json = serde_json::to_value(restored.accounts().unwrap()).unwrap();
    for row in json.as_array().unwrap() {
        assert_eq!(row.as_object().unwrap().len(), 3);
    }
    l.remove_account(&a.id).unwrap();
    assert!(l.account(&a.id).is_err());
    assert!(l.remove_account(&a.id).is_err());
    drop(l);
    let l = Library::open(db).unwrap();
    assert_eq!(l.accounts().unwrap().len(), 9);
}

#[test]
fn all_supported_platform_accounts_survive_restart_backup_and_restore() {
    let d = tempdir().unwrap();
    let db = d.path().join("library.db");
    let l = Library::open(&db).unwrap();
    let mut expected = Vec::new();
    for p in [
        "qidian", "shuqi", "zongheng", "17k", "jinjiang", "qimao", "ciweimao", "faloo", "migu",
    ] {
        expected.push(l.create_account(p, &format!("原创{p}账户")).unwrap());
    }
    assert_eq!(l.accounts().unwrap(), expected);
    let backup = d.path().join("backup");
    l.backup(&backup).unwrap();
    assert_eq!(
        Library::restore(&backup, d.path().join("restored"))
            .unwrap()
            .accounts()
            .unwrap(),
        expected
    );
    drop(l);
    assert_eq!(Library::open(&db).unwrap().accounts().unwrap(), expected);
}

#[test]
fn legacy_account_constraint_migrates_with_verified_backup_and_preserves_content() {
    use std::sync::atomic::AtomicBool;
    let d = tempdir().unwrap();
    let db = d.path().join("library.db");
    let l = Library::open(&db).unwrap();
    let job = l
        .create_import(include_str!("../../../examples/local-book.json"))
        .unwrap();
    l.run_import(&job, &AtomicBool::new(false)).unwrap();
    let account = l.create_account("qidian", "原始别名").unwrap();
    let before = serde_json::to_value(l.snapshot().unwrap()).unwrap();
    drop(l);
    let conn = rusqlite::Connection::open(&db).unwrap();
    conn.execute_batch("DROP TABLE automatic_exports; ALTER TABLE accounts RENAME TO accounts_fixture; CREATE TABLE accounts(id TEXT PRIMARY KEY,platform TEXT NOT NULL CHECK(platform IN ('qidian','shuqi')),label TEXT NOT NULL); INSERT INTO accounts SELECT * FROM accounts_fixture; DROP TABLE accounts_fixture; PRAGMA user_version=9;").unwrap();
    drop(conn);
    let l = Library::open(&db).unwrap();
    assert_eq!(l.account(&account.id).unwrap(), account);
    assert_eq!(serde_json::to_value(l.snapshot().unwrap()).unwrap(), before);
    l.create_account("faloo", "飞卢迁移验证").unwrap();
    let backups = std::fs::read_dir(d.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with("library-v9-backup-")
        })
        .collect::<Vec<_>>();
    assert_eq!(backups.len(), 1);
    let old = rusqlite::Connection::open(backups[0].path()).unwrap();
    assert_eq!(
        old.pragma_query_value(None, "user_version", |r| r.get::<_, u32>(0))
            .unwrap(),
        9
    );
    assert_eq!(
        old.query_row(
            "SELECT label FROM accounts WHERE id=?1",
            [&account.id],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "原始别名"
    );
    assert!(
        old.execute(
            "INSERT INTO accounts VALUES('blocked','faloo','old schema')",
            []
        )
        .is_err()
    );
    assert_eq!(
        old.query_row("SELECT COUNT(*) FROM chapters", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        2
    );
    drop(old);
    drop(l);
    assert_eq!(Library::open(&db).unwrap().accounts().unwrap().len(), 2);
    assert_eq!(
        std::fs::read_dir(d.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e
                .file_name()
                .to_string_lossy()
                .starts_with("library-v9-backup-"))
            .count(),
        1
    );
}

#[test]
fn failed_account_migration_leaves_old_schema_and_account_records_unchanged() {
    let d = tempdir().unwrap();
    let path = d.path().join("library.db");
    let l = Library::open(&path).unwrap();
    let account = l.create_account("shuqi", "原始书旗记录").unwrap();
    drop(l);
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("PRAGMA user_version=9; CREATE TABLE accounts_v10(sentinel TEXT); INSERT INTO accounts_v10 VALUES('original sentinel');").unwrap();
    drop(db);
    assert!(Library::open(&path).is_err());
    let db = rusqlite::Connection::open(&path).unwrap();
    assert_eq!(
        db.pragma_query_value(None, "user_version", |r| r.get::<_, u32>(0))
            .unwrap(),
        9
    );
    assert_eq!(
        db.query_row(
            "SELECT label FROM accounts WHERE id=?1",
            [account.id],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "原始书旗记录"
    );
    assert_eq!(
        db.query_row("SELECT sentinel FROM accounts_v10", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        "original sentinel"
    );
}
