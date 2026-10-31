use opennovel_core::{
    Error, ImportChapter, Library,
    sources::{Catalog, ChapterRef, Jinjiang, SourceAdapter, SourceProvider, identify},
};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use tempfile::tempdir;

#[test]
fn access_challenge_pauses_entire_batch_during_inspection_or_chapter_fetch() {
    struct Gate {
        inspection: bool,
        blocked: AtomicBool,
        inspects: AtomicUsize,
        reads: AtomicUsize,
    }
    impl SourceAdapter for Gate {
        fn source_id(&self) -> &str {
            "qimao-web"
        }
        fn fetch_chapter(
            &self,
            _: &str,
            c: &ChapterRef,
            _: &AtomicBool,
        ) -> opennovel_core::Result<ImportChapter> {
            self.reads.fetch_add(1, Ordering::AcqRel);
            if self.blocked.load(Ordering::Acquire) {
                return Err(Error::AccessChallenge("原创全站验证".into()));
            }
            Ok(ImportChapter {
                id: c.id.clone(),
                title: c.title.clone(),
                paragraphs: vec!["原创正文。".into()],
            })
        }
    }
    impl SourceProvider for Gate {
        fn inspect(&self, input: &str) -> opennovel_core::Result<Catalog> {
            self.inspects.fetch_add(1, Ordering::AcqRel);
            if self.inspection && self.blocked.load(Ordering::Acquire) {
                return Err(Error::AccessChallenge("原创目录验证".into()));
            }
            let (source, id) = identify(input)?;
            Ok(Catalog {
                source: source.into(),
                remote_id: id,
                title: "原创批次验证".into(),
                author: String::new(),
                description: String::new(),
                chapters: vec![ChapterRef {
                    id: "1".into(),
                    title: "原创一章".into(),
                    restricted: false,
                }],
            })
        }
        fn adapter(&self, _: &str) -> opennovel_core::Result<&dyn SourceAdapter> {
            Ok(self)
        }
    }
    for inspection in [true, false] {
        let dir = tempdir().unwrap();
        let library = Library::open(dir.path().join("library.db")).unwrap();
        let id = library
            .create_batch(&["qimao:1".into(), "qimao:2".into()], "1")
            .unwrap();
        let gate = Gate {
            inspection,
            blocked: AtomicBool::new(true),
            inspects: AtomicUsize::new(0),
            reads: AtomicUsize::new(0),
        };
        let stopped = library
            .run_batch(&id, &gate, &AtomicBool::new(false))
            .unwrap();
        assert_eq!(stopped.state, "paused");
        assert_eq!(gate.inspects.load(Ordering::Acquire), 1);
        assert!(stopped.items[1].job_id.is_none());
        let linked = stopped.items[0].job_id.clone();
        gate.blocked.store(false, Ordering::Release);
        let done = library
            .run_batch(&id, &gate, &AtomicBool::new(false))
            .unwrap();
        assert_eq!(done.state, "completed");
        if let Some(linked) = linked {
            assert_eq!(done.items[0].job_id.as_deref(), Some(linked.as_str()));
        }
        assert_eq!(library.snapshot().unwrap().jobs.len(), 2);
    }
}
struct Provider<'a> {
    library: &'a Library,
    batch: &'a str,
    pause_on: usize,
    calls: AtomicUsize,
    network: AtomicBool,
    inspects: AtomicUsize,
}
impl SourceAdapter for Provider<'_> {
    fn source_id(&self) -> &str {
        "qimao-web"
    }
    fn fetch_chapter(
        &self,
        _: &str,
        c: &ChapterRef,
        stop: &AtomicBool,
    ) -> opennovel_core::Result<ImportChapter> {
        let n = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
        if n == self.pause_on {
            self.library.stop_batch(self.batch, true, Some(stop))?;
        }
        Ok(ImportChapter {
            id: c.id.clone(),
            title: c.title.clone(),
            paragraphs: vec!["原创批次正文。".into()],
        })
    }
}
impl SourceProvider for Provider<'_> {
    fn inspect(&self, input: &str) -> opennovel_core::Result<Catalog> {
        self.inspects.fetch_add(1, Ordering::SeqCst);
        if self.network.load(Ordering::SeqCst) {
            return Err(Error::Network("受控限流".into()));
        }
        let (source, id) = identify(input)?;
        Ok(Catalog {
            source: source.into(),
            remote_id: id,
            title: "原创批次".into(),
            author: "OpenNovel".into(),
            description: String::new(),
            chapters: (1..=3)
                .map(|n| ChapterRef {
                    id: n.to_string(),
                    title: format!("原创第{n}章"),
                    restricted: false,
                })
                .collect(),
        })
    }
    fn adapter(&self, _: &str) -> opennovel_core::Result<&dyn SourceAdapter> {
        Ok(self)
    }
}
#[test]
fn batch_bounds_identity_dedup_and_capability_are_checked_before_persistence() {
    let d = tempdir().unwrap();
    let l = Library::open(d.path().join("library.db")).unwrap();
    for values in [
        vec![],
        vec!["qimao:1".into(); 11],
        vec!["shuqi:1".into()],
        vec!["https://user:secret@www.qimao.com/shuku/1/".into()],
    ] {
        assert!(l.create_batch(&values, "1").is_err());
    }
    assert!(l.batches().unwrap().is_empty());
    assert!(l.create_batch(&["qimao:1".into()], "0").is_err());
    let id = l
        .create_batch(
            &[
                "qimao:1".into(),
                "https://www.qimao.com/shuku/1/".into(),
                "qimao:2".into(),
            ],
            "1,3",
        )
        .unwrap();
    let b = l.batch(&id).unwrap();
    assert_eq!((b.items.len(), b.duplicates), (2, 1));
    assert!(b.items.iter().all(|i| i.job_id.is_none()));
}
#[test]
fn pause_mid_book_restart_resumes_original_link_without_duplicate_jobs() {
    let d = tempdir().unwrap();
    let path = d.path().join("library.db");
    let l = Library::open(&path).unwrap();
    let id = l
        .create_batch(&["qimao:1".into(), "qimao:2".into()], "1-3")
        .unwrap();
    let p = Provider {
        library: &l,
        batch: &id,
        pause_on: 2,
        calls: AtomicUsize::new(0),
        network: AtomicBool::new(false),
        inspects: AtomicUsize::new(0),
    };
    let b = l.run_batch(&id, &p, &AtomicBool::new(false)).unwrap();
    assert_eq!(b.state, "paused");
    let linked = b.items[0].job_id.clone().unwrap();
    assert_eq!(l.job(&linked).unwrap().saved, 1);
    assert!(l.run_job(&linked, &p, &AtomicBool::new(false)).is_err());
    assert!(l.pause_job(&linked).is_err());
    assert_eq!(l.job(&linked).unwrap().state, "paused");
    assert!(b.items[1].job_id.is_none());
    drop(l);
    let l = Library::open(&path).unwrap();
    let p = Provider {
        library: &l,
        batch: &id,
        pause_on: usize::MAX,
        calls: AtomicUsize::new(0),
        network: AtomicBool::new(false),
        inspects: AtomicUsize::new(0),
    };
    let b = l.run_batch(&id, &p, &AtomicBool::new(false)).unwrap();
    assert_eq!(b.state, "completed");
    assert_eq!(b.items[0].job_id.as_deref(), Some(linked.as_str()));
    assert_eq!(l.snapshot().unwrap().jobs.len(), 2);
    assert_eq!(p.calls.load(Ordering::SeqCst), 5);
    assert_eq!(p.inspects.load(Ordering::SeqCst), 1);
}
#[test]
fn network_during_inspection_pauses_queue_without_scheduling_following_books() {
    let d = tempdir().unwrap();
    let l = Library::open(d.path().join("library.db")).unwrap();
    let id = l
        .create_batch(&["qimao:1".into(), "qimao:2".into()], "1")
        .unwrap();
    let p = Provider {
        library: &l,
        batch: &id,
        pause_on: usize::MAX,
        calls: AtomicUsize::new(0),
        network: AtomicBool::new(true),
        inspects: AtomicUsize::new(0),
    };
    assert_eq!(
        l.run_batch(&id, &p, &AtomicBool::new(false)).unwrap().state,
        "paused"
    );
    assert_eq!(p.inspects.load(Ordering::SeqCst), 1);
    assert!(l.snapshot().unwrap().jobs.is_empty());
    p.network.store(false, Ordering::SeqCst);
    assert_eq!(
        l.run_batch(&id, &p, &AtomicBool::new(false)).unwrap().state,
        "completed"
    );
}
#[test]
fn first_page_search_is_scoped_deduplicated_and_rejects_wrong_sources() {
    let html = r#"<title>搜索原创</title><div id="search_result"><div><h3 class="title"><a href="http://www.jjwxc.net/onebook.php?novelid=123">原创书名</a></h3><div class="info"><a href="http://www.jjwxc.net/oneauthor.php?authorid=1">原创作者</a></div></div><div><h3 class="title"><a href="http://www.jjwxc.net/onebook.php?novelid=123">重复</a></h3></div></div><h3 class="title"><a href="https://evil.test/">广告</a></h3>"#;
    let r = Jinjiang::parse_search(html, "原创").unwrap();
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].author, "原创作者");
    assert_eq!(r[0].input, "jinjiang:123");
    assert!(
        Jinjiang::parse_search(
            &html.replace("www.jjwxc.net/onebook.php", "evil.test/onebook.php"),
            "原创"
        )
        .is_err()
    );
    assert!(Jinjiang::parse_search(html, "其他").is_err());
}
