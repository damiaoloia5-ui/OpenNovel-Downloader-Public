use opennovel_core::{
    Error, ExportFormat, ImportChapter, Library, SessionCatalog, SessionChapter, account_entry_url,
    clean_session_url, session_chapter_url, session_source,
    sources::{Catalog, ChapterRef, SourceAdapter},
};
use serde_json::json;
use std::sync::{Mutex, atomic::AtomicBool};
use tempfile::tempdir;
const PLATFORMS: [&str; 7] = [
    "zongheng", "17k", "jinjiang", "qimao", "shuqi", "ciweimao", "faloo",
];
fn catalog(p: &str) -> SessionCatalog {
    SessionCatalog::from_public(
        Catalog {
            source: format!("{p}-web"),
            remote_id: "123".into(),
            title: "原创山路".into(),
            author: "原创作者".into(),
            description: "原创简介".into(),
            chapters: (1..=3)
                .map(|i| ChapterRef {
                    id: i.to_string(),
                    title: format!("第{i}章"),
                    restricted: true,
                })
                .collect(),
        },
        p,
    )
    .unwrap()
}
struct Reader {
    source: String,
    block: bool,
    calls: Mutex<Vec<String>>,
}
impl SourceAdapter for Reader {
    fn source_id(&self) -> &str {
        &self.source
    }
    fn fetch_chapter(
        &self,
        _: &str,
        ch: &ChapterRef,
        _: &AtomicBool,
    ) -> opennovel_core::Result<ImportChapter> {
        self.calls.lock().unwrap().push(ch.id.clone());
        if self.block && ch.id == "2" {
            return Err(Error::Restricted("受控账号缺少本章权益".into()));
        }
        Ok(ImportChapter {
            id: ch.id.clone(),
            title: ch.title.clone(),
            paragraphs: vec!["旅人沿山路来到小屋，等候夜雨停下。".into()],
        })
    }
}
#[test]
fn all_session_sources_stop_on_denial_preserve_progress_and_resume_without_cross_source_access() {
    for p in PLATFORMS {
        let d = tempdir().unwrap();
        let path = d.path().join("library.db");
        let l = Library::open(&path).unwrap();
        let c = catalog(p);
        assert!(l.create_download(c.catalog(), 1, 3).is_err());
        let book = l.register_session_catalog(&c).unwrap();
        let job = l.create_session_download(&book, None).unwrap();
        assert!(l.is_session_download(&job).unwrap());
        let wrong = Reader {
            source: "qidian-session".into(),
            block: false,
            calls: Mutex::new(vec![]),
        };
        assert!(l.run_job(&job, &wrong, &AtomicBool::new(false)).is_err());
        assert!(wrong.calls.lock().unwrap().is_empty());
        let r = Reader {
            source: session_source(p).unwrap().into(),
            block: true,
            calls: Mutex::new(vec![]),
        };
        let result = l.run_job(&job, &r, &AtomicBool::new(false)).unwrap();
        assert_eq!(result.saved, 1);
        assert_eq!(result.blocked, 1);
        assert_eq!(*r.calls.lock().unwrap(), ["1", "2"]);
        drop(l);
        let l = Library::open(&path).unwrap();
        let r = Reader { block: false, ..r };
        let done = l.run_job(&job, &r, &AtomicBool::new(false)).unwrap();
        assert_eq!(done.saved, 3);
        assert_eq!(done.state, "completed");
        assert_eq!(*r.calls.lock().unwrap(), ["1", "2", "2", "3"]);
        assert!(
            !l.export(&book, d.path().join("exports"), ExportFormat::Txt)
                .unwrap()
                .partial
        );
    }
}
#[test]
fn routes_bind_platform_chapter_and_identity_queries_and_reject_truncated_pages() {
    for p in PLATFORMS {
        let url = session_chapter_url(p, "123", "2").unwrap();
        let value = json!({"url":url,"title":"原创章","paragraphs":["山路原创正文".repeat(20)],"declared_words":100});
        assert!(
            SessionChapter::parse_for(&value.to_string(), &url, p, "123", "2").is_ok(),
            "{p}"
        );
        assert!(SessionChapter::parse_for(&value.to_string(), &url, p, "123", "3").is_err());
        let mut short = value.clone();
        short["paragraphs"] = json!(["短试读"]);
        assert!(SessionChapter::parse_for(&short.to_string(), &url, p, "123", "2").is_err());
        assert!(clean_session_url(p, "https://example.invalid/").is_err());
    }
    assert_eq!(
        clean_session_url(
            "shuqi",
            "https://www.shuqi.com/reader?cid=2&token=discarded&bid=123#fragment"
        )
        .unwrap(),
        "https://www.shuqi.com/reader?bid=123&cid=2"
    );
    assert!(clean_session_url("shuqi", "https://www.shuqi.com/reader?bid=1&bid=2&cid=3").is_err());
    assert!(session_source("made-up").is_err());
    assert!(session_source("migu").is_err());
    let free = "https://www.jjwxc.net/onebook.php?novelid=123&chapterid=2";
    let value = json!({"url":free,"title":"免费章","paragraphs":["山路原创正文".repeat(20)],"declared_words":100});
    assert!(SessionChapter::parse_for(&value.to_string(), free, "jinjiang", "123", "2").is_ok());
    assert!(
        account_entry_url(
            "jinjiang",
            "https://www.jjwxc.net/onebook.php?novelid=123&token=secret"
        )
        .is_err()
    );
    assert!(account_entry_url("zongheng", "https://www.17k.com/book/123.html").is_err());
}
#[test]
fn confirmed_catalog_does_not_mix_sources_or_silently_reorder_saved_chapters() {
    let d = tempdir().unwrap();
    let l = Library::open(d.path().join("library.db")).unwrap();
    let c = catalog("shuqi");
    let book = l.register_session_catalog(&c).unwrap();
    let job = l.create_session_download(&book, None).unwrap();
    l.cancel_job(&job).unwrap();
    let mut changed = c.catalog().clone();
    changed.source = "shuqi-web".into();
    changed.chapters.swap(0, 1);
    let changed = SessionCatalog::from_public(changed, "shuqi").unwrap();
    assert!(l.register_session_catalog(&changed).is_err());
    assert!(SessionCatalog::from_public(c.catalog().clone(), "faloo").is_err());
    assert_eq!(l.session_catalog(&book).unwrap().chapters[0].id, "1");
}
#[test]
fn faloo_end_navigation_is_scoped_and_does_not_fabricate_a_word_count() {
    use opennovel_core::SessionChapter;
    let url = "https://b.faloo.com/1234567_2.html";
    let mut raw = serde_json::json!({"url":url,"title":"原创第二章","paragraphs":["山谷的灯亮了，旅人走向小屋。"],"declared_words":0,"visible_page_end":{"catalog_url":"https://b.faloo.com/html_1234_1234567/","next_url":"https://b.faloo.com/1234567_4.html"}});
    let page = SessionChapter::parse_for(&raw.to_string(), url, "faloo", "1234567", "2").unwrap();
    assert_eq!(page.declared_words, 0);
    for next in [
        "https://evil.test/1234567_4.html",
        "https://b.faloo.com/7654321_4.html",
        "https://b.faloo.com/1234567_2.html",
        "https://b.faloo.com/1234567_4.html?token=no",
        "https://u:p@b.faloo.com/1234567_4.html",
    ] {
        raw["visible_page_end"]["next_url"] = serde_json::json!(next);
        assert!(SessionChapter::parse_for(&raw.to_string(), url, "faloo", "1234567", "2").is_err());
    }
    raw["visible_page_end"]["next_url"] = serde_json::json!("https://b.faloo.com/1234567_4.html");
    raw["visible_page_end"]["catalog_url"] =
        serde_json::json!("https://b.faloo.com/html_7654_7654321/");
    assert!(SessionChapter::parse_for(&raw.to_string(), url, "faloo", "1234567", "2").is_err());
    raw.as_object_mut().unwrap().remove("visible_page_end");
    assert!(SessionChapter::parse_for(&raw.to_string(), url, "faloo", "1234567", "2").is_err());
    raw["url"] = serde_json::json!("https://www.qidian.com/chapter/1234567/2/");
    raw["visible_page_end"] = serde_json::json!({"catalog_url":"https://b.faloo.com/html_1234_1234567/","next_url":"https://b.faloo.com/1234567_4.html"});
    assert!(
        SessionChapter::parse(
            &raw.to_string(),
            "https://www.qidian.com/chapter/1234567/2/"
        )
        .is_err()
    );
}
