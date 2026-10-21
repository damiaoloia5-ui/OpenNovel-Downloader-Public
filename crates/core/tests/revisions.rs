use opennovel_core::{
    Error, ExportFormat, ImportChapter, Library,
    sources::{Catalog, ChapterRef, SourceAdapter},
};
use std::sync::atomic::{AtomicBool, Ordering};
use tempfile::tempdir;
struct Original {
    revision: &'static str,
    fail: bool,
    cancel: bool,
}
impl SourceAdapter for Original {
    fn source_id(&self) -> &str {
        "qimao-web"
    }
    fn fetch_chapter(
        &self,
        _: &str,
        c: &ChapterRef,
        stop: &AtomicBool,
    ) -> opennovel_core::Result<ImportChapter> {
        if self.fail {
            return Err(Error::Network("原创受控失败".into()));
        }
        if self.cancel {
            stop.store(true, Ordering::Release);
        }
        Ok(ImportChapter {
            id: c.id.clone(),
            title: c.title.clone(),
            paragraphs: vec![format!("原创正文 {} {}", self.revision, c.id)],
        })
    }
}
fn catalog() -> Catalog {
    Catalog {
        source: "qimao-web".into(),
        remote_id: "123".into(),
        title: "原创修订作品".into(),
        author: "OpenNovel".into(),
        description: "原创简介".into(),
        chapters: (1..=3)
            .map(|i| ChapterRef {
                id: i.to_string(),
                title: format!("原章节{i}"),
                restricted: false,
            })
            .collect(),
    }
}
#[test]
fn reviewed_reorder_removal_and_insertion_keep_old_body_and_job_exports() {
    let d = tempdir().unwrap();
    let l = Library::open(d.path().join("library.db")).unwrap();
    let old = catalog();
    let j = l.create_download(&old, 1, 3).unwrap();
    let original = Original {
        revision: "v1",
        fail: false,
        cancel: false,
    };
    let s = l.run_job(&j, &original, &AtomicBool::new(false)).unwrap();
    let book = s.book_id;
    l.organize(&book, &["用户标签".into()], true).unwrap();
    let before = l.export_job(&j, d.path(), ExportFormat::Epub).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(1100));
    let mut changed = old.clone();
    changed.title = "更新后的原创书名".into();
    changed.chapters = vec![
        old.chapters[2].clone(),
        old.chapters[0].clone(),
        ChapterRef {
            id: "4".into(),
            title: "插入新章".into(),
            restricted: false,
        },
    ];
    changed.chapters[0].title = "重命名第3章".into();
    let diff = l.preview_catalog_update(&book, &changed).unwrap();
    assert!(diff.changed);
    assert_eq!(diff.metadata[0].field, "title");
    assert_eq!(diff.changes.len(), 4);
    assert!(l.accept_catalog_update(&book, &changed, "stale").is_err());
    assert!(l.history(&book).unwrap().is_empty());
    let snapshot = l
        .accept_catalog_update(&book, &changed, &diff.previous_hash)
        .unwrap();
    assert!(l.read_chapter(&book, "2").is_err());
    assert_eq!(
        l.read_history(&snapshot, "2").unwrap().paragraphs,
        ["原创正文 v1 2"]
    );
    assert!(l.preferences().unwrap()[0].archived);
    let details = l.book_details(&book).unwrap();
    assert_eq!(
        (details.book.saved_chapters, details.book.total_chapters),
        (2, 3)
    );
    assert_eq!(details.chapters[0].id, "3");
    assert_eq!(l.job_details(&j).unwrap().items[2].title, "原章节3");
    let dir = d.path().join("again");
    let after = l.export_job(&j, &dir, ExportFormat::Epub).unwrap();
    assert_eq!(
        std::fs::read(before.path).unwrap(),
        std::fs::read(after.path).unwrap()
    );
    let history = l
        .export_history(&snapshot, &dir, ExportFormat::Txt)
        .unwrap();
    assert!(
        std::fs::read_to_string(history.path)
            .unwrap()
            .contains("原创正文 v1 2")
    );
    let history_epub = l
        .export_history(&snapshot, &dir, ExportFormat::Epub)
        .unwrap();
    let mut zip = zip::ZipArchive::new(std::fs::File::open(history_epub.path).unwrap()).unwrap();
    let mut opf = String::new();
    use std::io::Read;
    zip.by_name("EPUB/package.opf")
        .unwrap()
        .read_to_string(&mut opf)
        .unwrap();
    let xml = roxmltree::Document::parse(&opf).unwrap();
    let identifier = xml
        .descendants()
        .find(|n| n.tag_name().name() == "identifier")
        .unwrap()
        .text()
        .unwrap();
    assert!(uuid::Uuid::parse_str(identifier.strip_prefix("urn:uuid:").unwrap()).is_ok());
    assert!(!l.preview_catalog_update(&book, &changed).unwrap().changed);
    assert!(
        l.accept_catalog_update(&book, &changed, &diff.previous_hash)
            .is_err()
    );
}
#[test]
fn chapter_revision_failures_and_cancellation_keep_old_text_and_revised_exports_are_distinct() {
    let d = tempdir().unwrap();
    let l = Library::open(d.path().join("library.db")).unwrap();
    let c = catalog();
    let j = l.create_download(&c, 1, 3).unwrap();
    let original = Original {
        revision: "v1",
        fail: false,
        cancel: false,
    };
    let book = l
        .run_job(&j, &original, &AtomicBool::new(false))
        .unwrap()
        .book_id;
    let before = l.export(&book, d.path(), ExportFormat::Txt).unwrap();
    assert!(
        !l.refresh_chapter(&book, "1", &original, &AtomicBool::new(false))
            .unwrap()
            .changed
    );
    for adapter in [
        Original {
            revision: "v2",
            fail: true,
            cancel: false,
        },
        Original {
            revision: "v2",
            fail: false,
            cancel: true,
        },
    ] {
        assert!(
            l.refresh_chapter(&book, "1", &adapter, &AtomicBool::new(false))
                .is_err()
        );
        assert!(l.history(&book).unwrap().is_empty());
        assert_eq!(
            l.read_chapter(&book, "1").unwrap().paragraphs,
            ["原创正文 v1 1"]
        );
    }
    let revised = Original {
        revision: "v2",
        fail: false,
        cancel: false,
    };
    let r = l
        .refresh_chapter(&book, "1", &revised, &AtomicBool::new(false))
        .unwrap();
    assert!(r.changed);
    let id = r.snapshot_id.unwrap();
    assert_eq!(
        l.read_history(&id, "1").unwrap().paragraphs,
        ["原创正文 v1 1"]
    );
    assert_eq!(
        l.read_chapter(&book, "1").unwrap().paragraphs,
        ["原创正文 v2 1"]
    );
    let next = l.export(&book, d.path(), ExportFormat::Txt).unwrap();
    assert_ne!(before.path, next.path);
    assert!(std::path::Path::new(&before.path).exists());
    assert!(
        std::fs::read_to_string(next.path)
            .unwrap()
            .contains("原创正文 v2 1")
    );
    l.refresh_chapter(&book, "2", &revised, &AtomicBool::new(false))
        .unwrap();
    let db = rusqlite::Connection::open(d.path().join("library.db")).unwrap();
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM content_blobs", [], |r| r
            .get::<_, u32>(0))
            .unwrap(),
        4
    );
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM history_chapters", [], |r| r
            .get::<_, u32>(0))
            .unwrap(),
        6
    );
}
#[test]
fn active_tasks_and_wrong_identity_cannot_apply_catalog_mutations() {
    let d = tempdir().unwrap();
    let l = Library::open(d.path().join("library.db")).unwrap();
    let c = catalog();
    let j = l.create_download(&c, 1, 3).unwrap();
    let book = l.job(&j).unwrap().book_id;
    let mut new = c.clone();
    new.chapters[0].title = "改名".into();
    let diff = l.preview_catalog_update(&book, &new).unwrap();
    assert!(
        l.accept_catalog_update(&book, &new, &diff.previous_hash)
            .is_err()
    );
    assert!(l.history(&book).unwrap().is_empty());
    new.remote_id = "999".into();
    assert!(l.preview_catalog_update(&book, &new).is_err());
}
