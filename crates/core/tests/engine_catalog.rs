use opennovel_core::sources::{Fanqie, LOCAL_ENGINE_SOURCE};
use serde_json::{Value, json};

fn fixture() -> Value {
    json!({"page":{"bookId":"10000000001","bookName":"原创目录","author":"测试",
        "chapterListWithVolume":[[
            {"itemId":"10000000002","title":"免费网页章","needPay":0,"isChapterLock":false},
            {"itemId":"10000000003","title":"免费 App 章","needPay":0,"isChapterLock":true},
            {"itemId":"10000000004","title":"付费章","needPay":1,"isChapterLock":true}]],
        "itemIds":["10000000004","10000000003","10000000002"]}})
}
fn page(value: &Value) -> String {
    format!("window.__INITIAL_STATE__ = {value};")
}

#[test]
fn explicit_engine_catalog_distinguishes_app_only_and_paid_without_changing_web() {
    let raw = page(&fixture());
    let web = Fanqie::parse_catalog(&raw, "10000000001").unwrap();
    let engine = Fanqie::parse_engine_catalog(&raw, "10000000001").unwrap();
    assert_eq!(web.source, "fanqie-web");
    assert_eq!(engine.source, LOCAL_ENGINE_SOURCE);
    assert_eq!(
        web.chapters
            .iter()
            .map(|c| c.restricted)
            .collect::<Vec<_>>(),
        [false, true, true]
    );
    assert_eq!(
        engine
            .chapters
            .iter()
            .map(|c| c.restricted)
            .collect::<Vec<_>>(),
        [false, false, true]
    );
    assert_eq!(engine.chapters[1].id, "10000000003");
}

#[test]
fn engine_catalog_preserves_paid_book_flags_and_refuses_ambiguous_permissions() {
    for field in ["isPaidPublication", "isPaidStory"] {
        let mut value = fixture();
        value["page"][field] = json!(true);
        assert!(
            Fanqie::parse_engine_catalog(&page(&value), "10000000001")
                .unwrap()
                .chapters
                .iter()
                .all(|c| c.restricted)
        );
        value["page"][field] = json!("true");
        assert!(Fanqie::parse_engine_catalog(&page(&value), "10000000001").is_err());
    }
    for field in ["needPay", "isChapterLock"] {
        let mut value = fixture();
        value["page"]["chapterListWithVolume"][0][1]
            .as_object_mut()
            .unwrap()
            .remove(field);
        assert!(Fanqie::parse_engine_catalog(&page(&value), "10000000001").is_err());
    }
}
