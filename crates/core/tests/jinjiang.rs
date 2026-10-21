use opennovel_core::{
    Error,
    sources::{Jinjiang, identify},
};
fn fixture() -> String {
    r#"<span data-novelid="123"></span><h1 itemprop="name">原创章节</h1><span itemprop="author">OpenNovel</span><div itemprop="description">原创契约</div><table>
    <tr itemprop="chapter"><td>1</td><td><span itemprop="headline"><a itemprop="url" href="http://www.jjwxc.net/onebook.php?novelid=123&amp;chapterid=1">第一章</a></span></td><td itemprop="wordCount">12</td></tr>
    <tr itemprop="chapter newestChapter"><td>2</td><td><span itemprop="headline"><a itemprop="url" rel="http://my.jjwxc.net/onebook_vip.php?novelid=123&amp;chapterid=2">第二章</a>[VIP]</span></td><td itemprop="wordCount">12</td></tr>
    <tr itemprop="chapter"><td>3</td><td><span itemprop="headline">[锁]</span></td><td itemprop="wordCount">12</td></tr></table>"#.into()
}
#[test]
fn complete_directory_includes_newest_vip_and_locked_rows_without_fetching_them() {
    let html = fixture();
    let c = Jinjiang::parse_catalog(&html, "123").unwrap();
    assert_eq!(c.chapters.len(), 3);
    assert!(!c.chapters[0].restricted);
    assert!(c.chapters[1].restricted && c.chapters[2].restricted);
    for changed in [
        html.replace("<td>2</td>", "<td>4</td>"),
        html.replace("data-novelid=\"123\"", "data-novelid=\"999\""),
        html.replace("www.jjwxc.net/onebook.php", "evil.test/onebook.php"),
        html.replace("&amp;chapterid=1", "&amp;chapterid=9"),
    ] {
        assert!(Jinjiang::parse_catalog(&changed, "123").is_err());
    }
    assert_eq!(
        identify("jinjiang:123").unwrap(),
        ("jinjiang-web", "123".into())
    );
    assert_eq!(
        identify("https://www.jjwxc.net/onebook.php?novelid=123")
            .unwrap()
            .0,
        "jinjiang-web"
    );
    assert!(identify("https://www.jjwxc.net/onebook.php?novelid=123&chapterid=1").is_err());
}
#[test]
fn only_explicit_body_with_matching_identity_title_and_length_is_saved() {
    let c = Jinjiang::parse_catalog(&fixture(), "123").unwrap();
    let body = r#"<input name="novelid" value="123"><input name="chapterid" value="1"><div class="noveltext"><h2>第一章</h2><div id="paragraph_comment_content">本项目原创正文内容。<br>另一段原创文字。</div><p>评论不能保存。</p></div>"#;
    let parsed = Jinjiang::parse_chapter(body, "123", &c.chapters[0], 19).unwrap();
    assert_eq!(parsed.paragraphs.len(), 2);
    assert!(!parsed.paragraphs.join("").contains("评论"));
    for b in [
        body.replace("value=\"123\"", "value=\"999\""),
        body.replace("第一章", "错误标题"),
        body.replace("本项目原创正文内容。", "请登录"),
        body.replace("另一段原创文字。", "\u{fffd}错误字符"),
        body.replace("<br>", "<script>code</script>"),
    ] {
        assert!(Jinjiang::parse_chapter(&b, "123", &c.chapters[0], 19).is_err());
    }
    assert!(matches!(
        Jinjiang::parse_chapter(body, "123", &c.chapters[1], 19),
        Err(Error::Restricted(_))
    ));
}
