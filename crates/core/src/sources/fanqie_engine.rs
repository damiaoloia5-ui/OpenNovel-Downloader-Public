//! Validate the narrow plaintext envelope emitted by an isolated local engine.
//! This module neither loads that engine nor grants network/source capabilities.
use crate::{Error, ImportChapter, ImportDocument, Result};
use scraper::{Html, Selector};
use serde::Deserialize;

pub const LOCAL_ENGINE_SOURCE: &str = "fanqie-app-local";

// Observed upstream metadata counts ASCII, Han and Chinese punctuation, but not
// every Unicode scalar (e.g. middle dots, circled numbers and arrows). This is
// only a comparison metric: never remove those characters from saved content.
fn metadata_character(c: char) -> bool {
    c.is_ascii_graphic()
        || matches!(c, '\u{4e00}'..='\u{9fff}' | '\u{3000}'..='\u{303f}'
            | '\u{ff00}'..='\u{ffef}' | '—' | '‘' | '’' | '“' | '”' | '…')
}

impl crate::Library {
    /// Explicit experimental path; normal SourceHub, HTTP and batch capabilities stay disabled.
    pub fn create_engine_download(
        &self,
        catalog: &super::Catalog,
        positions: &[usize],
    ) -> Result<String> {
        if catalog.source != LOCAL_ENGINE_SOURCE {
            return Err(Error::Invalid("本地引擎任务必须使用独立来源身份".into()));
        }
        self.create_download_authorized(
            catalog,
            positions,
            None,
            crate::downloads::DownloadAuthorization::LocalEngine,
        )
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    schema_version: u32,
    book_id: String,
    item_id: String,
    title: String,
    word_number: usize,
    need_pay: u64,
    content: String,
}

pub fn parse_engine_chapter(
    raw: &str,
    expected_book: &str,
    expected_item: &str,
    expected_title: &str,
) -> Result<ImportChapter> {
    let changed = || Error::SourceChanged("本地引擎响应未通过身份、权限或完整性校验".into());
    if raw.len() > 2 * 1024 * 1024 {
        return Err(changed());
    }
    let value: Envelope = serde_json::from_str(raw).map_err(|_| changed())?;
    let clean = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
    if value.schema_version != 1
        || value.book_id != expected_book
        || value.item_id != expected_item
        || expected_book.is_empty()
        || expected_item.is_empty()
        || expected_title.trim().is_empty()
        || !(1..=200_000).contains(&value.word_number)
    {
        return Err(changed());
    }
    if clean(&value.title) != clean(expected_title) {
        return Err(Error::SourceChanged(
            "本地引擎正文标题与目录不一致，未保存本章".into(),
        ));
    }
    if value.need_pay != 0 {
        return Err(Error::Restricted("本地引擎返回需付费章节，未保存".into()));
    }
    if value
        .content
        .chars()
        .any(|c| matches!(c, '\u{e000}'..='\u{f8ff}' | '\u{fffd}'))
    {
        return Err(Error::SourceChanged(
            "本地引擎正文含未识别字形，未保存本章".into(),
        ));
    }
    let html = Html::parse_fragment(&value.content);
    let forbidden = Selector::parse("script,style,img,iframe,canvas,svg,audio,video,[hidden]")
        .expect("static selector");
    if html.select(&forbidden).next().is_some() {
        return Err(Error::SourceChanged(
            "本地引擎正文含隐藏、受保护或异常结构，未保存本章".into(),
        ));
    }
    let paragraphs = html
        .select(&Selector::parse("p").expect("static selector"))
        .map(|p| p.text().collect::<String>().trim().to_owned())
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>();
    let count = paragraphs
        .iter()
        .map(|p| clean(p).chars().count())
        .sum::<usize>();
    let metadata_count = paragraphs
        .iter()
        .flat_map(|p| p.chars())
        .filter(|c| !c.is_whitespace() && metadata_character(*c))
        .count();
    // A different counting convention needs a complete framed response. An
    // unexplained extra/missing character is not accepted via percentage slack.
    let framed_metadata_match = metadata_count == value.word_number
        && ["header", "article", "footer"].iter().all(|tag| {
            html.select(&Selector::parse(tag).expect("static tag"))
                .count()
                == 1
        })
        && value.content.contains("</article>")
        && value.content.contains("</footer>");
    if count != value.word_number && !framed_metadata_match {
        return Err(Error::SourceChanged(format!(
            "本地引擎正文长度未通过校验（实际 {count}，平台口径 {metadata_count}，声明 {}），未保存本章",
            value.word_number
        )));
    }
    if paragraphs.iter().any(|p| {
        ["订阅本章", "试读结束", "请完成验证"]
            .iter()
            .any(|m| p.contains(m))
    }) {
        return Err(Error::SourceChanged(
            "本地引擎正文含订阅、试读或验证提示，未保存本章".into(),
        ));
    }
    let document = ImportDocument {
        id: value.book_id,
        title: "本地引擎单章验收".into(),
        author: String::new(),
        chapters: vec![ImportChapter {
            id: value.item_id,
            title: value.title,
            paragraphs,
        }],
    }
    .validated()?;
    document.chapters.into_iter().next().ok_or_else(changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn fixture() -> serde_json::Value {
        json!({"schema_version":1,"book_id":"10000000001","item_id":"10000000002","title":"原创章","word_number":11,"need_pay":0,"content":"<p>山间的小屋亮起一盏灯。</p>"})
    }
    fn check(v: &serde_json::Value) -> Result<ImportChapter> {
        parse_engine_chapter(&v.to_string(), "10000000001", "10000000002", "原创章")
    }
    #[test]
    fn original_plaintext_is_validated_without_font_guessing() {
        assert_eq!(
            check(&fixture()).unwrap().paragraphs,
            ["山间的小屋亮起一盏灯。"]
        );
    }
    #[test]
    fn platform_count_preserves_symbols_without_accepting_missing_or_extra_text() {
        let mut value = fixture();
        let original = "山间的小屋亮起一盏灯。".repeat(100);
        value["word_number"] = json!(1100);
        value["content"] = json!(format!(
            "<header>原创章</header><article><p>{original}·①②③→Ⅹ∯のᗜ\u{200c}</p></article><footer></footer>"
        ));
        let parsed = check(&value).unwrap();
        assert_eq!(parsed.paragraphs, [format!("{original}·①②③→Ⅹ∯のᗜ\u{200c}")]);
        for body in [
            format!("{}····", original.chars().take(1099).collect::<String>()),
            format!("{original}补·"),
        ] {
            value["content"] = json!(format!(
                "<header>原创章</header><article><p>{body}</p></article><footer></footer>"
            ));
            assert!(check(&value).is_err());
        }
        value["content"] = json!(format!("<p>{original}·</p>"));
        assert!(check(&value).is_err());
        value["content"] = json!(format!(
            "<header>原创章</header><article><p>{original}·</p></article><footer>"
        ));
        assert!(check(&value).is_err());
    }
    #[test]
    fn exact_platform_count_handles_the_reported_four_symbol_difference() {
        let mut value = fixture();
        let body = format!("{}····", "山".repeat(3027));
        value["word_number"] = json!(3027);
        value["content"] = json!(format!(
            "<header>原创章</header><article><p>{body}</p></article><footer></footer>"
        ));
        assert_eq!(check(&value).unwrap().paragraphs, [body]);
        // The previous tiny-addition rule also falsely accepted ordinary extra text.
        value["content"] = json!(format!(
            "<header>原创章</header><article><p>{}多</p></article><footer></footer>",
            "山".repeat(3027)
        ));
        assert!(check(&value).is_err());
    }
    #[test]
    fn identities_permissions_types_and_schema_are_mandatory() {
        for (field, replacement) in [
            ("schema_version", json!(2)),
            ("book_id", json!("10000000009")),
            ("item_id", json!("10000000009")),
            ("title", json!("别章")),
            ("need_pay", json!(1)),
            ("need_pay", serde_json::Value::Null),
            ("word_number", json!("12")),
            ("extra", json!(true)),
        ] {
            let mut v = fixture();
            v[field] = replacement;
            assert!(check(&v).is_err(), "{field}");
        }
    }
    #[test]
    fn previews_truncation_markup_and_unknown_glyphs_are_refused() {
        for content in [
            "<p>少</p>",
            "<p hidden>山间的小屋亮起一盏灯。</p>",
            "<p>山间的小屋亮起一盏灯。</p><img src='x'>",
            "<p>\u{e001}</p>",
            "<p>订阅本章继续阅读。</p>",
        ] {
            let mut v = fixture();
            v["content"] = json!(content);
            assert!(check(&v).is_err());
        }
    }
    #[test]
    fn oversized_and_duplicate_fields_never_parse() {
        assert!(parse_engine_chapter(&" ".repeat(2 * 1024 * 1024 + 1), "x", "y", "z").is_err());
        let raw = fixture().to_string().replacen('{', "{\"need_pay\":0,", 1);
        assert!(parse_engine_chapter(&raw, "10000000001", "10000000002", "原创章").is_err());
    }
}
