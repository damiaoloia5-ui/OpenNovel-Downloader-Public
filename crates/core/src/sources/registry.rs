use super::{Catalog, Fanqie, Jinjiang, Qimao, Shuqi, SourceAdapter, page};
use crate::{Error, Result};
use reqwest::Url;
use serde::Serialize;
use std::sync::OnceLock;

#[derive(Debug, Serialize)]
pub struct SourceInfo {
    pub id: &'static str,
    pub name: &'static str,
    pub can_download: bool,
    pub can_search: bool,
    pub note: &'static str,
    pub example: &'static str,
}
static SOURCES: [SourceInfo; 5] = [
    SourceInfo {
        id: "qidian-session",
        name: "起点当前可读章节",
        can_download: false,
        can_search: false,
        note: "请在平台账户读取完整目录，逐章预览并确认保存；无自动批量下载，真实已购场景待验收。",
        example: "https://www.qidian.com/book/作品ID/",
    },
    SourceInfo {
        id: "fanqie-web",
        name: "番茄官网网页",
        can_download: true,
        can_search: false,
        note: "官网开放正文；App 专属或受限章节单独标注。",
        example: "https://fanqienovel.com/page/作品ID",
    },
    SourceInfo {
        id: "qimao-web",
        name: "七猫官网网页",
        can_download: true,
        can_search: false,
        note: "仅目录明确免订阅的完整网页正文；VIP 标记不按 App 广告免费解释。",
        example: "https://www.qimao.com/shuku/作品ID/",
    },
    SourceInfo {
        id: "jinjiang-web",
        name: "晋江官网免费章节",
        can_download: true,
        can_search: true,
        note: "官网完整目录与公开免费正文；VIP、锁定章节明确受限，未接入已购正文。",
        example: "https://www.jjwxc.net/onebook.php?novelid=作品ID",
    },
    SourceInfo {
        id: "shuqi-web",
        name: "书旗官网目录",
        can_download: false,
        can_search: false,
        note: "详情与完整目录已接入；正文实测限流，尚未通过验收，暂不开放下载。",
        example: "https://www.shuqi.com/book/作品ID.html",
    },
];
pub fn source_infos() -> &'static [SourceInfo] {
    &SOURCES
}
pub fn download_supported(source: &str) -> bool {
    SOURCES.iter().any(|s| s.id == source && s.can_download)
}

pub fn identify(input: &str) -> Result<(&'static str, String)> {
    let input = input.trim();
    for (prefix, source) in [
        ("fanqie:", "fanqie-web"),
        ("qimao:", "qimao-web"),
        ("shuqi:", "shuqi-web"),
        ("jinjiang:", "jinjiang-web"),
    ] {
        if let Some(id) = input.strip_prefix(prefix) {
            if page::numeric(id) && (source != "fanqie-web" || Fanqie::recognize(id).is_ok()) {
                return Ok((source, id.to_owned()));
            }
            return Err(Error::Invalid("来源 ID 无效".into()));
        }
    }
    if input.bytes().all(|c| c.is_ascii_digit()) {
        return Fanqie::recognize(input).map(|id| ("fanqie-web", id));
    }
    let url = Url::parse(input).map_err(|_| invalid_link())?;
    if !matches!(url.scheme(), "http" | "https")
        || url.port().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(invalid_link());
    }
    match url.host_str() {
        Some("www.jjwxc.net") if url.path() == "/onebook.php" && url.fragment().is_none() => {
            let pairs = url.query_pairs().collect::<Vec<_>>();
            if pairs.len() != 1 || pairs[0].0 != "novelid" || !page::numeric(&pairs[0].1) {
                return Err(invalid_link());
            }
            Ok(("jinjiang-web", pairs[0].1.to_string()))
        }
        Some("fanqienovel.com" | "www.fanqienovel.com") => {
            Fanqie::recognize(input).map(|id| ("fanqie-web", id))
        }
        Some("qimao.com" | "www.qimao.com") => url
            .path()
            .trim_end_matches('/')
            .strip_prefix("/shuku/")
            .filter(|id| page::numeric(id))
            .map(|id| ("qimao-web", id.to_owned()))
            .ok_or_else(invalid_link),
        Some("shuqi.com" | "www.shuqi.com") => url
            .path()
            .strip_prefix("/book/")
            .and_then(|p| p.strip_suffix(".html"))
            .filter(|id| page::numeric(id))
            .map(|id| ("shuqi-web", id.to_owned()))
            .ok_or_else(invalid_link),
        _ => Err(invalid_link()),
    }
}
fn invalid_link() -> Error {
    Error::Invalid(
        "请输入已接入平台的官网作品详情页；也可使用 fanqie:ID、qimao:ID、shuqi:ID 或 jinjiang:ID"
            .into(),
    )
}

#[derive(Default)]
pub struct SourceHub {
    fanqie: OnceLock<Fanqie>,
    qimao: OnceLock<Qimao>,
    shuqi: OnceLock<Shuqi>,
    jinjiang: OnceLock<Jinjiang>,
}
fn initialize<T>(cell: &OnceLock<T>, create: impl FnOnce() -> Result<T>) -> Result<&T> {
    if cell.get().is_none() {
        let _ = cell.set(create()?);
    }
    Ok(cell.get().expect("initialized source"))
}
impl SourceHub {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn search(&self, source: &str, query: &str) -> Result<Vec<super::SearchResult>> {
        match source {
            "jinjiang-web" => initialize(&self.jinjiang, Jinjiang::new)?.search(query),
            _ => Err(Error::Invalid("该来源尚未通过站内搜索验收".into())),
        }
    }
    pub fn inspect(&self, input: &str) -> Result<Catalog> {
        let (source, id) = identify(input)?;
        match source {
            "fanqie-web" => initialize(&self.fanqie, Fanqie::new)?.inspect(&id),
            "qimao-web" => initialize(&self.qimao, Qimao::new)?.inspect(&id),
            "shuqi-web" => initialize(&self.shuqi, Shuqi::new)?.inspect(&id),
            "jinjiang-web" => initialize(&self.jinjiang, Jinjiang::new)?.inspect(&id),
            _ => Err(invalid_link()),
        }
    }
    pub fn adapter(&self, source: &str) -> Result<&dyn SourceAdapter> {
        match source {
            "fanqie-web" => Ok(initialize(&self.fanqie, Fanqie::new)?),
            "qimao-web" => Ok(initialize(&self.qimao, Qimao::new)?),
            "jinjiang-web" => Ok(initialize(&self.jinjiang, Jinjiang::new)?),
            "shuqi-web" => Err(Error::Restricted(
                "书旗当前仅支持详情与目录，正文尚未通过实测验收，暂不开放下载".into(),
            )),
            _ => Err(Error::Invalid("该来源没有已实现的下载适配器".into())),
        }
    }
}
