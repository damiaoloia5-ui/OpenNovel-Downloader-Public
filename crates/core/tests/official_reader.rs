use opennovel_core::{
    OfficialPage, account_navigation_allowed, official_book_link, official_login_url,
};
use serde_json::json;
fn original() -> serde_json::Value {
    json!({"platform":"faloo","url":"https://b.faloo.com/123_10.html","status":"text","book_id":"123","chapter_id":"10","book_title":"原创旅行","chapter_title":"原创第一章","paragraphs":["山间有风，旅人向前行。"],"declared_words":null})
}
#[test]
fn official_routes_are_platform_bound_and_do_not_persist_login_parameters() {
    for (p, u) in [
        ("ciweimao", "https://www.ciweimao.com/book/123"),
        ("faloo", "https://b.faloo.com/123.html"),
        ("migu", "https://www.cmread.com/yuekan/novel/123.html"),
    ] {
        assert!(official_book_link(p, u).is_ok());
        assert!(account_navigation_allowed(
            p,
            &official_login_url(p).unwrap()
        ));
        for bad in [
            format!("{u}?token=secret"),
            format!("{u}#token"),
            u.replace("https://", "http://"),
            u.replace("https://", "https://user:secret@"),
        ] {
            assert!(official_book_link(p, &bad).is_err());
        }
        assert!(!account_navigation_allowed(
            p,
            &"https://evil.test/".parse().unwrap()
        ));
        assert!(!account_navigation_allowed(
            p,
            &"https://www.cmread.com.evil.test/".parse().unwrap()
        ));
    }
    assert!(official_book_link("faloo", "https://www.ciweimao.com/book/123").is_err());
    assert!(!account_navigation_allowed(
        "faloo",
        &official_login_url("migu").unwrap()
    ));
    assert!(official_login_url("unknown").is_err());
}
#[test]
fn snapshots_reject_cross_platform_page_partial_protected_or_forged_results() {
    let raw = original();
    assert!(
        OfficialPage::parse(&raw.to_string(), "faloo", "https://b.faloo.com/123_10.html").is_ok()
    );
    for (field, value) in [
        ("url", json!("https://b.faloo.com/123_11.html")),
        ("platform", json!("ciweimao")),
        ("chapter_id", json!("11")),
        ("paragraphs", json!(["订阅本章"])),
        ("declared_words", json!(10000)),
        ("status", json!("protected")),
    ] {
        let mut bad = raw.clone();
        bad[field] = value;
        assert!(
            OfficialPage::parse(&bad.to_string(), "faloo", "https://b.faloo.com/123_10.html")
                .is_err(),
            "{field}"
        );
    }
    assert!(
        OfficialPage::parse(
            &" ".repeat(2 * 1024 * 1024 + 1),
            "faloo",
            "https://b.faloo.com/123_10.html"
        )
        .is_err()
    );
    let mut unknown = raw.clone();
    unknown["cookie"] = json!("not a real secret");
    assert!(
        OfficialPage::parse(
            &unknown.to_string(),
            "faloo",
            "https://b.faloo.com/123_10.html"
        )
        .is_err()
    );
    let mut migu = raw.clone();
    migu["platform"] = json!("migu");
    migu["url"] = json!("https://www.cmread.com/yuekan/novel/123.html");
    assert!(
        OfficialPage::parse(
            &migu.to_string(),
            "migu",
            "https://www.cmread.com/yuekan/novel/123.html"
        )
        .is_err()
    );
}
#[test]
fn page_txt_marks_visible_scope_never_overwrites_and_revalidates_mutated_fields() {
    let dir = tempfile::tempdir().unwrap();
    let mut p = OfficialPage::parse(
        &original().to_string(),
        "faloo",
        "https://b.faloo.com/123_10.html",
    )
    .unwrap();
    let first = p.save_txt(dir.path()).unwrap();
    let second = p.save_txt(dir.path()).unwrap();
    assert_ne!(first, second);
    let text = std::fs::read_to_string(&first).unwrap();
    assert!(text.contains("未核验全章或全书完整性") && text.contains("山间有风"));
    p.url = "https://user:secret@b.faloo.com/123_10.html".into();
    assert!(p.save_txt(dir.path()).is_err());
    assert_eq!(std::fs::read_to_string(&first).unwrap(), text);
    p.status = "protected".into();
    assert!(p.save_txt(dir.path()).is_err());
}
