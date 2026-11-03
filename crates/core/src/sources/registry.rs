use super::{Catalog, Fanqie, Jinjiang, PublicHtml, Qimao, Shuqi, SourceAdapter, page};
use crate::{Error, Result};
use reqwest::Url;
use serde::Serialize;
use std::sync::OnceLock;

#[derive(Debug, Clone, Serialize)]
pub struct SourceInfo {
    pub can_inspect: bool,
    pub id: &'static str,
    pub name: &'static str,
    pub can_download: bool,
    pub can_search: bool,
    pub note: &'static str,
    pub example: &'static str,
}
static SOURCES: [SourceInfo; 10] = [
    SourceInfo {
        id: "qidian-session",
        can_inspect: false,
        name: "起点当前可读章节",
        can_download: false,
        can_search: false,
        note: "在平台账户确认目录后自动逐章保存正常可读正文，支持暂停与继续；遇到限制即停止，真实已购全书仍待验收。",
        example: "https://www.qidian.com/book/作品ID/",
    },
    SourceInfo {
        id: "fanqie-web",
        can_inspect: true,
        name: "番茄官网网页",
        can_download: true,
        can_search: false,
        note: "官网开放正文；App 专属或受限章节单独标注。",
        example: "https://fanqienovel.com/page/作品ID",
    },
    SourceInfo {
        id: "qimao-web",
        can_inspect: true,
        name: "七猫官网网页",
        can_download: true,
        can_search: false,
        note: "仅保存官网网页开放正文；部分章节引导到官方 App 免费阅读，当前尚未接入 App 通道。",
        example: "https://www.qimao.com/shuku/作品ID/",
    },
    SourceInfo {
        id: "jinjiang-web",
        can_inspect: true,
        name: "晋江官网免费章节",
        can_download: true,
        can_search: true,
        note: "官网完整目录与公开免费正文；VIP、锁定章节明确受限，未接入已购正文。",
        example: "https://www.jjwxc.net/onebook.php?novelid=作品ID",
    },
    SourceInfo {
        id: "shuqi-web",
        can_inspect: true,
        name: "书旗官网免费章节",
        can_download: true,
        can_search: false,
        note: "保存官方阅读目录明确开放的免费正文；未开放、付费及会员章节不请求，尚未接入登录权益。",
        example: "https://www.shuqi.com/book/作品ID.html",
    },
    SourceInfo {
        id: "zongheng-web",
        name: "纵横官网免费章节",
        can_inspect: true,
        can_download: true,
        can_search: false,
        note: "实验性完整分卷目录与公开免费正文；VIP 跳过，保护或异常呈现停止。",
        example: "https://www.zongheng.com/detail/作品ID",
    },
    SourceInfo {
        id: "17k-web",
        name: "17K 官网免费章节",
        can_inspect: true,
        can_download: true,
        can_search: false,
        note: "实验性完整分卷目录与公开免费正文；VIP 不请求，不接入包月或已购权限。",
        example: "https://www.17k.com/book/作品ID.html",
    },
    SourceInfo {
        id: "ciweimao-web",
        name: "刺猬猫当前页（账户实验）",
        can_inspect: false,
        can_download: false,
        can_search: false,
        note: "桌面可打开独立官网会话，由本人登录后识别当前页；普通可见文字可单独存 TXT，图片呈现拒绝。完整目录、自动全书与已购实测未验收。",
        example: "https://www.ciweimao.com/book/作品ID",
    },
    SourceInfo {
        id: "faloo-web",
        name: "飞卢当前页（账户实验）",
        can_inspect: false,
        can_download: false,
        can_search: false,
        note: "桌面可本人登录后识别当前页，普通可见文字可单独存 TXT；不保证全章完整，不支持保护呈现。自动全书和已购实测未验收。",
        example: "https://b.faloo.com/作品ID.html",
    },
    SourceInfo {
        id: "migu-web",
        name: "咪咕悦看（账户识别实验）",
        can_inspect: false,
        can_download: false,
        can_search: false,
        note: "已找到可访问的咪咕悦看官网，可打开隔离登录窗口并识别作品页；阅读正文路由和已购尚未验证，当前不能保存正文。",
        example: "https://www.cmread.com/yuekan/novel/作品ID.html",
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
        ("zongheng:", "zongheng-web"),
        ("17k:", "17k-web"),
        ("ciweimao:", "ciweimao-web"),
        ("faloo:", "faloo-web"),
        ("migu:", "migu-web"),
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
        Some("www.zongheng.com" | "book.zongheng.com")
            if url.query().is_none() && url.fragment().is_none() =>
        {
            let id = url
                .path()
                .strip_prefix("/detail/")
                .map(|p| p.trim_end_matches('/'))
                .or_else(|| {
                    url.path()
                        .strip_prefix("/book/")
                        .and_then(|p| p.strip_suffix(".html"))
                })
                .or_else(|| {
                    url.path()
                        .strip_prefix("/showchapter/")
                        .and_then(|p| p.strip_suffix(".html"))
                });
            id.filter(|id| page::numeric(id))
                .map(|id| ("zongheng-web", id.into()))
                .ok_or_else(invalid_link)
        }
        Some("www.17k.com" | "www.ciweimao.com")
            if url.query().is_none() && url.fragment().is_none() =>
        {
            let seventeen = url.host_str() == Some("www.17k.com");
            let path = url.path().strip_prefix("/book/").ok_or_else(invalid_link)?;
            let id = if seventeen {
                path.strip_suffix(".html").ok_or_else(invalid_link)?
            } else {
                path.trim_end_matches('/')
            };
            if !page::numeric(id) {
                return Err(invalid_link());
            }
            Ok((
                if seventeen { "17k-web" } else { "ciweimao-web" },
                id.into(),
            ))
        }
        Some("b.faloo.com") if url.query().is_none() && url.fragment().is_none() => url
            .path()
            .strip_prefix('/')
            .and_then(|p| p.strip_suffix(".html"))
            .filter(|id| page::numeric(id))
            .map(|id| ("faloo-web", id.into()))
            .ok_or_else(invalid_link),
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
    Error::Invalid("请输入来源清单中的官网作品详情页或 来源:ID；待适配平台暂不开放下载".into())
}

#[derive(Default)]
pub struct SourceHub {
    fanqie: OnceLock<Fanqie>,
    qimao: OnceLock<Qimao>,
    shuqi: OnceLock<Shuqi>,
    jinjiang: OnceLock<Jinjiang>,
    zongheng: OnceLock<PublicHtml>,
    seventeen: OnceLock<PublicHtml>,
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
            "zongheng-web" => initialize(&self.zongheng, || PublicHtml::new(true))?.inspect(&id),
            "17k-web" => initialize(&self.seventeen, || PublicHtml::new(false))?.inspect(&id),
            "ciweimao-web" | "faloo-web" | "migu-web" => Err(Error::Restricted(
                SOURCES
                    .iter()
                    .find(|s| s.id == source)
                    .expect("known source")
                    .note
                    .into(),
            )),
            _ => Err(invalid_link()),
        }
    }
    pub fn adapter(&self, source: &str) -> Result<&dyn SourceAdapter> {
        match source {
            "fanqie-web" => Ok(initialize(&self.fanqie, Fanqie::new)?),
            "qimao-web" => Ok(initialize(&self.qimao, Qimao::new)?),
            "jinjiang-web" => Ok(initialize(&self.jinjiang, Jinjiang::new)?),
            "zongheng-web" => Ok(initialize(&self.zongheng, || PublicHtml::new(true))?),
            "17k-web" => Ok(initialize(&self.seventeen, || PublicHtml::new(false))?),
            "shuqi-web" => Ok(initialize(&self.shuqi, Shuqi::new)?),
            _ => Err(Error::Invalid("该来源没有已实现的下载适配器".into())),
        }
    }
}
