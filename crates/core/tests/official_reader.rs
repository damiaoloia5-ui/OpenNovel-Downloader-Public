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
#[test]
fn readable_page_keeps_long_text_without_a_membership_or_word_quota() {
    let mut raw = original();
    raw["paragraphs"] = json!(["山".repeat(200_001)]);
    raw["declared_words"] = json!(1000);
    let page =
        OfficialPage::parse(&raw.to_string(), "faloo", "https://b.faloo.com/123_10.html").unwrap();
    assert_eq!(page.paragraphs[0].chars().count(), 200_001);
    raw["declared_words"] = json!(200001);
    assert!(
        OfficialPage::parse(&raw.to_string(), "faloo", "https://b.faloo.com/123_10.html").is_ok()
    );
}

#[test]
fn migu_mobile_preview_keeps_identity_metadata_and_explicit_excerpt_scope() {
    use opennovel_core::official_page_url;
    let url = "https://n.cmread.com/nap/p/wap_detail_new.jsp?bid=123";
    assert_eq!(
        official_book_link("migu", "https://www.cmread.com/yuekan/novel/123.html")
            .unwrap()
            .as_str(),
        url
    );
    assert_eq!(
        official_book_link("migu", &format!("{url}&cm=demo&type=1"))
            .unwrap()
            .as_str(),
        url
    );
    for bad in [
        format!("{url}&bid=456"),
        format!("{url}&token=no"),
        url.replace("bid=123", "bid=0"),
        url.replace("n.cmread.com", "evil.test"),
    ] {
        assert!(official_book_link("migu", &bad).is_err());
    }
    assert_eq!(
        official_page_url("migu", &format!("{url}&tracking=drop#drop"))
            .unwrap()
            .as_str(),
        url
    );
    let raw = serde_json::json!({"platform":"migu","url":url,"status":"excerpt","book_id":"123","chapter_id":"","book_title":"原创山路","chapter_title":"官网公开试读","author":"原创作者","description":"关于山间行走的原创故事。","paragraphs":["第一章","旅人抵达山顶，看到灯火。"],"declared_words":null});
    let page = OfficialPage::parse(&raw.to_string(), "migu", url).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let file = page.save_txt(dir.path()).unwrap();
    let txt = std::fs::read_to_string(file).unwrap();
    assert!(
        txt.contains("官网公开试读摘录")
            && txt.contains("不计入书库完成进度")
            && txt.contains("原创作者")
            && txt.contains("关于山间")
    );
    for (key, value) in [
        ("status", serde_json::json!("text")),
        ("chapter_id", serde_json::json!("1")),
        ("book_id", serde_json::json!("456")),
        ("declared_words", serde_json::json!(20)),
        ("url", serde_json::json!(format!("{url}&token=no"))),
    ] {
        let mut bad = raw.clone();
        bad[key] = value;
        assert!(
            OfficialPage::parse(&bad.to_string(), "migu", url).is_err(),
            "{key}"
        );
    }
}
