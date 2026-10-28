use opennovel_core::{
    Error, ImportChapter, Library,
    sources::{Catalog, ChapterRef, SourceAdapter},
};
use std::sync::atomic::AtomicBool;
use tempfile::tempdir;

fn catalog(id: &str) -> Catalog {
    Catalog {
        source: "fanqie-web".into(),
        remote_id: id.into(),
        title: "原创范围验证".into(),
        author: "原创".into(),
        description: String::new(),
        chapters: (1..=5)
            .map(|i| ChapterRef {
                id: format!("chapter-{i}"),
                title: format!("原创第{i}章"),
                restricted: i == 3,
            })
            .collect(),
    }
}
struct Fake {
    fail: bool,
}
impl SourceAdapter for Fake {
    fn source_id(&self) -> &str {
        "fanqie-web"
    }
    fn fetch_chapter(
        &self,
        _: &str,
        c: &ChapterRef,
        _: &AtomicBool,
    ) -> opennovel_core::Result<ImportChapter> {
        if self.fail && c.id == "chapter-2" {
            return Err(Error::Network("原创连接失败，不能推断需要订阅".into()));
        }
        Ok(ImportChapter {
            id: c.id.clone(),
            title: c.title.clone(),
            paragraphs: vec!["仅供受控测试的原创正文。".into()],
        })
    }
}
fn states(l: &Library, id: &str) -> Vec<String> {
    l.book_details(id)
        .unwrap()
        .chapters
        .iter()
        .map(|c| c.availability.as_ref().unwrap().state.clone())
        .collect()
}

#[test]
fn whole_catalog_reasons_keep_out_of_range_and_restriction_evidence() {
    let d = tempdir().unwrap();
    let l = Library::open(d.path().join("test.db")).unwrap();
    let c = catalog("first");
    let job = l.create_download_selected(&c, &[1, 3]).unwrap();
    let book = l.job(&job).unwrap().book_id;
    l.run_job(&job, &Fake { fail: false }, &AtomicBool::new(false))
        .unwrap();
    assert_eq!(
        states(&l, &book),
        [
            "saved",
            "unattempted",
            "restricted",
            "unattempted",
            "unattempted"
        ]
    );
    let v = l.book_details(&book).unwrap();
    assert_eq!(v.chapters.len(), 5);
    assert_eq!(v.book.saved_chapters, 1);
    let serialized = serde_json::to_value(v).unwrap();
    assert_eq!(
        serialized["chapters"][2]["availability"]["state"],
        "restricted"
    );
}

#[test]
fn latest_evidence_is_per_chapter_and_new_narrow_job_does_not_erase_errors() {
    let d = tempdir().unwrap();
    let path = d.path().join("test.db");
    let l = Library::open(&path).unwrap();
    let c = catalog("first");
    let old = l.create_download(&c, 1, 5).unwrap();
    let book = l.job(&old).unwrap().book_id;
    l.run_job(&old, &Fake { fail: true }, &AtomicBool::new(false))
        .unwrap();
    let new = l.create_download_selected(&c, &[4]).unwrap();
    l.run_job(&new, &Fake { fail: false }, &AtomicBool::new(false))
        .unwrap();
    let unrelated = l.create_download(&catalog("other"), 1, 5).unwrap();
    l.run_job(&unrelated, &Fake { fail: false }, &AtomicBool::new(false))
        .unwrap();
    assert_eq!(
        states(&l, &book),
        ["saved", "failed", "restricted", "saved", "unattempted"]
    );
    let v = l.book_details(&book).unwrap();
    let evidence = v.chapters[1].availability.as_ref().unwrap();
    assert_eq!(evidence.job_id.as_deref(), Some(old.as_str()));
    assert!(evidence.reason.contains("原创连接失败"));
    drop(l);
    let l = Library::open(&path).unwrap();
    assert_eq!(states(&l, &book)[1], "failed");
    let recovery = l.create_download_selected(&c, &[2]).unwrap();
    l.run_job(&recovery, &Fake { fail: false }, &AtomicBool::new(false))
        .unwrap();
    assert_eq!(states(&l, &book)[1], "saved");
}

#[test]
fn queued_paused_cancelled_are_not_claimed_as_unreadable() {
    let d = tempdir().unwrap();
    let l = Library::open(d.path().join("test.db")).unwrap();
    let c = catalog("first");
    let job = l.create_download_selected(&c, &[1, 2]).unwrap();
    let book = l.job(&job).unwrap().book_id;
    assert_eq!(states(&l, &book)[0], "pending");
    l.pause_job(&job).unwrap();
    assert_eq!(states(&l, &book)[0], "paused");
    l.cancel_job(&job).unwrap();
    assert_eq!(states(&l, &book)[0], "cancelled");
}

#[test]
fn local_import_reports_pending_then_saved_without_remote_requests() {
    let d = tempdir().unwrap();
    let l = Library::open(d.path().join("test.db")).unwrap();
    let job = l
        .create_import(include_str!("../../../examples/local-book.json"))
        .unwrap();
    let book = l.job(&job).unwrap().book_id;
    assert_eq!(states(&l, &book), ["pending", "pending"]);
    l.run_import(&job, &AtomicBool::new(false)).unwrap();
    assert_eq!(states(&l, &book), ["saved", "saved"]);
}

#[test]
fn account_catalog_without_tasks_never_proves_login_or_free_access() {
    let d = tempdir().unwrap();
    let l = Library::open(d.path().join("test.db")).unwrap();
    let url = "https://www.qidian.com/book/123/catalog/";
    let raw = serde_json::json!({"url":url,"title":"原创账户目录","author":"测试", "declared_total":1,"chapters":[{"id":"10","title":"原创首章"}]}).to_string();
    let catalog = opennovel_core::SessionCatalog::parse(&raw, url).unwrap();
    let book = l.register_session_catalog(&catalog).unwrap();
    let details = l.book_details(&book).unwrap();
    let evidence = details.chapters[0].availability.as_ref().unwrap();
    assert_eq!(evidence.state, "unattempted");
    assert!(evidence.reason.contains("核对登录"));
    assert!(evidence.job_id.is_none());
}

#[test]
fn saved_content_wins_over_a_new_restriction_marker() {
    let d = tempdir().unwrap();
    let l = Library::open(d.path().join("test.db")).unwrap();
    let mut c = catalog("first");
    let job = l.create_download_selected(&c, &[1]).unwrap();
    let book = l.job(&job).unwrap().book_id;
    l.run_job(&job, &Fake { fail: false }, &AtomicBool::new(false))
        .unwrap();
    c.chapters[0].restricted = true;
    l.create_download_selected(&c, &[4]).unwrap();
    assert_eq!(states(&l, &book)[0], "saved");
}
