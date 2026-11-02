use opennovel_core::{
    Error, ExportFormat, Library,
    sources::{Catalog, ChapterRef, PublicHtml, SourceAdapter, SourceHub, identify, source_infos},
};
use std::sync::atomic::AtomicBool;

fn catalog(zh: bool) -> String {
    if zh {
        r#"<div class="book-meta"><h1>原创旅行</h1><p><span><a>OpenNovel</a></span></p></div><em class="count">共2章</em><ul class="chapter-list"><li class="col-4"><a href="https://book.zongheng.com/chapter/123/10.html">原创第一章</a></li><li class="vip col-4"><a href="https://book.zongheng.com/chapter/123/20.html">原创第二章</a></li></ul>"#.into()
    } else {
        r#"<script>window.$__BOOK = {type: 'list', bookId: 123, vip: 1}</script><h1 class="Title">原创旅行</h1><div class="Author"><a>OpenNovel</a></div><dl class="Volume"><dt><span class="info">已更新2章</span></dt><dd><a href="/chapter/123/10.html"><span class="ellipsis">原创第一章</span></a><a href="/chapter/123/20.html"><span class="ellipsis vip">原创第二章</span></a></dd></dl>"#.into()
    }
}
fn chapter(zh: bool) -> String {
    if zh {
        r#"<body data-bookid="123" data-chapterid="10" data-vip="0" data-ec="0"><div id="Jcontent"><div class="reader-box"><div class="title_txtbox">原创第一章</div><div class="bookinfo">本章字数：10</div><div class="content" data-decoder=""><p>山间有风，旅人向前行。</p></div></div></div></body>"#.into()
    } else {
        r#"<script>window.$__BOOK = {type: 'chapter', bookId: 123, chapterId: 10, vip: 0}</script><div id="readArea"><div class="readAreaBox"><h1>原创第一章</h1><div class="chapter_update_time">本章共10字</div><div class="p"><p>山间有风，旅人向前行。</p><p class="copy"><a>版权声明不属于正文</a></p><div class="author-say"><script>/* Original fixture, never executed. */</script>作者附言不属于正文</div></div></div></div>"#.into()
    }
}
#[test]
fn public_catalogs_preserve_order_and_reject_missing_duplicate_or_foreign_items() {
    for zh in [false, true] {
        let raw = catalog(zh);
        let c = PublicHtml::parse_catalog(&raw, "123", zh).unwrap();
        assert_eq!(
            c.chapters.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
            ["10", "20"]
        );
        assert!(!c.chapters[0].restricted);
        assert!(c.chapters[1].restricted);
        for invalid in [
            raw.replace("2章", "3章"),
            raw.replace("/20.html", "/10.html"),
            raw.replace("/123/20.html", "/999/20.html"),
            raw.replace(
                "/chapter/123/10.html",
                "https://evil.test/chapter/123/10.html",
            ),
        ] {
            assert!(PublicHtml::parse_catalog(&invalid, "123", zh).is_err());
        }
        assert!(PublicHtml::parse_catalog(&raw, "999", zh).is_err());
    }
}
#[test]
fn public_catalog_description_uses_only_unambiguous_17k_book_metadata() {
    let meta =
        r#"<meta property="og:description" content="原创旅人走进山谷。&#10;风带来远方的信。">"#;
    let html = format!("{meta}{}", catalog(false));
    assert_eq!(
        PublicHtml::parse_catalog(&html, "123", false)
            .unwrap()
            .description,
        "原创旅人走进山谷。\n风带来远方的信。"
    );
    assert!(
        PublicHtml::parse_catalog(&catalog(false), "123", false)
            .unwrap()
            .description
            .is_empty()
    );
    assert!(
        PublicHtml::parse_catalog(&format!("{meta}{html}"), "123", false)
            .unwrap()
            .description
            .is_empty()
    );
    assert!(
        PublicHtml::parse_catalog(&format!("{meta}{}", catalog(true)), "123", true)
            .unwrap()
            .description
            .is_empty()
    );
    assert!(PublicHtml::parse_catalog(&html, "999", false).is_err());
}
#[test]
fn public_bodies_require_identity_known_permission_title_and_full_length() {
    for zh in [false, true] {
        let c = PublicHtml::parse_catalog(&catalog(zh), "123", zh).unwrap();
        let raw = chapter(zh).replace("作者附言不属于正文</div>", "作者附言不属于正文</div><div id=\"banner_content\"><script>/* ignored ad */</script></div><div class=\"qrcode\"><img src=\"qr.png\"></div><div class=\"chapter_text_ad\">广告不能保存</div>");
        assert_eq!(
            PublicHtml::parse_chapter(&raw, "123", &c.chapters[0], zh)
                .unwrap()
                .paragraphs,
            ["山间有风，旅人向前行。"]
        );
        assert!(matches!(
            PublicHtml::parse_chapter(&raw, "123", &c.chapters[1], zh),
            Err(Error::Restricted(_))
        ));
        for invalid in [
            raw.replace("山间有风，旅人向前行。", "短文"),
            raw.replace("原创第一章", "其他章"),
            raw.replace("<p>", "<p style=\"display:none\">"),
            raw.replace("<p>", "<canvas></canvas><p>"),
        ] {
            assert!(PublicHtml::parse_chapter(&invalid, "123", &c.chapters[0], zh).is_err());
        }
        assert!(PublicHtml::parse_chapter(&raw, "999", &c.chapters[0], zh).is_err());
        let blocked = if zh {
            raw.replace("data-vip=\"0\"", "data-vip=\"1\"")
        } else {
            raw.replace("vip: 0", "vip: 1")
        };
        assert!(matches!(
            PublicHtml::parse_chapter(&blocked, "123", &c.chapters[0], zh),
            Err(Error::Restricted(_))
        ));
    }
}
#[test]
fn source_links_and_planned_capabilities_do_not_enable_downloads() {
    for (url, id) in [
        ("https://www.zongheng.com/detail/123", "zongheng-web"),
        ("https://book.zongheng.com/book/123.html", "zongheng-web"),
        ("https://www.17k.com/book/123.html", "17k-web"),
        ("https://www.ciweimao.com/book/123", "ciweimao-web"),
        ("https://b.faloo.com/123.html", "faloo-web"),
        ("migu:123", "migu-web"),
    ] {
        assert_eq!(identify(url).unwrap(), (id, "123".into()));
    }
    for url in [
        "https://evil.zongheng.com/detail/123",
        "https://www.17k.com.evil.test/book/123.html",
        "https://user:secret@www.17k.com/book/123.html",
        "https://www.17k.com/book/123.html?token=x",
        "https://b.faloo.com/123_1.html",
        "https://www.ciweimao.com/chapter/123",
        "zongheng:../123",
    ] {
        assert!(identify(url).is_err());
    }
    for s in source_infos()
        .iter()
        .filter(|s| ["ciweimao-web", "faloo-web", "migu-web"].contains(&s.id))
    {
        assert!(!s.can_inspect && !s.can_download);
        assert!(
            SourceHub::new()
                .inspect(&format!("{}:123", s.id.trim_end_matches("-web")))
                .is_err()
        );
        assert!(SourceHub::new().adapter(s.id).is_err());
    }
}
struct Original(bool);
impl SourceAdapter for Original {
    fn source_id(&self) -> &str {
        if self.0 { "zongheng-web" } else { "17k-web" }
    }
    fn fetch_chapter(
        &self,
        book: &str,
        c: &ChapterRef,
        _: &AtomicBool,
    ) -> opennovel_core::Result<opennovel_core::ImportChapter> {
        PublicHtml::parse_chapter(&chapter(self.0), book, c, self.0)
    }
}
#[test]
fn new_sources_use_persistent_jobs_and_partial_txt_without_reading_vip() {
    for zh in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let l = Library::open(dir.path().join("library.db")).unwrap();
        let c: Catalog = PublicHtml::parse_catalog(&catalog(zh), "123", zh).unwrap();
        let id = l.create_download(&c, 1, 2).unwrap();
        let job = l
            .run_job(&id, &Original(zh), &AtomicBool::new(false))
            .unwrap();
        assert_eq!((job.saved, job.blocked), (1, 1));
        let receipt = l
            .export(&job.book_id, dir.path(), ExportFormat::Txt)
            .unwrap();
        assert!(receipt.partial);
        assert_eq!(receipt.saved_chapters, 1);
        assert!(
            std::fs::read_to_string(receipt.path)
                .unwrap()
                .contains("山间有风")
        );
        assert!(
            l.book_details(&job.book_id)
                .unwrap()
                .book
                .source_url()
                .is_some()
        );
    }
}
