use super::{Catalog, ChapterRef, page, web::WebClient};
use crate::{Error, Result};
use reqwest::Url;
use scraper::Html;
use std::sync::atomic::AtomicBool;

pub fn inspect_session_catalog(platform: &str, book: &str) -> Result<Catalog> {
    inspect_session_catalog_with_stop(platform, book, &AtomicBool::new(false))
}

pub(super) fn inspect_session_catalog_with_stop(
    platform: &str,
    book: &str,
    stop: &AtomicBool,
) -> Result<Catalog> {
    if !page::numeric(book) {
        return Err(page::changed("作品身份无效"));
    }
    match platform {
        "ciweimao" => {
            let http = WebClient::new(&["www.ciweimao.com"])?;
            let detail = http.text(
                &format!("https://www.ciweimao.com/book/{book}"),
                stop,
                8 * 1024 * 1024,
            )?;
            let directory = http.text(
                &format!("https://www.ciweimao.com/chapter-list/{book}/book_detail"),
                stop,
                8 * 1024 * 1024,
            )?;
            parse_ciweimao_catalog(&detail, &directory, book)
        }
        "faloo" => {
            let http = WebClient::new(&["b.faloo.com"])?;
            let bytes = http.get(
                &format!("https://b.faloo.com/{book}.html"),
                stop,
                8 * 1024 * 1024,
            )?;
            let (html, _, _) = encoding_rs::GBK.decode(&bytes);
            parse_faloo_catalog(&html, book)
        }
        _ => Err(Error::Invalid("目录通道不存在".into())),
    }
}
fn meta(doc: &Html, name: &str) -> Result<String> {
    page::one(doc, &format!("meta[property='{name}']"))?
        .value()
        .attr("content")
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.trim().into())
        .ok_or_else(|| page::changed("作品元数据缺失"))
}
pub fn parse_ciweimao_catalog(detail: &str, directory: &str, book: &str) -> Result<Catalog> {
    if !page::numeric(book) {
        return Err(page::changed("作品身份无效"));
    }
    let d = Html::parse_document(detail);
    let c = Html::parse_document(directory);
    if meta(&d, "og:novel:read_url")? != format!("https://www.ciweimao.com/book/{book}")
        || !c
            .select(&page::selector(".book-catalog .hd a[href]"))
            .any(|a| {
                a.value().attr("href") == Some(&format!("https://www.ciweimao.com/book/{book}"))
            })
    {
        return Err(page::changed("刺猬猫目录与作品身份不符"));
    }
    let title = meta(&d, "og:novel:book_name")?;
    if page::text(&c, ".book-catalog .hd h3")? != title {
        return Err(page::changed("刺猬猫目录书名不符"));
    }
    let mut chapters = Vec::new();
    for a in c.select(&page::selector(
        ".book-catalog .book-chapter-list > li > a[href]",
    )) {
        let url = Url::parse(a.value().attr("href").unwrap_or_default())
            .map_err(|_| page::changed("目录地址无效"))?;
        let id = url
            .path()
            .strip_prefix("/chapter/")
            .filter(|id| page::numeric(id))
            .ok_or_else(|| page::changed("章节身份无效"))?;
        if url.as_str() != format!("https://www.ciweimao.com/chapter/{id}") {
            return Err(page::changed("章节超出官网"));
        }
        chapters.push(ChapterRef {
            id: id.into(),
            title: a.text().collect::<String>().trim().into(),
            restricted: true,
        });
    }
    let latest = page::text(&c, ".book-catalog .update-info")?;
    let title_latest = latest
        .split_once("最新章节：")
        .and_then(|(_, s)| s.split_once("更新时间："))
        .map(|(s, _)| s.trim());
    if chapters.last().map(|c| c.title.as_str()) != title_latest {
        return Err(page::changed("刺猬猫目录未包含声明的最新章节"));
    }
    let result = Catalog {
        source: "ciweimao-web".into(),
        remote_id: book.into(),
        title,
        author: meta(&d, "og:novel:author")?,
        description: meta(&d, "og:description")?,
        chapters,
    };
    result.validate()?;
    Ok(result)
}
pub fn parse_faloo_catalog(html: &str, book: &str) -> Result<Catalog> {
    if !page::numeric(book) {
        return Err(page::changed("作品身份无效"));
    }
    let d = Html::parse_document(html);
    if page::one(&d, "#hidNovelID")?.value().attr("value") != Some(book) {
        return Err(page::changed("飞卢作品身份不符"));
    }
    let title = meta(&d, "og:title")?;
    let prefix = format!("/{book}_");
    let mut chapters = Vec::new();
    for a in d.select(&page::selector("#mulu a[href]")) {
        let url = Url::parse("https://b.faloo.com")
            .expect("static URL")
            .join(a.value().attr("href").unwrap_or_default())
            .map_err(|_| page::changed("飞卢目录地址无效"))?;
        if url.path().starts_with("/v_") {
            continue;
        }
        let id = url
            .path()
            .strip_prefix(&prefix)
            .and_then(|p| p.strip_suffix(".html"))
            .filter(|id| page::numeric(id))
            .ok_or_else(|| page::changed("飞卢目录混入其他作品"))?;
        if url.as_str() != format!("https://b.faloo.com/{book}_{id}.html")
            || id.parse::<u64>().ok().is_none_or(|n| {
                chapters
                    .last()
                    .is_some_and(|c: &ChapterRef| c.id.parse::<u64>().is_ok_and(|old| n <= old))
            })
        {
            return Err(page::changed("飞卢目录地址或章节顺序无效"));
        }
        let full = a
            .value()
            .attr("title")
            .and_then(|s| s.strip_prefix(&format!("{title}:")))
            .map(str::to_owned)
            .unwrap_or_else(|| a.text().collect::<String>());
        chapters.push(ChapterRef {
            id: id.into(),
            title: full.trim().into(),
            restricted: true,
        });
    }
    // The detail page repeats the latest chapter outside the catalog. This also
    // detects a missing tail. Remote IDs may have gaps after official removals;
    // they are identities, not positions and must never be synthesized.
    let latest = d
        .select(&page::selector("a[href]"))
        .filter_map(|a| {
            let url = Url::parse("https://b.faloo.com")
                .ok()?
                .join(a.value().attr("href")?)
                .ok()?;
            let id = url.path().strip_prefix(&prefix)?.strip_suffix(".html")?;
            if url.as_str() != format!("https://b.faloo.com/{book}_{id}.html") {
                return None;
            }
            id.parse::<usize>().ok()
        })
        .max();
    if latest != chapters.last().and_then(|c| c.id.parse::<usize>().ok()) || chapters.is_empty() {
        return Err(page::changed("飞卢目录未包含声明的最新章节"));
    }
    let result = Catalog {
        source: "faloo-web".into(),
        remote_id: book.into(),
        title,
        author: d
            .select(&page::selector(".authorInfo .box1 .zi1 a"))
            .next()
            .map(|e| e.text().collect::<String>().trim().to_owned())
            .unwrap_or_default(),
        description: meta(&d, "og:description")?,
        chapters,
    };
    result.validate()?;
    Ok(result)
}
