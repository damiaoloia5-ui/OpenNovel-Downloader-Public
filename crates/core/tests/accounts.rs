use opennovel_core::{Library, account_entry_url, account_navigation_allowed};
use tempfile::tempdir;
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
