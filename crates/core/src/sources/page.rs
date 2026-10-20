use crate::{Error, Result};
use scraper::{ElementRef, Html, Selector};

pub(super) fn changed(message: &str) -> Error {
    Error::SourceChanged(message.into())
}
pub(super) fn selector(value: &str) -> Selector {
    Selector::parse(value).expect("static selector")
}
pub(super) fn one<'a>(document: &'a Html, value: &str) -> Result<ElementRef<'a>> {
    let rule = selector(value);
    let mut found = document.select(&rule);
    let element = found
        .next()
        .ok_or_else(|| changed("官网必需的页面字段缺失"))?;
    if found.next().is_some() {
        return Err(changed("官网页面字段重复，不能确定内容归属"));
    }
    Ok(element)
}
pub(super) fn text(document: &Html, value: &str) -> Result<String> {
    Ok(one(document, value)?
        .text()
        .collect::<String>()
        .trim()
        .to_owned())
}
pub(super) fn numeric(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 24
        && !value.starts_with('0')
        && value.bytes().all(|c| c.is_ascii_digit())
}
pub(super) fn count(value: &str) -> Result<usize> {
    let count = value
        .trim()
        .trim_end_matches('章')
        .trim()
        .parse::<usize>()
        .map_err(|_| changed("官网章节总数字段变化"))?;
    if count == 0 || count > crate::MAX_CHAPTERS {
        return Err(changed("官网目录为空或超过 10000 章限制"));
    }
    Ok(count)
}
pub(super) fn route_identity(html: &str, expected: &str) -> Result<()> {
    let start = html
        .rfind("routePath:")
        .ok_or_else(|| changed("官网页面路由身份缺失"))?
        + "routePath:".len();
    let path = serde_json::Deserializer::from_str(&html[start..])
        .into_iter::<String>()
        .next()
        .ok_or_else(|| changed("官网页面路由身份缺失"))?
        .map_err(|_| changed("官网页面路由身份不是字符串"))?;
    if path != expected {
        return Err(changed("官网返回的作品或章节身份与请求不一致"));
    }
    Ok(())
}
