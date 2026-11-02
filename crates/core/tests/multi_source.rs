use opennovel_core::{
    Error, ExportFormat, ImportChapter, Library,
    sources::{
        Catalog, ChapterRef, Qimao, Shuqi, SourceAdapter, SourceHub, identify, source_infos,
    },
};
use serde_json::{Value, json};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use tempfile::tempdir;

const BOOK: &str = "10000000001";
fn qimao_fixture() -> (String, Value) {
    let html = format!(
        r#"<div class="book-information"><div class="title"><span class="txt">原创多来源作品</span></div><div class="update-chapter-title"><a href="https://www.qimao.com/shuku/{BOOK}-103/">原创第3章</a></div></div><div class="author-information"><div class="author-name"><a>OpenNovel</a></div></div><div class="book-introduction"><p class="intro">本项目原创的契约样例。</p></div><div class="tab-inner">作品目录<span class="sub-txt">3章</span></div><script>window.__NUXT__={{routePath:"/shuku/{BOOK}/"}};</script>"#
    );
    let chapters=(1..=3).map(|n|json!({"id":format!("10{n}"),"title":format!("原创第{n}章"),"is_vip":if n==2{"1"}else{"0"},"index":n.to_string(),"chapter_sort":n.to_string()})).collect::<Vec<_>>();
    (html, json!({"data":{"chapters":chapters}}))
}
fn qimao_catalog() -> Catalog {
    let (html, json) = qimao_fixture();
    Qimao::parse_catalog(&html, &json.to_string(), BOOK).unwrap()
}
fn chapter_fixture() -> (String, String) {
    let paragraph = "原创正文包含 <安全> & 中文标点。";
    let encoded = paragraph
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    let html = format!(
        r#"<div class="chapter-detail-wrap"><div class="chapter-detail-wrap-info"><h2 class="chapter-title">原创第1章</h2><dl class="chapter-tips"><dt>本章字数：</dt><dd>{}</dd></dl></div><div class="chapter-detail-wrap-content"><div class="chapter-detail-article"><div class="article"><p>{encoded}</p></div></div></div></div><div class="chapter-detail-article"><div class="article"><p>其他作品推荐不能进入正文。</p></div></div><script>window.__NUXT__={{routePath:"/shuku/{BOOK}-101/"}};</script>"#,
        paragraph.chars().count()
    );
    (html, paragraph.into())
}
fn shuqi_fixture() -> (String, String) {
    let html = format!(
        r#"<i class="js-pageData">{{"bookId":"{BOOK}","bookName":"原创书旗目录","authorName":"OpenNovel"}}</i><div class="titleline"><span class="title">目录 · 共3章</span></div><span class="js-showDesc" data-text="原创简介"></span>"#
    );
    let links=(1..=3).map(|n|format!(r#"<tr><td><a href="/reader?bid={BOOK}&amp;cid=10{n}">{n}.原创第{n}章</a></td></tr>"#)).collect::<String>();
    (html, format!("<table class=\"chapterul\">{links}</table>"))
}

#[test]
fn links_are_explicit_and_source_ids_do_not_guess_another_platform() {
    for (input, source, id) in [
        ("https://www.qimao.com/shuku/222767/", "qimao-web", "222767"),
        (
            "http://qimao.com/shuku/222767?utm_source=example",
            "qimao-web",
            "222767",
        ),
        ("qimao:222767", "qimao-web", "222767"),
        (
            "https://shuqi.com/book/9147568.html",
            "shuqi-web",
            "9147568",
        ),
        ("shuqi:9147568", "shuqi-web", "9147568"),
        ("7400799554502675518", "fanqie-web", "7400799554502675518"),
    ] {
        assert_eq!(identify(input).unwrap(), (source, id.into()));
    }
    for input in [
        "",
        "222767",
        "qimao:0",
        "qimao:00123",
        "qimao:abc",
        "shuqi:1/2",
        "https://qimao.com.evil.test/shuku/222767/",
        "https://user:secret@www.qimao.com/shuku/222767/",
        "https://www.qimao.com:444/shuku/222767/",
        "https://www.qimao.com/shuku/222767-101/",
        "https://www.shuqi.com/reader?bid=9147568",
        "file:///book/9147568.html",
    ] {
        assert!(identify(input).is_err(), "{input}");
    }
}

#[test]
fn qimao_catalog_requires_complete_order_identity_and_known_permissions() {
    let (html, data) = qimao_fixture();
    let catalog = qimao_catalog();
    assert_eq!(catalog.source, "qimao-web");
    assert_eq!(catalog.chapters.len(), 3);
    assert!(catalog.chapters[1].restricted);
    for field in ["is_vip", "index", "chapter_sort", "id"] {
        let mut invalid = data.clone();
        invalid["data"]["chapters"][0][field] = Value::Null;
        assert!(
            Qimao::parse_catalog(&html, &invalid.to_string(), BOOK).is_err(),
            "{field}"
        );
    }
    let mut short = data.clone();
    short["data"]["chapters"].as_array_mut().unwrap().pop();
    assert!(Qimao::parse_catalog(&html, &short.to_string(), BOOK).is_err());
    let mut reordered = data.clone();
    reordered["data"]["chapters"]
        .as_array_mut()
        .unwrap()
        .swap(0, 1);
    assert!(Qimao::parse_catalog(&html, &reordered.to_string(), BOOK).is_err());
    for invalid in [
        html.replace("-103/", "-104/"),
        html.replace("routePath:", "otherPath:"),
        html.replace("3章", "4章"),
        html.replace(&format!("/shuku/{BOOK}/"), "/shuku/999/"),
    ] {
        assert!(Qimao::parse_catalog(&invalid, &data.to_string(), BOOK).is_err());
    }
}

#[test]
fn qimao_only_reads_the_requested_main_chapter_and_rejects_previews() {
    let (html, paragraph) = chapter_fixture();
    let chapter = qimao_catalog().chapters.remove(0);
    let parsed = Qimao::parse_chapter(&html, BOOK, &chapter).unwrap();
    assert_eq!(parsed.paragraphs, [paragraph]);
    for invalid in [
        html.replace("-101/", "-102/"),
        html.replace("原创第1章", "另一章"),
        html.replace("原创正文包含 &lt;安全&gt; &amp; 中文标点。", "请到App阅读"),
        html.replace("原创正文包含", "\u{e001}文字内容"),
        html.replace("本章字数", "未知字段"),
    ] {
        assert!(Qimao::parse_chapter(&invalid, BOOK, &chapter).is_err());
    }
    let vip = qimao_catalog().chapters.remove(1);
    assert!(matches!(
        Qimao::parse_chapter(&html, BOOK, &vip),
        Err(Error::Restricted(_))
    ));
}

#[test]
fn shuqi_can_inspect_complete_directories_without_claiming_reading_permission() {
    let (html, directory) = shuqi_fixture();
    let catalog = Shuqi::parse_catalog(&html, &directory, BOOK).unwrap();
    assert_eq!(
        (catalog.source.as_str(), catalog.chapters.len()),
        ("shuqi-web", 3)
    );
    assert_eq!(catalog.description, "原创简介");
    let capability = source_infos().iter().find(|s| s.id == "shuqi-web").unwrap();
    assert!(!capability.can_download);
    assert!(matches!(
        SourceHub::new().adapter("shuqi-web"),
        Err(Error::Restricted(_))
    ));
    let temp = tempdir().unwrap();
    let library = Library::open(temp.path().join("library.db")).unwrap();
    assert!(library.create_download(&catalog, 1, 3).is_err());
    assert!(library.snapshot().unwrap().books.is_empty());
    for invalid in [
        directory.replace(&format!("bid={BOOK}"), "bid=999"),
        directory.replace("/reader?bid=", "https://evil.test/reader?bid="),
        directory.replace("1.原创第1章", "2.原创第1章"),
        directory.replace("cid=102", "cid=101"),
    ] {
        assert!(Shuqi::parse_catalog(&html, &invalid, BOOK).is_err());
    }
    assert!(Shuqi::parse_catalog(&html.replace("共3章", "共4章"), &directory, BOOK).is_err());
}

struct OriginalSource {
    source: &'static str,
    calls: AtomicUsize,
}
impl SourceAdapter for OriginalSource {
    fn source_id(&self) -> &str {
        self.source
    }
    fn fetch_chapter(
        &self,
        _: &str,
        chapter: &ChapterRef,
        _: &AtomicBool,
    ) -> opennovel_core::Result<ImportChapter> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(ImportChapter {
            id: chapter.id.clone(),
            title: chapter.title.clone(),
            paragraphs: vec![format!("原创来源 {}，正文 {}", self.source, chapter.id)],
        })
    }
}

#[test]
fn same_remote_identity_on_two_sources_never_shares_cache_or_the_wrong_adapter() {
    let temp = tempdir().unwrap();
    let library = Library::open(temp.path().join("library.db")).unwrap();
    let stop = AtomicBool::new(false);
    let fanqie = OriginalSource {
        source: "fanqie-web",
        calls: AtomicUsize::new(0),
    };
    let qimao = OriginalSource {
        source: "qimao-web",
        calls: AtomicUsize::new(0),
    };
    let mut first = qimao_catalog();
    first.source = "fanqie-web".into();
    let job = library.create_download(&first, 1, 3).unwrap();
    let a = library.run_job(&job, &fanqie, &stop).unwrap();
    let job = library.create_download(&qimao_catalog(), 1, 3).unwrap();
    assert_eq!(library.job(&job).unwrap().saved, 0);
    assert!(library.run_job(&job, &fanqie, &stop).is_err());
    assert_eq!(library.job(&job).unwrap().state, "queued");
    let b = library.run_job(&job, &qimao, &stop).unwrap();
    assert_ne!(a.book_id, b.book_id);
    assert_eq!((a.saved, b.saved, b.blocked), (2, 2, 1));
    assert_eq!(qimao.calls.load(Ordering::SeqCst), 2);
    let receipt = library
        .export(&b.book_id, temp.path(), ExportFormat::Txt)
        .unwrap();
    let text = std::fs::read_to_string(receipt.path).unwrap();
    assert!(text.contains("https://www.qimao.com/shuku/"));
    assert!(text.contains("原创来源 qimao-web"));
    assert!(!text.contains("原创来源 fanqie-web"));
    assert_eq!(library.snapshot().unwrap().books.len(), 2);
}

#[test]
fn qimao_uses_the_shared_cancellation_resume_and_append_only_export_workflow() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("library.db");
    let library = Library::open(&path).unwrap();
    let mut catalog = qimao_catalog();
    for ch in &mut catalog.chapters {
        ch.restricted = false;
    }
    let job = library.create_download(&catalog, 1, 3).unwrap();
    struct Late<'a> {
        library: &'a Library,
        job: &'a str,
        calls: AtomicUsize,
    }
    impl SourceAdapter for Late<'_> {
        fn source_id(&self) -> &str {
            "qimao-web"
        }
        fn fetch_chapter(
            &self,
            _: &str,
            ch: &ChapterRef,
            stop: &AtomicBool,
        ) -> opennovel_core::Result<ImportChapter> {
            if self.calls.fetch_add(1, Ordering::SeqCst) == 1 {
                self.library.request_stop(self.job, false, Some(stop))?;
            }
            Ok(ImportChapter {
                id: ch.id.clone(),
                title: ch.title.clone(),
                paragraphs: vec!["原创取消恢复正文。".into()],
            })
        }
    }
    let partial = library
        .run_job(
            &job,
            &Late {
                library: &library,
                job: &job,
                calls: AtomicUsize::new(0),
            },
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!((partial.state.as_str(), partial.saved), ("cancelled", 1));
    drop(library);
    let library = Library::open(&path).unwrap();
    let source = OriginalSource {
        source: "qimao-web",
        calls: AtomicUsize::new(0),
    };
    let result = library
        .run_job(&job, &source, &AtomicBool::new(false))
        .unwrap();
    assert_eq!(result.saved, 3);
    assert_eq!(source.calls.load(Ordering::SeqCst), 2);
    let old = library
        .export(&result.book_id, temp.path(), ExportFormat::Epub)
        .unwrap();
    assert!(old.path.ends_with(".epub"));
    catalog.chapters.push(ChapterRef {
        id: "104".into(),
        title: "原创新增第4章".into(),
        restricted: false,
    });
    let update = library.create_download(&catalog, 1, 4).unwrap();
    assert_eq!(
        library
            .run_job(&update, &source, &AtomicBool::new(false))
            .unwrap()
            .saved,
        4
    );
    let new = library
        .export(&result.book_id, temp.path(), ExportFormat::Epub)
        .unwrap();
    assert!(new.path.ends_with("（2）.epub"));
    assert_ne!(old.path, new.path);
    assert!(std::path::Path::new(&old.path).exists());
}
