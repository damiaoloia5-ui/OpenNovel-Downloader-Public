use opennovel_core::{
    Error, ExportFormat, Library, Result,
    sources::{Catalog, ChapterRef, LOCAL_ENGINE_SOURCE, SourceAdapter, parse_engine_chapter},
};
use std::{
    path::Path,
    sync::{Mutex, atomic::AtomicBool},
};

fn catalog() -> Catalog {
    Catalog {
        source: LOCAL_ENGINE_SOURCE.into(),
        remote_id: "10000000001".into(),
        title: "原创队列".into(),
        author: String::new(),
        description: String::new(),
        chapters: (2..=4)
            .map(|n| ChapterRef {
                id: format!("1000000000{n}"),
                title: "原创章".into(),
                restricted: false,
            })
            .collect(),
    }
}
struct Adapter {
    stop: Option<usize>,
    calls: Mutex<Vec<String>>,
}
impl SourceAdapter for Adapter {
    fn source_id(&self) -> &str {
        LOCAL_ENGINE_SOURCE
    }
    fn fetch_chapter(
        &self,
        book: &str,
        c: &ChapterRef,
        _: &AtomicBool,
    ) -> Result<opennovel_core::ImportChapter> {
        let mut calls = self.calls.lock().unwrap();
        calls.push(c.id.clone());
        if self.stop == Some(calls.len()) {
            return Err(Error::Cancelled);
        }
        parse_engine_chapter(&serde_json::json!({"schema_version":1,"book_id":book,"item_id":c.id,"title":c.title,"word_number":11,"need_pay":0,"content":"<p>山间的小屋亮起一盏灯。</p>"}).to_string(),book,&c.id,&c.title)
    }
}
fn adapter(stop: Option<usize>) -> Adapter {
    Adapter {
        stop,
        calls: Mutex::new(vec![]),
    }
}

#[test]
fn explicit_engine_authorization_does_not_enable_generic_or_other_sources() {
    let dir = tempfile::tempdir().unwrap();
    let lib = Library::open(dir.path().join("library.db")).unwrap();
    assert!(!opennovel_core::sources::download_supported(
        LOCAL_ENGINE_SOURCE
    ));
    assert!(lib.create_download(&catalog(), 1, 3).is_err());
    let mut other = catalog();
    other.source = "qidian-session".into();
    assert!(lib.create_engine_download(&other, &[1]).is_err());
    assert!(lib.snapshot().unwrap().books.is_empty());
    assert!(lib.create_engine_download(&catalog(), &[1, 2, 3]).is_ok());
}

#[test]
fn engine_resume_reuses_same_book_and_never_refetches_committed_chapter() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.db");
    let lib = Library::open(&path).unwrap();
    let job = lib.create_engine_download(&catalog(), &[1, 2, 3]).unwrap();
    let first = lib
        .run_job(&job, &adapter(Some(2)), &AtomicBool::new(false))
        .unwrap();
    assert_eq!(first.saved, 1);
    assert_eq!(first.state, "cancelled");
    let partial = lib
        .export(
            &first.book_id,
            dir.path().join("partial"),
            ExportFormat::Txt,
        )
        .unwrap();
    assert!(partial.partial);
    drop(lib);
    let lib = Library::open(&path).unwrap();
    let resumed = adapter(None);
    let finished = lib
        .run_job(&job, &resumed, &AtomicBool::new(false))
        .unwrap();
    assert_eq!(finished.saved, 3);
    assert_eq!(finished.book_id, first.book_id);
    assert_eq!(
        *resumed.calls.lock().unwrap(),
        ["10000000003", "10000000004"]
    );
    let full = lib
        .export(
            &finished.book_id,
            dir.path().join("full"),
            ExportFormat::Txt,
        )
        .unwrap();
    assert!(!full.partial);
    assert!(Path::new(&full.path).is_file());
}

#[test]
fn catalog_restricted_items_remain_missing_and_are_not_requested() {
    let dir = tempfile::tempdir().unwrap();
    let lib = Library::open(dir.path().join("library.db")).unwrap();
    let mut cat = catalog();
    cat.chapters[1].restricted = true;
    let job = lib.create_engine_download(&cat, &[1, 2, 3]).unwrap();
    let source = adapter(None);
    let result = lib.run_job(&job, &source, &AtomicBool::new(false)).unwrap();
    assert_eq!(result.saved, 2);
    assert_eq!(result.blocked, 1);
    assert_eq!(
        *source.calls.lock().unwrap(),
        ["10000000002", "10000000004"]
    );
}
