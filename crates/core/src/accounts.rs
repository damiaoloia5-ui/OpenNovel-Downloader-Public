use crate::{Error, Library, Result};
use reqwest::Url;
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

/// Non-secret local labels only. Browser sessions and credentials never enter SQLite.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AccountRecord {
    pub id: String,
    pub platform: String,
    pub label: String,
}
fn platform_host(platform: &str) -> Result<&'static str> {
    match platform {
        "qidian" => Ok("qidian.com"),
        "shuqi" => Ok("shuqi.com"),
        _ => Err(Error::Invalid("当前账户实验仅开放起点与书旗".into())),
    }
}
pub fn account_navigation_allowed(platform: &str, url: &Url) -> bool {
    let Ok(host) = platform_host(platform) else {
        return false;
    };
    url.scheme() == "https"
        && url.port().is_none()
        && url.username().is_empty()
        && url.password().is_none()
        && url
            .host_str()
            .is_some_and(|h| h == host || h.ends_with(&format!(".{host}")))
}
/// Only accept clean book links at entry; tokens and arbitrary URLs are not persisted.
pub fn account_entry_url(platform: &str, input: &str) -> Result<Url> {
    let host = platform_host(platform)?;
    if input.trim().is_empty() {
        return Url::parse(&format!("https://www.{host}/"))
            .map_err(|_| Error::Invalid("平台地址无效".into()));
    }
    let url = Url::parse(input.trim()).map_err(|_| Error::Invalid("请输入官网作品链接".into()))?;
    let path = url.path().trim_end_matches('/');
    let id = path.strip_prefix("/book/").and_then(|p| {
        if platform == "shuqi" {
            p.strip_suffix(".html")
        } else {
            Some(p)
        }
    });
    if !account_navigation_allowed(platform, &url)
        || url.host_str() != Some(&format!("www.{host}"))
        || url.query().is_some()
        || url.fragment().is_some()
        || !id.is_some_and(|id| {
            !id.is_empty() && id.len() <= 30 && id.bytes().all(|c| c.is_ascii_digit())
        })
    {
        return Err(Error::Invalid(
            "只接受当前平台不含登录参数的官网作品链接".into(),
        ));
    }
    Ok(url)
}
impl Library {
    pub fn accounts(&self) -> Result<Vec<AccountRecord>> {
        Ok(self
            .connection()?
            .prepare("SELECT id,platform,label FROM accounts ORDER BY rowid")?
            .query_map([], |r| {
                Ok(AccountRecord {
                    id: r.get(0)?,
                    platform: r.get(1)?,
                    label: r.get(2)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?)
    }
    pub fn account(&self, id: &str) -> Result<AccountRecord> {
        self.connection()?
            .query_row(
                "SELECT id,platform,label FROM accounts WHERE id=?1",
                [id],
                |r| {
                    Ok(AccountRecord {
                        id: r.get(0)?,
                        platform: r.get(1)?,
                        label: r.get(2)?,
                    })
                },
            )
            .optional()?
            .ok_or_else(|| Error::Invalid("账户记录不存在".into()))
    }
    pub fn create_account(&self, platform: &str, label: &str) -> Result<AccountRecord> {
        platform_host(platform)?;
        let label = label.trim();
        if label.is_empty() || label.chars().count() > 40 || label.chars().any(|c| c.is_control()) {
            return Err(Error::Invalid(
                "本地别名应为 1～40 个可见字符，请勿填写密码或令牌".into(),
            ));
        }
        let mut db = self.connection()?;
        let tx = db.transaction()?;
        let count: u32 = tx.query_row("SELECT COUNT(*) FROM accounts", [], |r| r.get(0))?;
        if count >= 10 {
            return Err(Error::Conflict("最多保存 10 个本地账户记录".into()));
        }
        let account = AccountRecord {
            id: uuid::Uuid::new_v4().to_string(),
            platform: platform.into(),
            label: label.into(),
        };
        tx.execute(
            "INSERT INTO accounts(id,platform,label) VALUES(?1,?2,?3)",
            params![account.id, account.platform, account.label],
        )?;
        tx.commit()?;
        Ok(account)
    }
    pub fn remove_account(&self, id: &str) -> Result<()> {
        if self
            .connection()?
            .execute("DELETE FROM accounts WHERE id=?1", [id])?
            != 1
        {
            return Err(Error::Invalid("账户记录不存在".into()));
        }
        Ok(())
    }
}
