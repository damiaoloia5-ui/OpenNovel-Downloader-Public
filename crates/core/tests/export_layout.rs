use opennovel_core::{
    ExportFormat, Library,
    sources::{Catalog, ChapterRef, SourceAdapter},
};
use std::{path::Path, sync::atomic::AtomicBool};

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
            paragraphs: vec!["原创正文：山间的灯亮了。".into()],
        })
    }
}
fn catalog(id: &str) -> Catalog {
    Catalog {
        source: "fanqie-web".into(),
        remote_id: id.into(),
        title: "山间来信".into(),
        author: "林间客".into(),
        description: "第一行简介 <山> & 风\n第二行简介。".into(),
        chapters: vec![ChapterRef {
            id: "one".into(),
            title: "第一章 来信".into(),
            restricted: false,
        }],
    }
}
fn imported(l: &Library, id: &str) -> String {
    let job = l.create_download(&catalog(id), 1, 1).unwrap();
    l.run_job(&job, &Original, &AtomicBool::new(false))
        .unwrap()
        .book_id
}

#[test]
fn metadata_and_description_are_in_txt_and_epub_with_concise_filenames() {
    let d = tempfile::tempdir().unwrap();
    let l = Library::open(d.path().join("library.db")).unwrap();
    let book = imported(&l, "10000000001");
    let txt = l.export_automatic_txt(&book, d.path()).unwrap();
    assert_eq!(
        Path::new(&txt.path).file_name().unwrap(),
        "番茄小说－山间来信－林间客.txt"
    );
    let text = std::fs::read_to_string(txt.path).unwrap();
    for value in [
        "作者：林间客",
        "番茄公开网页",
        "https://fanqienovel.com/page/10000000001",
        "作品简介：",
        "第一行简介 <山> & 风\n第二行简介。",
        "1 / 1 章",
    ] {
        assert!(text.contains(value), "{value}");
    }
    let epub = l.export(&book, d.path(), ExportFormat::Epub).unwrap();
    let mut archive = zip::ZipArchive::new(std::fs::File::open(epub.path).unwrap()).unwrap();
    for name in ["EPUB/about.xhtml", "EPUB/package.opf"] {
        let mut xml = String::new();
        use std::io::Read;
        archive
            .by_name(name)
            .unwrap()
            .read_to_string(&mut xml)
            .unwrap();
        assert!(xml.contains("第一行简介 &lt;山&gt; &amp; 风"));
    }
}

#[test]
fn directory_choice_is_per_book_persistent_and_failed_choice_preserves_it() {
    let d = tempfile::tempdir().unwrap();
    let db = d.path().join("library.db");
    let l = Library::open(&db).unwrap();
    let a = imported(&l, "10000000001");
    let b = imported(&l, "10000000002");
    let custom = d.path().join("我的作品");
    std::fs::create_dir(&custom).unwrap();
    l.set_export_directory(&a, Some(&custom)).unwrap();
    assert!(
        l.set_export_directory(&a, Some(Path::new("relative")))
            .is_err()
    );
    assert!(l.set_export_directory(&a, Some(&db)).is_err());
    drop(l);
    let l = Library::open(&db).unwrap();
    let location = l.export_destination(&a, d.path().join("default")).unwrap();
    assert_eq!(Path::new(&location.directory), custom);
    assert!(location.custom);
    assert!(
        !l.export_destination(&b, d.path().join("default"))
            .unwrap()
            .custom
    );
    l.set_export_directory(&a, None).unwrap();
    assert!(
        Path::new(
            &l.export_destination(&a, d.path().join("default"))
                .unwrap()
                .directory
        )
        .starts_with(d.path().join("default"))
    );
}

#[test]
fn identical_names_for_different_books_do_not_overwrite_and_restart_reuses_own_file() {
    let d = tempfile::tempdir().unwrap();
    let db = d.path().join("library.db");
    let l = Library::open(&db).unwrap();
    let a = imported(&l, "10000000001");
    let b = imported(&l, "10000000002");
    let first = l.export_automatic_txt(&a, d.path()).unwrap();
    let second = l.export_automatic_txt(&b, d.path()).unwrap();
    assert_ne!(first.path, second.path);
    assert!(second.path.ends_with("（2）.txt"));
    assert!(
        std::fs::read_to_string(&first.path)
            .unwrap()
            .contains("10000000001")
    );
    drop(l);
    let l = Library::open(&db).unwrap();
    assert_eq!(
        l.export_automatic_txt(&a, d.path()).unwrap().path,
        first.path
    );
    assert_eq!(
        l.export_automatic_txt(&b, d.path()).unwrap().path,
        second.path
    );
}

#[test]
fn schema_eight_upgrade_keeps_original_backup_and_restore_resets_external_directories() {
    let d = tempfile::tempdir().unwrap();
    let db = d.path().join("library.db");
    let l = Library::open(&db).unwrap();
    let book = imported(&l, "10000000001");
    drop(l);
    let conn = rusqlite::Connection::open(&db).unwrap();
    conn.execute_batch("DROP INDEX exports_content; ALTER TABLE exports DROP COLUMN content_hash; ALTER TABLE exports DROP COLUMN automatic; DROP TABLE export_directories; PRAGMA user_version=8;").unwrap();
    drop(conn);
    let l = Library::open(&db).unwrap();
    assert_eq!(l.diagnostics().unwrap().schema, 10);
    let backup = std::fs::read_dir(d.path())
        .unwrap()
        .map(Result::unwrap)
        .find(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with("library-v8-backup-")
        })
        .unwrap();
    let old = rusqlite::Connection::open(backup.path()).unwrap();
    assert_eq!(
        old.pragma_query_value(None, "user_version", |r| r.get::<_, u32>(0))
            .unwrap(),
        8
    );
    assert_eq!(
        old.query_row("SELECT count(*) FROM chapters", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        1
    );
    l.set_export_directory(&book, Some(d.path())).unwrap();
    let backup = d.path().join("backup");
    l.backup(&backup).unwrap();
    let restored = Library::restore(&backup, d.path().join("restored")).unwrap();
    assert!(
        !restored
            .export_destination(&book, d.path().join("restored/exports"))
            .unwrap()
            .custom
    );
    assert!(l.export_destination(&book, d.path()).unwrap().custom);
}

#[test]
fn forbidden_filename_characters_and_empty_author_get_safe_readable_names() {
    let d = tempfile::tempdir().unwrap();
    let l = Library::open(d.path().join("library.db")).unwrap();
    let mut c = catalog("10000000001");
    c.title = "山<间>:来/信?*".into();
    c.author = String::new();
    let job = l.create_download(&c, 1, 1).unwrap();
    let book = l
        .run_job(&job, &Original, &AtomicBool::new(false))
        .unwrap()
        .book_id;
    let receipt = l.export_automatic_txt(&book, d.path()).unwrap();
    assert_eq!(
        Path::new(&receipt.path).file_name().unwrap(),
        "番茄小说－山间来信－佚名.txt"
    );
}
