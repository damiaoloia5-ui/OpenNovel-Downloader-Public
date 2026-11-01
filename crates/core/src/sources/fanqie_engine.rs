//! Validate the narrow plaintext envelope emitted by an isolated local engine.
//! This module neither loads that engine nor grants network/source capabilities.
use crate::{Error, ImportChapter, ImportDocument, Result};
use scraper::{Html, Selector};
use serde::Deserialize;

pub const LOCAL_ENGINE_SOURCE: &str = "fanqie-app-local";

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
        || clean(&value.title) != clean(expected_title)
        || expected_book.is_empty()
        || expected_item.is_empty()
        || expected_title.trim().is_empty()
        || !(1..=200_000).contains(&value.word_number)
    {
        return Err(changed());
    }
    if value.need_pay != 0 {
        return Err(Error::Restricted("本地引擎返回需付费章节，未保存".into()));
    }
    if value
        .content
        .chars()
        .any(|c| matches!(c, '\u{e000}'..='\u{f8ff}' | '\u{fffd}'))
    {
        return Err(changed());
    }
    let html = Html::parse_fragment(&value.content);
    let forbidden = Selector::parse("script,style,img,iframe,canvas,svg,audio,video,[hidden]")
        .expect("static selector");
    if html.select(&forbidden).next().is_some() {
        return Err(changed());
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
    if count != value.word_number
        || paragraphs.iter().any(|p| {
            ["订阅本章", "试读结束", "请完成验证"]
                .iter()
                .any(|m| p.contains(m))
        })
    {
        return Err(changed());
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
