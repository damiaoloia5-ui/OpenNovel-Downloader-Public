use opennovel_core::sources::{parse_ciweimao_catalog, parse_faloo_catalog};

#[test]
fn ciweimao_catalog_requires_matching_book_and_latest_chapter() {
    let detail = r#"<meta property="og:novel:read_url" content="https://www.ciweimao.com/book/123"><meta property="og:novel:book_name" content="原创山路"><meta property="og:novel:author" content="原创作者"><meta property="og:description" content="旅人远行。">"#;
    let directory = r#"<div class="book-catalog"><div class="hd"><a href="https://www.ciweimao.com/book/123">返回</a><h3>原创山路</h3></div><div class="update-info">最新章节：第二章 雨停 更新时间：2026-11-03</div><ul class="book-chapter-list"><li><a href="https://www.ciweimao.com/chapter/101">第一章 山路</a></li><li><a href="https://www.ciweimao.com/chapter/102">第二章 雨停</a></li></ul></div>"#;
    let c = parse_ciweimao_catalog(detail, directory, "123").unwrap();
    assert_eq!(c.chapters.len(), 2);
    assert_eq!(c.author, "原创作者");
    assert!(c.chapters.iter().all(|c| c.restricted));
    assert!(parse_ciweimao_catalog(detail, directory, "456").is_err());
    assert!(
        parse_ciweimao_catalog(detail, &directory.replace("chapter/102", "book/102"), "123")
            .is_err()
    );
    assert!(
        parse_ciweimao_catalog(
            detail,
            &directory.replace("最新章节：第二章 雨停", "最新章节：第三章 到达"),
            "123"
        )
        .is_err()
    );
}

#[test]
fn faloo_catalog_preserves_id_gaps_and_rejects_missing_tail_order_and_foreign_books() {
    let html = r#"<input id="hidNovelID" value="123"><meta property="og:title" content="原创山路"><meta property="og:description" content="旅人远行。"><a href="/123_2.html">最新章节</a><div id="mulu"><a href="/v_123.html">第一卷</a><a href="/123_1.html" title="原创山路:第一章 山路">第一章...</a><a href="/123_2.html">第二章 雨停</a></div>"#;
    let c = parse_faloo_catalog(html, "123").unwrap();
    assert_eq!(c.chapters.len(), 2);
    assert_eq!(c.chapters[0].title, "第一章 山路");
    assert!(parse_faloo_catalog(html, "456").is_err());
    assert!(
        parse_faloo_catalog(
            &html.replace("<a href=\"/123_2.html\">第二章 雨停</a>", ""),
            "123"
        )
        .is_err()
    );
    let gaps = parse_faloo_catalog(&html.replace("/123_2.html", "/123_3.html"), "123").unwrap();
    assert_eq!(gaps.chapters.len(), 2);
    assert_eq!(gaps.chapters[1].id, "3");
    assert!(parse_faloo_catalog(&html.replace("/123_2.html", "/123_1.html"), "123").is_err());
    assert!(
        parse_faloo_catalog(
            &html.replace("/123_1.html", "https://other.invalid/123_1.html"),
            "123"
        )
        .is_err()
    );
}
