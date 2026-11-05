use opennovel_core::{
    Error,
    sources::{Catalog, ChapterRef, Migu, identify},
};
use serde_json::{Value, json};

fn chapter(id: &str, position: usize, fee: u8) -> Value {
    json!({"chapterId":id,"chapterName":format!("原创第{position}章"),"chapterSort":position,"feeStatus":fee,
        "chapterUrl":format!("https://n.cmread.com/nap/123/{id}.htm"),"isContentHide":0,"isHide":0,"onlyContentHide":"0","otherHidden":"0","isReadAhead":false})
}
fn catalog() -> Catalog {
    Catalog {
        source: "migu-web".into(),
        remote_id: "123".into(),
        title: "原创山路".into(),
        author: "原创作者".into(),
        description: String::new(),
        chapters: vec![],
    }
}
fn response() -> Value {
    json!({"status":"200","isvisable":"1","chapterContent":{"chapterContent":"<p><span>原创山路。</span></p><p>旅人继续前行。</p>","currPage":"1","totalPage":"1","payUrl":"","isSDKSub":"0","onlyContentHide":"0","otherHidden":"0","pageActionUrl":"/nap/f/ms.content.wechatService/wechatForChapterContentAction.json?bid=123&cid=101"}})
}
fn selected() -> ChapterRef {
    ChapterRef {
        id: "101".into(),
        title: "原创章节".into(),
        restricted: false,
    }
}

#[test]
fn catalogs_require_explicit_access_and_complete_ordered_pages() {
    let mut c = catalog();
    let first = json!({"totalRecordCount":2,"volumeInfo":[{"chapterInfo":[chapter("101",1,0)]}],"nextPageUrl":"/nap/p/catalogForWap.jsp?bid=123&start=2&count=10&order=0&tracking=drop"});
    let next = Migu::append_catalog(&mut c, &first, 2).unwrap().unwrap();
    assert!(!next.contains("tracking"));
    let last = json!({"totalRecordCount":2,"volumeInfo":[{"chapterInfo":[chapter("102",2,1)]}],"nextPageUrl":""});
    assert!(Migu::append_catalog(&mut c, &last, 2).unwrap().is_none());
    assert!(!c.chapters[0].restricted && c.chapters[1].restricted);
    for link in [
        "https://evil.test/nap/p/catalogForWap.jsp?bid=123&start=2&count=10&order=0",
        "/nap/p/catalogForWap.jsp?bid=999&start=2&count=10&order=0",
        "/nap/p/catalogForWap.jsp?bid=123&bid=123&start=2&count=10&order=0",
        "",
    ] {
        let mut bad = first.clone();
        bad["nextPageUrl"] = link.into();
        assert!(Migu::append_catalog(&mut catalog(), &bad, 2).is_err());
    }
    let mut unknown = first.clone();
    unknown["volumeInfo"][0]["chapterInfo"][0]["feeStatus"] = json!(2);
    let mut blocked = catalog();
    Migu::append_catalog(&mut blocked, &unknown, 2).unwrap();
    assert!(blocked.chapters[0].restricted);
    unknown["volumeInfo"][0]["chapterInfo"][0]["feeStatus"] = Value::Null;
    assert!(Migu::append_catalog(&mut catalog(), &unknown, 2).is_err());
}

#[test]
fn chapter_content_rejects_cross_identity_hidden_paid_and_incomplete_pages() {
    let value = response();
    let (body, pages) = Migu::parse_content(&value.to_string(), "123", &selected(), 1).unwrap();
    assert_eq!(body, vec!["原创山路。", "旅人继续前行。"]);
    assert_eq!(pages, 1);
    for style in ["font-size:0.75em", "opacity:0.5", "font-size:12px"] {
        let mut visible = value.clone();
        visible["chapterContent"]["chapterContent"] =
            format!("<p style='{style}'>原创正文</p>").into();
        assert!(Migu::parse_content(&visible.to_string(), "123", &selected(), 1).is_ok());
    }
    for body in [
        "<p style='font-size:0.0em'>原创</p>",
        "<p>原创</p>遗漏文字",
        "<p aria-hidden='true'>原创</p>",
        "<p>原创\u{e001}</p>",
    ] {
        let mut hidden = value.clone();
        hidden["chapterContent"]["chapterContent"] = body.into();
        assert!(Migu::parse_content(&hidden.to_string(), "123", &selected(), 1).is_err());
    }
    for (key, bad) in [
        ("payUrl", json!("/purchase")),
        ("onlyContentHide", json!("1")),
        ("isSDKSub", json!("1")),
        ("currPage", json!("2")),
        ("totalPage", json!("0")),
        (
            "pageActionUrl",
            json!(
                "/nap/f/ms.content.wechatService/wechatForChapterContentAction.json?bid=999&cid=101"
            ),
        ),
        ("chapterContent", json!("<p>原创</p><img src='x'>")),
        ("chapterContent", json!("<p style='display:none'>原创</p>")),
    ] {
        let mut changed = value.clone();
        changed["chapterContent"][key] = bad;
        assert!(
            Migu::parse_content(&changed.to_string(), "123", &selected(), 1).is_err(),
            "{key}"
        );
    }
    let mut paid = selected();
    paid.restricted = true;
    assert!(matches!(
        Migu::parse_content(&value.to_string(), "123", &paid, 1),
        Err(Error::Restricted(_))
    ));
}

