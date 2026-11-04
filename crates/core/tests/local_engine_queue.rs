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

#[test]
fn prefetch_plan_excludes_saved_restricted_and_unselected_chapters() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.db");
    let lib = Library::open(&path).unwrap();
    let job = lib.create_engine_download(&catalog(), &[1, 2, 3]).unwrap();
    lib.run_job(&job, &adapter(Some(2)), &AtomicBool::new(false))
        .unwrap();
    assert_eq!(
        lib.pending_download_ids(&job, None).unwrap(),
        ["10000000003", "10000000004"]
    );
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute(
        "UPDATE job_items SET state='failed' WHERE job_id=?1 AND chapter_id='10000000003'",
        [&job],
    )
    .unwrap();
    let selected = vec!["10000000003".to_string()];
    assert_eq!(
        lib.pending_download_ids(&job, Some(&selected)).unwrap(),
        selected
    );
    for invalid in [
        vec![],
        vec!["10000000003".into(), "10000000003".into()],
        vec!["10000000004".into()],
    ] {
        assert!(lib.pending_download_ids(&job, Some(&invalid)).is_err());
    }
    // Another task may already have saved a body while this old item is queued.
    db.execute(
        "UPDATE job_items SET state='queued' WHERE job_id=?1 AND chapter_id='10000000002'",
        [&job],
    )
    .unwrap();
    db.execute("UPDATE job_items SET payload=json_set(payload,'$.restricted',json('true')) WHERE job_id=?1 AND chapter_id='10000000004'", [&job]).unwrap();
    assert_eq!(
        lib.pending_download_ids(&job, None).unwrap(),
        ["10000000003"]
    );
    assert!(lib.pending_download_ids("missing", None).is_err());
}

#[test]
fn preparation_failure_is_persisted_without_overriding_a_user_pause() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.db");
    let lib = Library::open(&path).unwrap();
    let job = lib.create_engine_download(&catalog(), &[1, 2, 3]).unwrap();
    lib.record_worker_failure(&job, "原创组件冷却中").unwrap();
    assert_eq!(lib.job(&job).unwrap().state, "failed");
    assert_eq!(lib.pending_download_ids(&job, None).unwrap().len(), 3);
    let mut other = catalog();
    other.remote_id = "10000000009".into();
    let paused = lib.create_engine_download(&other, &[1]).unwrap();
    lib.pause_job(&paused).unwrap();
    lib.record_worker_failure(&paused, "迟到的准备失败")
        .unwrap();
    assert_eq!(lib.job(&paused).unwrap().state, "paused");
    drop(lib);
    let lib = Library::open(&path).unwrap();
    let resumed = lib
        .run_job(&job, &adapter(None), &AtomicBool::new(false))
        .unwrap();
    assert_eq!(resumed.saved, 3);
    assert_eq!(resumed.state, "completed");
}

#[test]
#[ignore = "explicit offline prefetch planning benchmark"]
fn prefetch_planning_benchmark() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.db");
    let lib = Library::open(&path).unwrap();
    let mut cat = catalog();
    cat.chapters = (0..10000)
        .map(|n| ChapterRef {
            id: (10000000002_u64 + n).to_string(),
            title: "原创基准章".into(),
            restricted: false,
        })
        .collect();
    let job = lib
        .create_engine_download(&cat, &(1..=10000).collect::<Vec<_>>())
        .unwrap();
    let book = lib.job(&job).unwrap().book_id;
    let mut db = rusqlite::Connection::open(&path).unwrap();
    let tx = db.transaction().unwrap();
    for (i, chapter) in cat.chapters.iter().take(9000).enumerate() {
        tx.execute("INSERT INTO chapters(book_id,chapter_id,position,title,paragraphs) VALUES(?1,?2,?3,'原创基准章','[\"原创性能文字\"]')",rusqlite::params![book,chapter.id,i]).unwrap();
    }
    tx.execute(
        "UPDATE job_items SET state='failed' WHERE job_id=?1 AND position>=9000 AND position<9005",
        [&job],
    )
    .unwrap();
    tx.commit().unwrap();
    let selection = cat.chapters[9000..9005]
        .iter()
        .map(|c| c.id.clone())
        .collect::<Vec<_>>();
    for sample in 1..=5 {
        let now = std::time::Instant::now();
        for _ in 0..20 {
            std::hint::black_box(lib.book_details(&book).unwrap());
            let old = lib
                .job_details(&job)
                .unwrap()
                .items
                .into_iter()
                .map(|i| i.chapter_id)
                .collect::<Vec<_>>();
            assert_eq!(old.len(), 10000);
        }
        let old = now.elapsed().as_secs_f64() * 1000.;
        let now = std::time::Instant::now();
        for _ in 0..20 {
            std::hint::black_box(lib.book_summary(&book).unwrap());
            assert_eq!(
                lib.pending_download_ids(&job, Some(&selection)).unwrap(),
                selection
            );
        }
        println!(
            "sample={sample} old_plan_ms={old:.3} selected_plan_ms={:.3} old_authorized_ids=10000 new_authorized_ids=5 iterations=20",
            now.elapsed().as_secs_f64() * 1000.
        );
    }
}
