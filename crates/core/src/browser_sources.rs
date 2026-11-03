//! Identity and routing for normal, user-owned official browser sessions.
use crate::{Error, Result};
use reqwest::Url;

pub fn session_source(platform: &str) -> Result<&'static str> {
    match platform {
        "qidian" => Ok("qidian-session"),
        "zongheng" => Ok("zongheng-session"),
        "17k" => Ok("17k-session"),
        "jinjiang" => Ok("jinjiang-session"),
        "qimao" => Ok("qimao-session"),
        "shuqi" => Ok("shuqi-session"),
        "ciweimao" => Ok("ciweimao-session"),
        "faloo" => Ok("faloo-session"),
        _ => Err(Error::Invalid("该平台尚无可验证的官网连续阅读通道".into())),
    }
}
pub fn session_platform(source: &str) -> Option<&str> {
    source
        .strip_suffix("-session")
        .filter(|p| session_source(p).is_ok())
}
pub fn session_chapter_url(platform: &str, book: &str, chapter: &str) -> Result<String> {
    if [book, chapter]
        .iter()
        .any(|s| s.is_empty() || s.len() > 30 || !s.bytes().all(|b| b.is_ascii_digit()))
    {
        return Err(Error::Invalid("官网作品或章节身份无效".into()));
    }
    Ok(match platform {
        "qidian" => format!("https://www.qidian.com/chapter/{book}/{chapter}/"),
        "zongheng" => format!("https://read.zongheng.com/chapter/{book}/{chapter}.html"),
        "17k" => format!("https://www.17k.com/chapter/{book}/{chapter}.html"),
        "jinjiang" => {
            format!("https://my.jjwxc.net/onebook_vip.php?novelid={book}&chapterid={chapter}")
        }
        "qimao" => format!("https://www.qimao.com/shuku/{book}-{chapter}/"),
        "shuqi" => format!("https://www.shuqi.com/reader?bid={book}&cid={chapter}"),
        "ciweimao" => format!("https://www.ciweimao.com/chapter/{chapter}"),
        "faloo" => format!("https://b.faloo.com/{book}_{chapter}.html"),
        _ => return Err(Error::Invalid("该平台正文地址未验证".into())),
    })
}
/// Preserve identity queries only; session tokens never enter snapshots or SQLite.
pub fn clean_session_url(platform: &str, input: &str) -> Result<String> {
    let mut u = Url::parse(input).map_err(|_| Error::Invalid("官网地址无效".into()))?;
    if !crate::account_navigation_allowed(platform, &u) {
        return Err(Error::Restricted("页面已离开当前平台官网".into()));
    }
    let keys: &[&str] = match platform {
        "shuqi" => &["bid", "cid"],
        "jinjiang" => &["novelid", "chapterid"],
        _ => &[],
    };
    let pairs = u
        .query_pairs()
        .filter(|(k, _)| keys.contains(&k.as_ref()))
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect::<Vec<_>>();
    if pairs.iter().any(|(k, v)| {
        pairs.iter().filter(|(key, _)| key == k).count() != 1
            || v.is_empty()
            || v.len() > 30
            || !v.bytes().all(|b| b.is_ascii_digit())
    }) {
        return Err(Error::Invalid("官网身份参数重复或无效".into()));
    }
    u.set_query(None);
    u.set_fragment(None);
    if !pairs.is_empty() {
        let mut q = u.query_pairs_mut();
        for key in keys {
            if let Some((k, v)) = pairs.iter().find(|(k, _)| k == key) {
                q.append_pair(k, v);
            }
        }
    }
    Ok(u.into())
}
