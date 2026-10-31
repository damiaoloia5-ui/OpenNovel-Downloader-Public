use super::{Catalog, ChapterRef, SourceAdapter, page, web::WebClient};
use crate::{Error, ImportChapter, ImportDocument, Result};
use reqwest::Url;
use scraper::Html;
use std::sync::atomic::AtomicBool;

/// Public desktop pages only; each instance has an independent domain allowlist.
pub struct PublicHtml {
    zongheng: bool,
    http: WebClient,
}
fn digits_after(text: &str, prefix: &str) -> Result<usize> {
    let value = text
        .split_once(prefix)
        .ok_or_else(|| page::changed("官网声明数量缺失"))?
        .1
        .trim_start();
    page::count(
        &value
            .chars()
            .take_while(char::is_ascii_digit)
            .collect::<String>(),
    )
}
fn field(html: &str, name: &str) -> Result<String> {
    let doc = Html::parse_document(html);
    let scripts = doc
        .select(&page::selector("script"))
        .filter_map(|s| {
            s.text()
                .collect::<String>()
                .split_once("window.$__BOOK = {")
                .map(|(_, rest)| rest.split('}').next().unwrap_or_default().to_owned())
        })
        .collect::<Vec<_>>();
    if scripts.len() != 1 {
        return Err(page::changed("17K 页面身份声明缺失或重复"));
    }
    let values = scripts[0]
        .split(',')
        .filter_map(|s| s.split_once(':'))
        .filter(|(key, _)| key.trim() == name)
        .map(|(_, value)| value.trim().to_owned())
        .collect::<Vec<_>>();
    if values.len() != 1 {
        return Err(page::changed("17K 页面身份字段无效"));
    }
    Ok(values[0].clone())
}
impl PublicHtml {
    pub fn new(zongheng: bool) -> Result<Self> {
        Ok(Self {
            zongheng,
            http: WebClient::new(if zongheng {
                &["book.zongheng.com", "read.zongheng.com"]
            } else {
                &["www.17k.com"]
            })?,
        })
    }
    pub fn inspect(&self, book: &str) -> Result<Catalog> {
        if !page::numeric(book) {
            return Err(page::changed("作品 ID 无效"));
        }
        let url = if self.zongheng {
            format!("https://book.zongheng.com/showchapter/{book}.html")
        } else {
            format!("https://www.17k.com/list/{book}.html")
        };
        let html = self
            .http
            .text(&url, &AtomicBool::new(false), 8 * 1024 * 1024)?;
        Self::parse_catalog(&html, book, self.zongheng)
    }
    pub fn parse_catalog(html: &str, book: &str, zongheng: bool) -> Result<Catalog> {
        if !page::numeric(book) {
            return Err(page::changed("作品 ID 无效"));
        }
        let doc = Html::parse_document(html);
        let (title, author, counts, links) = if zongheng {
            (
                page::text(&doc, ".book-meta h1")?,
                page::text(&doc, ".book-meta p span:first-child a")?,
                ".count",
                ".chapter-list li a[href]",
            )
        } else {
            if field(html, "bookId")? != book || field(html, "type")? != "'list'" {
                return Err(page::changed("17K 目录作品身份不符"));
            }
            (
                page::text(&doc, "h1.Title")?,
                page::text(&doc, ".Author a")?,
                "dl.Volume > dt .info",
                "dl.Volume > dd > a[href]",
            )
        };
        let declared = doc
            .select(&page::selector(counts))
            .map(|e| {
                digits_after(
                    &e.text().collect::<String>(),
                    if zongheng { "共" } else { "已更新" },
                )
            })
            .collect::<Result<Vec<_>>>()?;
        if declared.is_empty() {
            return Err(page::changed("分卷章节总数缺失"));
        }
        let mut chapters = Vec::new();
        for a in doc.select(&page::selector(links)) {
            let base = if zongheng {
                "https://book.zongheng.com"
            } else {
                "https://www.17k.com"
            };
            let url = Url::parse(base)
                .expect("static URL")
                .join(a.value().attr("href").unwrap_or_default())
                .map_err(|_| page::changed("章节地址无效"))?;
            let prefix = format!("/chapter/{book}/");
            let id = url
                .path()
                .strip_prefix(&prefix)
                .and_then(|s| s.strip_suffix(".html"))
                .filter(|s| page::numeric(s))
                .ok_or_else(|| page::changed("目录含其他作品或无效章节"))?;
            if url.scheme() != "https"
                || ![if zongheng {
                    "book.zongheng.com"
                } else {
                    "www.17k.com"
                }]
                .contains(&url.host_str().unwrap_or_default())
                || !url.username().is_empty()
                || url.password().is_some()
                || url.port().is_some()
                || url.query().is_some()
                || url.fragment().is_some()
            {
                return Err(page::changed("章节链接超出官网范围"));
            }
            let restricted = if zongheng {
                a.parent()
                    .and_then(scraper::ElementRef::wrap)
                    .is_some_and(|p| {
                        p.value()
                            .has_class("vip", scraper::CaseSensitivity::CaseSensitive)
                    })
            } else {
                a.select(&page::selector("span.vip")).next().is_some()
            };
            chapters.push(ChapterRef {
                id: id.into(),
                title: a.text().collect::<String>().trim().into(),
                restricted,
            });
        }
        if chapters.len() != declared.iter().sum::<usize>() {
            return Err(page::changed("分卷数量与实际目录不符，拒绝保存不完整目录"));
        }
        // Check every volume separately; equal global totals must not hide a truncated volume.
        let groups = doc
            .select(&page::selector(if zongheng {
                ".chapter-list"
            } else {
                "dl.Volume > dd"
            }))
            .collect::<Vec<_>>();
        if groups.len() != declared.len()
            || groups
                .iter()
                .zip(&declared)
                .any(|(g, n)| g.select(&page::selector("a[href]")).count() != *n)
        {
            return Err(page::changed("分卷目录不完整"));
        }
        let catalog = Catalog {
            source: if zongheng { "zongheng-web" } else { "17k-web" }.into(),
            remote_id: book.into(),
            title,
            author,
            description: String::new(),
            chapters,
        };
        catalog.validate()?;
        Ok(catalog)
    }
    pub fn parse_chapter(
        html: &str,
        book: &str,
        chapter: &ChapterRef,
        zongheng: bool,
    ) -> Result<ImportChapter> {
        if chapter.restricted {
            return Err(Error::Restricted("目录标记该章需订阅，不请求正文".into()));
        }
        let doc = Html::parse_document(html);
        let (title, info, body) = if zongheng {
            let owner = page::one(&doc, "body[data-bookid][data-chapterid][data-vip]")?;
            if owner.value().attr("data-bookid") != Some(book)
                || owner.value().attr("data-chapterid") != Some(chapter.id.as_str())
            {
                return Err(page::changed("纵横章节身份不符"));
            }
            if owner.value().attr("data-vip") != Some("0")
                || owner.value().attr("data-ec") != Some("0")
            {
                return Err(Error::Restricted("纵横章节受限或使用保护呈现".into()));
            }
            let content = page::one(&doc, "#Jcontent .reader-box .content")?;
            if content.value().attr("data-decoder") != Some("")
                || content
                    .select(&page::selector("canvas, img, script, iframe, [style]"))
                    .next()
                    .is_some()
            {
                return Err(Error::Restricted("正文含保护内容或未知呈现".into()));
            }
            (
                page::text(&doc, "#Jcontent .title_txtbox")?,
                page::text(&doc, "#Jcontent .bookinfo")?,
                content,
            )
        } else {
            if field(html, "bookId")? != book
                || field(html, "chapterId")? != chapter.id
                || field(html, "type")? != "'chapter'"
            {
                return Err(page::changed("17K 章节身份不符"));
            }
            if field(html, "vip")? != "0" {
                return Err(Error::Restricted("17K 章节需订阅".into()));
            }
            (
                page::text(&doc, "#readArea .readAreaBox > h1")?,
                page::text(&doc, "#readArea .chapter_update_time")?,
                page::one(&doc, "#readArea .readAreaBox > .p")?,
            )
        };
        if title.split_whitespace().collect::<String>()
            != chapter.title.split_whitespace().collect::<String>()
        {
            return Err(page::changed("章节标题与目录不符"));
        }
        let mut paragraphs = Vec::new();
        for element in body.children().filter_map(scraper::ElementRef::wrap) {
            // 17K places its copyright footer and author notes in the same wrapper.
            // Neither is chapter prose; scripts in notes are never evaluated.
            if !zongheng
                && ((element.value().name() == "p"
                    && element
                        .value()
                        .has_class("copy", scraper::CaseSensitivity::CaseSensitive))
                    || (element.value().name() == "div"
                        && (element.value().attr("id") == Some("banner_content")
                            || ["author-say", "qrcode", "chapter_text_ad"]
                                .iter()
                                .any(|class| {
                                    element
                                        .value()
                                        .has_class(class, scraper::CaseSensitivity::CaseSensitive)
                                }))))
            {
                continue;
            }
            if element.value().name() != "p"
                || element.value().attr("style").is_some()
                || element
                    .select(&page::selector(
                        "script, iframe, canvas, img, [style], .vip, .subscribe",
                    ))
                    .next()
                    .is_some()
            {
                return Err(Error::Restricted("正文含未知呈现或订阅提示".into()));
            }
            let paragraph = element.text().collect::<String>().trim().to_owned();
            if !paragraph.is_empty() {
                paragraphs.push(paragraph);
            }
        }
        let words = digits_after(
            &info,
            if zongheng {
                "本章字数："
            } else {
                "本章共"
            },
        )?;
        let actual = paragraphs.iter().map(|p| p.chars().count()).sum::<usize>();
        if actual * 100 < words * 80
            || actual * 100 > words * 150
            || paragraphs.iter().any(|p| {
                p.chars()
                    .any(|c| c == '\u{fffd}' || ('\u{e000}'..='\u{f8ff}').contains(&c))
            })
        {
            return Err(page::changed("正文长度或编码异常，拒绝保存截断预览"));
        }
        let document = ImportDocument {
            id: book.into(),
            title: "官网正文校验".into(),
            author: String::new(),
            chapters: vec![ImportChapter {
                id: chapter.id.clone(),
                title,
                paragraphs,
            }],
        };
        Ok(ImportDocument::parse(&serde_json::to_string(&document)?)?
            .chapters
            .remove(0))
    }
}
impl SourceAdapter for PublicHtml {
    fn source_id(&self) -> &str {
        if self.zongheng {
            "zongheng-web"
        } else {
            "17k-web"
        }
    }
    fn fetch_chapter(
        &self,
        book: &str,
        chapter: &ChapterRef,
        stop: &AtomicBool,
    ) -> Result<ImportChapter> {
        if chapter.restricted {
            return Err(Error::Restricted("目录标记需订阅，跳过正文请求".into()));
        }
        if !page::numeric(book) || !page::numeric(&chapter.id) {
            return Err(page::changed("章节 ID 无效"));
        }
        let host = if self.zongheng {
            "read.zongheng.com"
        } else {
            "www.17k.com"
        };
        let html = self.http.text(
            &format!("https://{host}/chapter/{book}/{}.html", chapter.id),
            stop,
            2 * 1024 * 1024,
        )?;
        Self::parse_chapter(&html, book, chapter, self.zongheng)
    }
}
