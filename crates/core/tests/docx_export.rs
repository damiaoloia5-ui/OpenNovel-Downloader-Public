use opennovel_core::{
    ExportFormat, Library, SaveDestination,
    sources::{Catalog, ChapterRef, SourceAdapter},
};
use std::{io::Read, path::Path, sync::atomic::AtomicBool};

struct Original;
impl SourceAdapter for Original {
    fn source_id(&self) -> &str {
        "fanqie-web"
    }
    fn fetch_chapter(
        &self,
        _: &str,
        c: &ChapterRef,
        _: &AtomicBool,
    ) -> opennovel_core::Result<opennovel_core::ImportChapter> {
        Ok(opennovel_core::ImportChapter {
            id: c.id.clone(),
            title: c.title.clone(),
            paragraphs: vec![format!(
                "　　原创正文 {}：Hello <forest> & \"wind\"。\n下一段的灯亮了。",
                c.id
            )],
        })
    }
}
fn catalog() -> Catalog {
    Catalog {
        source: "fanqie-web".into(),
        remote_id: "10000000999".into(),
        title: "原创山间来信".into(),
        author: "林间客".into(),
        description: "原创简介 <山> & 风。".into(),
        chapters: (1..=4)
            .map(|i| ChapterRef {
                id: format!("c{i}"),
                title: format!("原创第{i}章"),
                restricted: false,
            })
            .collect(),
    }
}
fn xml(path: &str, name: &str) -> String {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    let mut value = String::new();
    zip.by_name(name)
        .unwrap()
        .read_to_string(&mut value)
        .unwrap();
    value
}
#[test]
fn word_contains_metadata_safe_text_and_requested_typography() {
    let d = tempfile::tempdir().unwrap();
    let l = Library::open(d.path().join("book.db")).unwrap();
    let j = l.create_download(&catalog(), 1, 4).unwrap();
    let b = l
        .run_job(&j, &Original, &AtomicBool::new(false))
        .unwrap()
        .book_id;
    let receipt = l.export(&b, d.path(), ExportFormat::Docx).unwrap();
    assert_eq!(
        Path::new(&receipt.path).file_name().unwrap(),
        "番茄小说－原创山间来信－林间客.docx"
    );
    let mut zip = zip::ZipArchive::new(std::fs::File::open(&receipt.path).unwrap()).unwrap();
    for i in 0..zip.len() {
        let mut s = String::new();
        zip.by_index(i).unwrap().read_to_string(&mut s).unwrap();
        roxmltree::Document::parse(&s).unwrap();
    }
    let document = xml(&receipt.path, "word/document.xml");
    assert!(document.contains("原创简介 &lt;山&gt; &amp; 风。"));
    assert!(document.contains("Hello &lt;forest&gt; &amp; &quot;wind&quot;。"));
    assert!(!document.contains('　'));
    let parsed = roxmltree::Document::parse(&document).unwrap();
    assert_eq!(
        parsed
            .descendants()
            .filter(|n| n.tag_name().name() == "p")
            .count(),
        19
    );
    let styles = xml(&receipt.path, "word/styles.xml");
    let styles = roxmltree::Document::parse(&styles).unwrap();
    let w = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
    let normal = styles
        .descendants()
        .find(|n| n.attribute((w, "styleId")) == Some("Normal"))
        .unwrap();
    assert!(
        normal
            .descendants()
            .any(|n| n.attribute((w, "firstLineChars")) == Some("200"))
    );
    assert!(
        normal
            .descendants()
            .any(|n| n.attribute((w, "line")) == Some("360")
                && n.attribute((w, "lineRule")) == Some("auto"))
    );
    let defaults = styles
        .descendants()
        .find(|n| n.tag_name().name() == "docDefaults")
        .unwrap();
    assert!(
        defaults
            .descendants()
            .any(|n| n.attribute((w, "eastAsia")) == Some("宋体")
                && n.attribute((w, "ascii")) == Some("Times New Roman"))
    );
    assert!(
        defaults
            .descendants()
            .any(|n| n.tag_name().name() == "sz" && n.attribute((w, "val")) == Some("24"))
    );
    let heading = styles
        .descendants()
        .find(|n| n.attribute((w, "styleId")) == Some("Heading1"))
        .unwrap();
    assert!(
        heading
            .descendants()
            .any(|n| n.attribute((w, "eastAsia")) == Some("黑体"))
    );
    assert!(
        heading
            .descendants()
            .any(|n| n.tag_name().name() == "sz" && n.attribute((w, "val")) == Some("30"))
    );
    assert!(
        styles
            .descendants()
            .any(|n| n.attribute((w, "eastAsia")) == Some("楷体"))
    );
}
#[test]
fn automatic_word_keeps_exact_task_scope_after_restart_and_preserves_edited_files() {
    let d = tempfile::tempdir().unwrap();
    let db = d.path().join("book.db");
    let l = Library::open(&db).unwrap();
    let j = l.create_download(&catalog(), 1, 4).unwrap();
    let b = l
        .run_job(&j, &Original, &AtomicBool::new(false))
        .unwrap()
        .book_id;
    let destination = SaveDestination {
        directory: Some(d.path().to_path_buf()),
        format: Some("docx".into()),
        selection_only: true,
    };
    let selected = l
        .create_download_selected_with_destination(&catalog(), &[2, 3], Some(&destination))
        .unwrap();
    l.run_job(&selected, &Original, &AtomicBool::new(false))
        .unwrap();
    drop(l);
    let l = Library::open(&db).unwrap();
    assert_eq!(
        l.automatic_export_plan(&b).unwrap().job_id.as_deref(),
        Some(selected.as_str())
    );
    let r = l.export_automatic(&b, d.path()).unwrap();
    assert_eq!(
        (r.saved_chapters, r.total_chapters, r.partial),
        (2, 2, false)
    );
    let document = xml(&r.path, "word/document.xml");
    assert!(document.contains("原创第2章") && document.contains("原创第3章"));
    assert!(!document.contains("原创第1章") && !document.contains("原创第4章"));
    assert_eq!(l.book_summary(&b).unwrap().saved_chapters, 4);
    assert_eq!(l.export_automatic(&b, d.path()).unwrap().path, r.path);
    let bytes = std::fs::read(&r.path).unwrap();
    std::fs::remove_file(&r.path).unwrap();
    assert_eq!(l.export_automatic(&b, d.path()).unwrap().path, r.path);
    assert_eq!(std::fs::read(&r.path).unwrap(), bytes);
    std::fs::write(&r.path, "用户保留的编辑内容").unwrap();
    assert!(
        l.export_automatic(&b, d.path())
            .unwrap_err()
            .to_string()
            .contains("已被修改")
    );
    assert_eq!(
        std::fs::read_to_string(&r.path).unwrap(),
        "用户保留的编辑内容"
    );
    let r = l
        .export_selected(&b, "2-3", d.path(), ExportFormat::Txt)
        .unwrap();
    assert_eq!((r.saved_chapters, r.total_chapters), (2, 2));
    assert!(
        !std::fs::read_to_string(&r.path)
            .unwrap()
            .contains("原创第1章")
    );
    assert!(
        l.export_selected(&b, "3-9", d.path(), ExportFormat::Docx)
            .is_err()
    );
}
#[test]
fn new_preferences_are_transactional_and_schema_ten_is_backed_up() {
    let d = tempfile::tempdir().unwrap();
    let db = d.path().join("book.db");
    let l = Library::open(&db).unwrap();
    drop(l);
    let conn = rusqlite::Connection::open(&db).unwrap();
    conn.execute_batch("DROP TABLE automatic_exports; PRAGMA user_version=10;")
        .unwrap();
    drop(conn);
    let l = Library::open(&db).unwrap();
    assert_eq!(l.diagnostics().unwrap().schema, 11);
    assert!(std::fs::read_dir(d.path()).unwrap().any(|e| {
        e.unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("library-v10-backup-")
    }));
    let conn = rusqlite::Connection::open(&db).unwrap();
    conn.execute_batch("CREATE TRIGGER reject_export_plan BEFORE INSERT ON automatic_exports BEGIN SELECT RAISE(ABORT,'original injected export plan failure'); END;").unwrap();
    let destination = SaveDestination {
        format: Some("docx".into()),
        selection_only: true,
        ..Default::default()
    };
    assert!(
        l.create_download_selected_with_destination(&catalog(), &[2, 3], Some(&destination))
            .is_err()
    );
    assert!(l.snapshot().unwrap().books.is_empty());
    assert!(l.snapshot().unwrap().jobs.is_empty());
    assert!(
        SaveDestination {
            format: Some("exe".into()),
            ..Default::default()
        }
        .validate()
        .is_err()
    );
}

#[test]
fn failed_schema_upgrade_does_not_change_old_running_jobs() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("original.db");
    let l = Library::open(&path).unwrap();
    let job = l.create_download(&catalog(), 1, 4).unwrap();
    drop(l);
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("DROP TABLE automatic_exports; CREATE TABLE automatic_exports(original TEXT); PRAGMA user_version=10; UPDATE jobs SET state='running';").unwrap();
    assert!(Library::open(&path).is_err());
    assert_eq!(
        db.pragma_query_value(None, "user_version", |r| r.get::<_, u32>(0))
            .unwrap(),
        10
    );
    assert_eq!(
        db.query_row("SELECT state FROM jobs WHERE id=?1", [job], |r| r
            .get::<_, String>(0))
            .unwrap(),
        "running"
    );
}
