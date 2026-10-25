use opennovel_core::{ExportFormat, Library, SessionCatalog, SessionChapter, qidian_capture_url};
use serde_json::{Value, json};
use tempfile::tempdir;

const BOOK: &str = "https://www.qidian.com/book/123/";
fn catalog_value() -> Value {
    json!({"url":BOOK,"title":"原创受控测试书","author":"测试作者","declared_total":3,
        "chapters":[{"id":"10","title":"第一章 出发"},{"id":"30","title":"第二章 山路"},{"id":"20","title":"第三章 归途"}]})
}
fn catalog() -> SessionCatalog {
    SessionCatalog::parse(&catalog_value().to_string(), BOOK).unwrap()
}
fn chapter_value(id: &str, title: &str) -> Value {
    json!({"url":format!("https://www.qidian.com/chapter/123/{id}/"),"title":title,
        "paragraphs":["这是一段专门用于验证保存与离线导出的原创测试文字。山路尽头有一座小屋，旅人停下脚步，决定等雨停后再出发。"],"declared_words":50})
}
fn chapter(id: &str, title: &str) -> SessionChapter {
    let value = chapter_value(id, title);
    SessionChapter::parse(&value.to_string(), value["url"].as_str().unwrap()).unwrap()
}

#[test]
fn rejects_foreign_origins_tokens_incomplete_or_mixed_catalogs() {
    assert_eq!(
        qidian_capture_url("https://www.qidian.com/book/123/catalog/").unwrap(),
        ("123".into(), None)
    );
    for url in [
        "https://passport.qidian.com/",
        "http://www.qidian.com/book/123/",
        "https://www.qidian.com.evil/book/123/",
        "https://user@www.qidian.com/book/123/",
        "https://www.qidian.com/book/123/?token=private",
        "https://www.qidian.com/book/123/#Catalog",
        "https://www.qidian.com:9999/book/123/",
        "https://www.qidian.com/book/a/",
    ] {
        assert!(qidian_capture_url(url).is_err(), "{url}");
    }
    assert!(
        SessionCatalog::parse(
            &catalog_value().to_string(),
            "https://www.qidian.com/book/999/"
        )
        .is_err()
    );
    for mutate in 0..6 {
        let mut value = catalog_value();
        match mutate {
            0 => value["declared_total"] = json!(4),
            1 => value["chapters"][1]["id"] = json!("10"),
            2 => value["chapters"][1]["id"] = json!("not-an-id"),
            3 => value["chapters"][1]["title"] = json!("\u{fffe}"),
            4 => value["cookies"] = json!("not accepted"),
            _ => value["declared_total"] = json!(0),
        }
        assert!(SessionCatalog::parse(&value.to_string(), BOOK).is_err());
    }
}

#[test]
fn rejects_trials_protected_glyphs_empty_and_changed_chapter_pages() {
    let original = chapter_value("10", "第一章 出发");
    let url = original["url"].as_str().unwrap();
    for paragraphs in [
        json!(["订阅本章后继续阅读"]),
        json!(["\u{e001}".repeat(50)]),
        json!(["短试读"]),
        json!([]),
        json!(["测试\u{0001}".repeat(50)]),
    ] {
        let mut value = original.clone();
        value["paragraphs"] = paragraphs;
        assert!(SessionChapter::parse(&value.to_string(), url).is_err());
    }
    let mut no_words = original.clone();
    no_words["declared_words"] = json!(0);
    assert!(SessionChapter::parse(&no_words.to_string(), url).is_err());
    assert!(
        SessionChapter::parse(
            &original.to_string(),
            "https://www.qidian.com/chapter/123/30/"
        )
        .is_err()
    );
    assert!(SessionChapter::parse(&original.to_string(), BOOK).is_err());
    assert!(
        SessionChapter::parse(&" ".repeat(opennovel_core::MAX_CAPTURE_BYTES + 1), url).is_err()
    );
}