#[test]
fn official_links_and_json_extraction_preserve_boundaries() {
    assert_eq!(
        identify("https://www.cmread.com/yuekan/novel/123.html").unwrap(),
        ("migu-web", "123".into())
    );
    assert_eq!(
        identify("https://n.cmread.com/nap/p/wap_detail_new.jsp?bid=123").unwrap(),
        ("migu-web", "123".into())
    );
    for bad in [
        "https://n.cmread.com/nap/p/wap_detail_new.jsp?bid=123&token=x",
        "https://n.cmread.com.evil/nap/p/wap_detail_new.jsp?bid=123",
        "https://n.cmread.com/nap/p/wap_detail_new.jsp?bid=123&bid=456",
    ] {
        assert!(identify(bad).is_err());
    }
    assert_eq!(
        Migu::page_data(
            "<script>var pageData = {\"label\":\"原创 } \\\"\"};doNotExecute()</script>"
        )
        .unwrap()["label"],
        "原创 } \""
    );
    assert!(Migu::page_data("var pageData = executeCode()").is_err());
}

#[test]
fn larger_catalog_pages_preserve_every_chapter_and_access_flag() {
    let mut c = catalog();
    for start in [1, 101, 201] {
        let end = (start + 99).min(235);
        let rows: Vec<_> = (start..=end)
            .map(|position| {
                chapter(
                    &(1000 + position).to_string(),
                    position,
                    u8::from(position > 91),
                )
            })
            .collect();
        let next = if end < 235 {
            format!(
                "/nap/p/catalogForWap.jsp?bid=123&start={}&count=100&order=0",
                end + 1
            )
        } else {
            String::new()
        };
        let block =
            json!({"totalRecordCount":235,"volumeInfo":[{"chapterInfo":rows}],"nextPageUrl":next});
        let returned = Migu::append_catalog(&mut c, &block, 235).unwrap();
        assert_eq!(returned.is_some(), end < 235);
        if let Some(url) = returned {
            assert!(url.contains("count=100"));
        }
    }
    assert_eq!(c.chapters.len(), 235);
    assert_eq!(c.chapters.iter().filter(|c| !c.restricted).count(), 91);
    assert_eq!(c.chapters.last().unwrap().id, "1235");
    c.validate().unwrap();
}

#[test]
fn catalog_batches_reject_replayed_pages_and_ambiguous_page_sizes() {
    let first = json!({"totalRecordCount":200,"volumeInfo":[{"chapterInfo":(1..=100).map(|p| chapter(&(1000+p).to_string(),p,0)).collect::<Vec<_>>()}],"nextPageUrl":"/nap/p/catalogForWap.jsp?bid=123&start=101&count=100&order=0"});
    let mut c = catalog();
    Migu::append_catalog(&mut c, &first, 200).unwrap();
    assert!(Migu::append_catalog(&mut c, &first, 200).is_err());
    for query in ["count=100&count=10", "count=1000", "count=0", "count=abc"] {
        let mut bad = first.clone();
        bad["nextPageUrl"] =
            format!("/nap/p/catalogForWap.jsp?bid=123&start=101&{query}&order=0").into();
        assert!(Migu::append_catalog(&mut catalog(), &bad, 200).is_err());
    }
}

#[test]
fn visible_line_breaks_are_preserved_and_unknown_glyphs_stop_the_job() {
    let mut value = response();
    value["chapterContent"]["chapterContent"] =
        "<p><span>第一行<br>第二行</span><br><br>第三行</p><p>另一段 &amp; 原创。</p>".into();
    let (paragraphs, _) = Migu::parse_content(&value.to_string(), "123", &selected(), 1).unwrap();
    assert_eq!(
        paragraphs,
        ["第一行", "第二行", "第三行", "另一段 & 原创。"]
    );
    for glyph in ['\u{e001}', '\u{f0001}', '\u{100001}', '\u{fffd}'] {
        value["chapterContent"]["chapterContent"] = format!("<p>原创{glyph}</p>").into();
        assert!(matches!(
            Migu::parse_content(&value.to_string(), "123", &selected(), 1),
            Err(Error::SourceChanged(_))
        ));
    }
}
