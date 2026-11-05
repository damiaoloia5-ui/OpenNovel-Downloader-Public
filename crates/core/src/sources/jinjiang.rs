use super::{Catalog, ChapterRef, SourceAdapter, page, web::WebClient};
use crate::{Error, ImportChapter, ImportDocument, Result};
use reqwest::Url;
use scraper::Html;
use std::{
    collections::HashMap,
    sync::{Mutex, atomic::AtomicBool},
};

type ObservedCatalog = (Catalog, HashMap<String, usize>);
pub struct Jinjiang {
    http: WebClient,
    catalogs: Mutex<HashMap<String, ObservedCatalog>>,
}
impl Jinjiang {
    pub fn new() -> Result<Self> {
        Ok(Self {
            http: WebClient::new(&["www.jjwxc.net"])?,
            catalogs: Mutex::new(HashMap::new()),
        })
    }
    pub fn search(&self, query: &str) -> Result<Vec<super::SearchResult>> {
        let query = query.trim();
        if query.is_empty() || query.chars().count() > 80 || query.chars().any(|c| c.is_control()) {
            return Err(Error::Invalid("搜索词须为 1～80 个可见字符".into()));
        }
        let (bytes, _, errors) = encoding_rs::GBK.encode(query);
        if errors {
            return Err(Error::Invalid("晋江官网搜索暂不支持该字符编码".into()));
        }
        let encoded = bytes
            .iter()
            .map(|b| format!("%{b:02X}"))
            .collect::<String>();
        let html = self.text(
            &format!("https://www.jjwxc.net/search.php?kw={encoded}&ord=&version=1&t=1"),
            &AtomicBool::new(false),
        )?;
        Self::parse_search(&html, query)
    }
    pub fn parse_search(html: &str, query: &str) -> Result<Vec<super::SearchResult>> {
        let doc = Html::parse_document(html);
        if !page::text(&doc, "title")?.contains(query) {
            return Err(page::changed("晋江搜索响应与关键词不符"));
        }
        let container = page::one(&doc, "#search_result")?;
        let mut results = Vec::new();
        let mut ids = std::collections::HashSet::new();
        for entry in container.select(&page::selector("#search_result > div")) {
            let Some(a) = entry.select(&page::selector("h3.title > a[href]")).next() else {
                continue;
            };
            let raw = a.value().attr("href").expect("href selector");
            let (source, id) = super::identify(raw)?;
            if source != "jinjiang-web" {
                return Err(page::changed("搜索结果超出晋江官网"));
            }
            let title = a.text().collect::<String>().trim().to_owned();
            let author = entry
                .select(&page::selector(
                    ".info > a[href*='oneauthor.php?authorid=']",
                ))
                .next()
                .map(|e| e.text().collect::<String>().trim().to_owned())
                .unwrap_or_default();
            if title.is_empty() || title.contains('\u{fffd}') || author.contains('\u{fffd}') {
                return Err(page::changed("搜索结果标题或作者编码无效"));
            }
            if ids.insert(id.clone()) {
                results.push(super::SearchResult {
                    source: source.into(),
                    input: format!("jinjiang:{id}"),
                    remote_id: id,
                    title,
                    author,
                });
            }
            if results.len() == 20 {
                break;
            }
        }
        Ok(results)
    }
    fn text(&self, url: &str, stop: &AtomicBool) -> Result<String> {
        let bytes = self.http.get(url, stop, 8 * 1024 * 1024)?;
        // The official desktop pages declare GB2312. Decode errors outside the selected content
        // (for example in comments) are tolerated; selected metadata/body must contain no replacement.
        let (text, _, _) = encoding_rs::GBK.decode(&bytes);
        Ok(text.into_owned())
    }
    pub fn inspect(&self, book: &str) -> Result<Catalog> {
        self.inspect_with_stop(book, &AtomicBool::new(false))
    }
    pub fn inspect_with_stop(&self, book: &str, stop: &AtomicBool) -> Result<Catalog> {
        if !page::numeric(book) {
            return Err(page::changed("晋江作品 ID 无效"));
        }
        let html = self.text(
            &format!("https://www.jjwxc.net/onebook.php?novelid={book}"),
            stop,
        )?;
        let catalog = Self::parse_catalog(&html, book)?;
        let doc = Html::parse_document(&html);
        let mut words = HashMap::new();
        for row in doc.select(&page::selector("tr[itemprop~='chapter']")) {
            let id = row
                .select(&page::selector("td"))
                .next()
                .expect("validated row")
                .text()
                .collect::<String>()
                .trim()
                .to_owned();
            let n = row
                .select(&page::selector("[itemprop='wordCount']"))
                .next()
                .ok_or_else(|| page::changed("晋江目录字数缺失"))?
                .text()
                .collect::<String>()
                .trim()
                .parse::<usize>()
                .map_err(|_| page::changed("晋江目录字数无效"))?;
            words.insert(id, n);
        }
        self.catalogs
            .lock()
            .map_err(|_| Error::Conflict("晋江目录缓存不可用".into()))?
            .insert(book.into(), (catalog.clone(), words));
        Ok(catalog)
    }
    pub fn parse_catalog(html: &str, book: &str) -> Result<Catalog> {
        if !page::numeric(book) {
            return Err(page::changed("晋江作品 ID 无效"));
        }
        let doc = Html::parse_document(html);
        let identities = doc
            .select(&page::selector("[data-novelid]"))
            .collect::<Vec<_>>();
        if identities.is_empty()
            || identities
                .iter()
                .any(|e| e.value().attr("data-novelid") != Some(book))
        {
            return Err(page::changed("晋江作品身份不一致"));
        }
        let mut chapters = Vec::new();
        for row in doc.select(&page::selector("tr[itemprop~='chapter']")) {
            let id = row
                .select(&page::selector("td"))
                .next()
                .ok_or_else(|| page::changed("晋江章节序号缺失"))?
                .text()
                .collect::<String>()
                .trim()
                .to_owned();
            if !page::numeric(&id) || id.parse::<usize>().ok() != Some(chapters.len() + 1) {
                return Err(page::changed("晋江完整目录顺序不连续"));
            }
            let headline = row
                .select(&page::selector("[itemprop='headline']"))
                .next()
                .ok_or_else(|| page::changed("晋江章节标题缺失"))?;
            let label = headline.text().collect::<String>();
            let locked = label.contains("[锁]");
            let vip = label.contains("[VIP]");
            let title = if locked {
                format!("第{id}章 [锁定]")
            } else {
                headline
                    .select(&page::selector("a[itemprop='url']"))
                    .next()
                    .ok_or_else(|| page::changed("晋江章节链接缺失"))?
                    .text()
                    .collect::<String>()
                    .trim()
                    .to_owned()
            };
            if !locked {
                let a = headline
                    .select(&page::selector("a[itemprop='url']"))
                    .next()
                    .expect("validated link");
                let raw = a
                    .value()
                    .attr(if vip { "rel" } else { "href" })
                    .ok_or_else(|| page::changed("晋江章节链接身份缺失"))?;
                let url = Url::parse(raw).map_err(|_| page::changed("晋江章节链接无效"))?;
                let host = if vip { "my.jjwxc.net" } else { "www.jjwxc.net" };
                let path = if vip {
                    "/onebook_vip.php"
                } else {
                    "/onebook.php"
                };
                if !matches!(url.scheme(), "http" | "https")
                    || url.host_str() != Some(host)
                    || url.port().is_some()
                    || !url.username().is_empty()
                    || url.password().is_some()
                    || url.path() != path
                    || url.fragment().is_some()
                {
                    return Err(page::changed("晋江章节链接超出官网范围"));
                }
                let pairs = url.query_pairs().collect::<Vec<_>>();
                if pairs.len() != 2
                    || !pairs.iter().any(|(k, v)| k == "novelid" && v == book)
                    || !pairs.iter().any(|(k, v)| k == "chapterid" && v == &id)
                {
                    return Err(page::changed("晋江章节链接与目录身份不符"));
                }
            }
            if title.contains('\u{fffd}') {
                return Err(page::changed("晋江标题编码错误"));
            }
            chapters.push(ChapterRef {
                id,
                title,
                restricted: locked || vip,
            });
        }
        let catalog = Catalog {
            source: "jinjiang-web".into(),
            remote_id: book.into(),
            title: page::text(&doc, "h1[itemprop='name']")?,
            author: page::text(&doc, "[itemprop='author']")?,
            description: page::text(&doc, "[itemprop='description']")?,
            chapters,
        };
        if [&catalog.title, &catalog.author, &catalog.description]
            .iter()
            .any(|s| s.contains('\u{fffd}'))
        {
            return Err(page::changed("晋江作品信息编码错误"));
        }
        catalog.validate()?;
        Ok(catalog)
    }
    pub fn parse_chapter(
        html: &str,
        book: &str,
        ch: &ChapterRef,
        words: usize,
    ) -> Result<ImportChapter> {
        if ch.restricted {
            return Err(Error::Restricted(
                "晋江 VIP 或锁定章节，不请求受限正文".into(),
            ));
        }
        let doc = Html::parse_document(html);
        for (name, expected) in [("novelid", book), ("chapterid", ch.id.as_str())] {
            if page::one(&doc, &format!("input[name='{name}']"))?
                .value()
                .attr("value")
                != Some(expected)
            {
                return Err(page::changed("晋江正文身份不符"));
            }
        }
        let title = page::text(&doc, ".noveltext h2")?;
        if title != ch.title {
            return Err(page::changed("晋江正文标题与目录不符"));
        }
        let body = page::one(&doc, ".noveltext > #paragraph_comment_content")?;
        let mut paragraphs = Vec::new();
        let mut current = String::new();
        for node in body.children() {
            if let Some(t) = node.value().as_text() {
                current.push_str(t);
            } else if let Some(e) = node.value().as_element() {
                if e.name() != "br" {
                    return Err(page::changed("晋江正文出现未知内容标签"));
                }
                if !current.trim().is_empty() {
                    paragraphs.push(current.trim().to_owned());
                    current.clear();
                }
            }
        }
        if !current.trim().is_empty() {
            paragraphs.push(current.trim().to_owned());
        }
        let actual = paragraphs.iter().map(|s| s.chars().count()).sum::<usize>();
        if words == 0
            || actual as u128 * 100 < words as u128 * 80
            || paragraphs.iter().any(|s| {
                s.contains('\u{fffd}') || s.chars().any(|c| ('\u{e000}'..='\u{f8ff}').contains(&c))
            })
        {
            return Err(page::changed("晋江正文编码或完整性校验失败"));
        }
        let input = ImportDocument {
            id: book.into(),
            title: "晋江正文校验".into(),
            author: String::new(),
            chapters: vec![ImportChapter {
                id: ch.id.clone(),
                title,
                paragraphs,
            }],
        };
        Ok(ImportDocument::parse(&serde_json::to_string(&input)?)?
            .chapters
            .remove(0))
    }
}
impl SourceAdapter for Jinjiang {
    fn source_id(&self) -> &str {
        "jinjiang-web"
    }
    fn fetch_chapter(
        &self,
        book: &str,
        ch: &ChapterRef,
        stop: &AtomicBool,
    ) -> Result<ImportChapter> {
        if ch.restricted {
            return Err(Error::Restricted("晋江 VIP 或锁定章节".into()));
        }
        let missing = !self
            .catalogs
            .lock()
            .map_err(|_| Error::Conflict("晋江目录缓存不可用".into()))?
            .contains_key(book);
        if missing {
            self.inspect_with_stop(book, stop)?;
        }
        let words = {
            let cache = self
                .catalogs
                .lock()
                .map_err(|_| Error::Conflict("晋江目录缓存不可用".into()))?;
            let (cat, words) = cache.get(book).expect("initialized catalog");
            let fresh = cat
                .chapters
                .iter()
                .find(|c| c.id == ch.id)
                .ok_or_else(|| page::changed("晋江章节不在当前目录"))?;
            if fresh.restricted {
                return Err(Error::Restricted("晋江最新目录标记受限".into()));
            }
            if fresh.title != ch.title {
                return Err(page::changed("晋江章节标题已变化，请检查更新"));
            }
            *words
                .get(&ch.id)
                .ok_or_else(|| page::changed("晋江字数缺失"))?
        };
        let html = self.text(
            &format!(
                "https://www.jjwxc.net/onebook.php?novelid={book}&chapterid={}",
                ch.id
            ),
            stop,
        )?;
        Self::parse_chapter(&html, book, ch, words)
    }
}