#[test]
fn saves_out_of_order_with_missing_markers_and_reopens_offline() {
    let d = tempdir().unwrap();
    let path = d.path().join("library.db");
    let l = Library::open(&path).unwrap();
    assert!(
        l.save_session_chapter(&chapter("20", "第三章 归途"))
            .is_err()
    );
    let book = l.register_session_catalog(&catalog()).unwrap();
    let err = l
        .export(&book, d.path().join("exports"), ExportFormat::Txt)
        .unwrap_err()
        .to_string();
    assert!(err.contains("尚无已保存章节"));
    let third = l
        .save_session_chapter(&chapter("20", "第三章 归途"))
        .unwrap();
    assert_eq!(third.position, 3);
    assert_eq!(third.saved, 1);
    assert_eq!(third.total, 3);
    l.save_session_chapter(&chapter("10", "第一章 出发"))
        .unwrap();
    let details = l.book_details(&book).unwrap();
    assert_eq!(
        details
            .chapters
            .iter()
            .map(|c| (c.id.as_str(), c.saved))
            .collect::<Vec<_>>(),
        [("10", true), ("30", false), ("20", true)]
    );
    let receipt = l
        .export(&book, d.path().join("exports"), ExportFormat::Txt)
        .unwrap();
    assert!(receipt.partial);
    assert_eq!(receipt.saved_chapters, 2);
    let text = std::fs::read_to_string(receipt.path).unwrap();
    assert!(text.find("第一章 出发").unwrap() < text.find("第三章 归途").unwrap());
    assert!(text.contains("2/3") || text.contains("2 / 3"));
    let receipt = l
        .export(&book, d.path().join("exports"), ExportFormat::Epub)
        .unwrap();
    assert!(receipt.partial);
    assert!(std::fs::metadata(receipt.path).unwrap().len() > 0);
    assert!(
        l.snapshot()
            .unwrap()
            .jobs
            .iter()
            .all(|j| j.state == "completed" && j.saved == 1 && j.total == 1)
    );
    drop(l);
    let reopened = Library::open(path).unwrap();
    assert_eq!(
        reopened.read_chapter(&book, "20").unwrap().title,
        "第三章 归途"
    );
    assert_eq!(reopened.book_details(&book).unwrap().book.saved_chapters, 2);
}

#[test]
fn duplicates_and_conflicts_preserve_saved_body_and_completed_tasks() {
    let d = tempdir().unwrap();
    let l = Library::open(d.path().join("library.db")).unwrap();
    let book = l.register_session_catalog(&catalog()).unwrap();
    let c = chapter("10", "第一章 出发");
    l.save_session_chapter(&c).unwrap();
    assert!(l.save_session_chapter(&c).unwrap().already_saved);
    assert_eq!(l.snapshot().unwrap().jobs.len(), 1);
    let mut changed = c.clone();
    changed
        .chapter
        .paragraphs
        .push("这是官网可能出现的正文修订，但未获得覆盖原文的确认。".into());
    assert!(l.save_session_chapter(&changed).is_err());
    assert!(l.save_session_chapter(&chapter("30", "错误标题")).is_err());
    assert!(
        l.save_session_chapter(&chapter("99", "目录外章节"))
            .is_err()
    );
    assert_eq!(
        l.read_chapter(&book, "10").unwrap().paragraphs,
        c.chapter.paragraphs
    );
    assert_eq!(l.snapshot().unwrap().jobs.len(), 1);
    assert_eq!(l.book_details(&book).unwrap().book.saved_chapters, 1);
}

#[test]
fn catalog_reorder_is_refused_append_keeps_identity_and_history() {
    let d = tempdir().unwrap();
    let l = Library::open(d.path().join("library.db")).unwrap();
    let book = l.register_session_catalog(&catalog()).unwrap();
    l.save_session_chapter(&chapter("10", "第一章 出发"))
        .unwrap();
    let mut changed = catalog_value();
    changed["chapters"].as_array_mut().unwrap().swap(0, 1);
    let changed = SessionCatalog::parse(&changed.to_string(), BOOK).unwrap();
    assert!(l.register_session_catalog(&changed).is_err());
    let mut added = catalog_value();
    added["declared_total"] = json!(4);
    added["chapters"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"40","title":"第四章 新旅程"}));
    assert_eq!(
        l.register_session_catalog(&SessionCatalog::parse(&added.to_string(), BOOK).unwrap())
            .unwrap(),
        book
    );
    assert_eq!(l.book_details(&book).unwrap().book.total_chapters, 4);
    let history = l.history(&book).unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(
        l.history_details(&history[0].id)
            .unwrap()
            .book
            .total_chapters,
        3
    );
    assert_eq!(l.read_chapter(&book, "10").unwrap().title, "第一章 出发");
}

#[test]
fn database_failure_rolls_back_body_and_task_state_together() {
    let d = tempdir().unwrap();
    let path = d.path().join("library.db");
    let l = Library::open(&path).unwrap();
    let book = l.register_session_catalog(&catalog()).unwrap();
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection.execute_batch("CREATE TRIGGER simulated_failure BEFORE INSERT ON job_items BEGIN SELECT RAISE(ABORT,'controlled failure'); END;").unwrap();
    assert!(
        l.save_session_chapter(&chapter("10", "第一章 出发"))
            .is_err()
    );
    assert_eq!(l.book_details(&book).unwrap().book.saved_chapters, 0);
    assert!(l.snapshot().unwrap().jobs.is_empty());
    assert!(l.read_chapter(&book, "10").is_err());
    connection
        .execute_batch("DROP TRIGGER simulated_failure;")
        .unwrap();
    assert_eq!(
        l.save_session_chapter(&chapter("10", "第一章 出发"))
            .unwrap()
            .saved,
        1
    );
}
