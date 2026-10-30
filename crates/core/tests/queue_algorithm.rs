use opennovel_core::{
    Error, ImportChapter, Library,
    sources::{Catalog, ChapterRef, SourceAdapter},
};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
struct Original(AtomicUsize);
impl SourceAdapter for Original {
    fn source_id(&self) -> &str {
        "fanqie-web"
    }
    fn fetch_chapter(
        &self,
        _: &str,
        c: &ChapterRef,
        _: &AtomicBool,
    ) -> opennovel_core::Result<ImportChapter> {
        self.0.fetch_add(1, Ordering::Relaxed);
        Ok(ImportChapter {
            id: c.id.clone(),
            title: c.title.clone(),
            paragraphs: vec!["原创队列性能样本。".repeat(300)],
        })
    }
}
fn catalog(n: usize) -> Catalog {
    Catalog {
        source: "fanqie-web".into(),
        remote_id: "12345678901".into(),
        title: "原创千章队列".into(),
        author: "测试".into(),
        description: String::new(),
        chapters: (0..n)
            .map(|i| ChapterRef {
                id: format!("id-{i}"),
                title: format!("原创第{i}章"),
                restricted: false,
            })
            .collect(),
    }
}
#[test]
fn cached_reconciliation_is_atomic_and_never_changes_saved_bodies() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("library.db");
    let l = Library::open(&path).unwrap();
    let c = catalog(12);
    let source = Original(AtomicUsize::new(0));
    let stop = AtomicBool::new(false);
    let first = l.create_download(&c, 1, 10).unwrap();
    l.run_job(&first, &source, &stop).unwrap();
    let second = l.create_download(&c, 1, 12).unwrap();
    let db = rusqlite::Connection::open(&path).unwrap();
    // A pending job may find bodies saved by another completed task. Seed that
    // persistent state explicitly; ordinary creation already recognizes cache.
    db.execute(
        "UPDATE job_items SET state='queued' WHERE job_id=?1",
        [&second],
    )
    .unwrap();
    db.execute_batch("CREATE TRIGGER reject_cache BEFORE UPDATE ON job_items WHEN NEW.job_id!=OLD.job_id OR (NEW.state='completed' AND NEW.chapter_id='id-5') BEGIN SELECT RAISE(FAIL,'controlled cache failure'); END").unwrap();
    assert!(l.run_job(&second, &source, &stop).is_err());
    assert_eq!(source.0.load(Ordering::Relaxed), 10);
    assert!(
        l.job_details(&second)
            .unwrap()
            .items
            .iter()
            .all(|i| i.state == "queued")
    );
    db.execute_batch("DROP TRIGGER reject_cache").unwrap();
    let result = l.run_job(&second, &source, &stop).unwrap();
    assert_eq!(result.saved, 12);
    assert_eq!(source.0.load(Ordering::Relaxed), 12);
    assert_eq!(
        l.book_details(&result.book_id).unwrap().book.saved_chapters,
        12
    );
}
#[test]
fn queue_stops_on_invalid_content_and_retains_remaining_items() {
    struct Bad;
    impl SourceAdapter for Bad {
        fn source_id(&self) -> &str {
            "fanqie-web"
        }
        fn fetch_chapter(
            &self,
            _: &str,
            c: &ChapterRef,
            _: &AtomicBool,
        ) -> opennovel_core::Result<ImportChapter> {
            if c.id == "id-1" {
                return Err(Error::Network("原创暂停原因".into()));
            }
            Ok(ImportChapter {
                id: c.id.clone(),
                title: c.title.clone(),
                paragraphs: vec![" 原创正文 ".into()],
            })
        }
    }
    let d = tempfile::tempdir().unwrap();
    let l = Library::open(d.path().join("library.db")).unwrap();
    let j = l.create_download(&catalog(10), 1, 10).unwrap();
    let r = l.run_job(&j, &Bad, &AtomicBool::new(false)).unwrap();
    assert_eq!((r.saved, r.failed), (1, 1));
    assert!(
        l.job_details(&j).unwrap().items[2..]
            .iter()
            .all(|i| i.state == "queued")
    );
    assert_eq!(
        l.read_chapter(&r.book_id, "id-0").unwrap().paragraphs,
        ["原创正文"]
    );
}
#[test]
#[ignore = "explicit controlled old/new whole-run timing"]
fn whole_run_benchmark() {
    println!("compiled_manifest={}", env!("CARGO_MANIFEST_DIR"));
    for sample in 1..=3 {
        println!("sample={sample}");
        let d = tempfile::tempdir().unwrap();
        let l = Library::open(d.path().join("library.db")).unwrap();
        let c = catalog(1001);
        let source = Original(AtomicUsize::new(0));
        let stop = AtomicBool::new(false);
        let now = std::time::Instant::now();
        let first = l.create_download(&c, 1, 1000).unwrap();
        l.run_job(&first, &source, &stop).unwrap();
        let cold = now.elapsed();
        let now = std::time::Instant::now();
        let second = l.create_download(&c, 1, 1001).unwrap();
        let result = l.run_job(&second, &source, &stop).unwrap();
        let resume = now.elapsed();
        assert_eq!(result.saved, 1001);
        assert_eq!(source.0.load(Ordering::Relaxed), 1001);
        println!(
            "whole_run original 1000 chapters: cold_ms={:.3}; resume_1000_cached_plus_1_new_ms={:.3}; fetch_calls={}",
            cold.as_secs_f64() * 1000.,
            resume.as_secs_f64() * 1000.,
            source.0.load(Ordering::Relaxed)
        );
    }
}
