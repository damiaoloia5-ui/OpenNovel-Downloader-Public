//! Read-only official-page snapshots. No cookies, credentials, OCR or site script execution.
use crate::{Error, ImportChapter, ImportDocument, Result};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use std::{io::Write, path::Path};

fn numeric_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 24
        && !value.starts_with('0')
        && value.bytes().all(|c| c.is_ascii_digit())
}

pub fn official_platform_name(platform: &str) -> Option<&'static str> {
    match platform {
        "qidian" => Some("起点"),
        "shuqi" => Some("书旗"),
        "ciweimao" => Some("刺猬猫"),
        "faloo" => Some("飞卢"),
        "migu" => Some("咪咕悦看"),
        _ => None,
    }
}
pub fn official_book_link(platform: &str, input: &str) -> Result<Url> {
    let url = Url::parse(input.trim())
        .map_err(|_| Error::Invalid("请输入当前平台官网作品链接".into()))?;
    if url.scheme() != "https"
        || url.port().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(Error::Invalid("作品链接不能包含登录参数或凭据".into()));
    }
    let id = match (platform, url.host_str()) {
        ("ciweimao", Some("www.ciweimao.com")) => {
            url.path().trim_end_matches('/').strip_prefix("/book/")
        }
        ("faloo", Some("b.faloo.com")) => url
            .path()
            .strip_prefix('/')
            .and_then(|p| p.strip_suffix(".html")),
        ("migu", Some("www.cmread.com")) => url
            .path()
            .strip_prefix("/yuekan/novel/")
            .and_then(|p| p.strip_suffix(".html")),
        _ => None,
    };
    if !id.is_some_and(numeric_id) {
        return Err(Error::Invalid("当前平台作品链接尚不支持".into()));
    }
    Ok(url)
}
pub fn official_login_url(platform: &str) -> Result<Url> {
    let url = match platform {
        "ciweimao" => "https://www.ciweimao.com/signup/login",
        "faloo" => "https://u.faloo.com/regist/login.aspx",
        "migu" => "https://passport.migu.cn/login?sourceid=204001",
        _ => return Err(Error::Invalid("该平台不使用此登录流程".into())),
    };
    Ok(Url::parse(url).expect("static official login URL"))
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OfficialPage {
    pub platform: String,
    pub url: String,
    pub status: String,
    pub book_id: String,
    pub chapter_id: String,
    pub book_title: String,
    pub chapter_title: String,
    pub paragraphs: Vec<String>,
    pub declared_words: Option<u32>,
}
impl OfficialPage {
    pub fn parse(raw: &str, platform: &str, expected: &str) -> Result<Self> {
        if raw.len() > crate::MAX_CAPTURE_BYTES {
            return Err(Error::Invalid("页面超过读取上限".into()));
        }
        let page: Self = serde_json::from_str(raw)
            .map_err(|_| Error::SourceChanged("官网识别结果无效".into()))?;
        let actual = Url::parse(&page.url).map_err(|_| Error::Invalid("页面地址无效".into()))?;
        let expected = Url::parse(expected).map_err(|_| Error::Invalid("页面地址无效".into()))?;
        if page.platform != platform
            || actual != expected
            || actual.query().is_some()
            || actual.fragment().is_some()
            || !crate::account_navigation_allowed(platform, &actual)
            || !["ciweimao", "faloo", "migu"].contains(&platform)
        {
            return Err(Error::Conflict("平台或页面已变化，请重新识别".into()));
        }
        if ![
            "text",
            "book",
            "restricted",
            "protected",
            "verification",
            "unsupported",
            "loading",
        ]
        .contains(&page.status.as_str())
        {
            return Err(Error::SourceChanged("未知页面状态".into()));
        }
        if page.status != "text" && !page.paragraphs.is_empty() {
            return Err(Error::SourceChanged("受限状态不能携带正文".into()));
        }
        if page.book_title.chars().count() > 500
            || page.chapter_title.chars().count() > 500
            || page.book_id.len() > 24
            || page.chapter_id.len() > 24
        {
            return Err(Error::Invalid("页面字段超过限制".into()));
        }
        if page.status == "text" {
            page.validate_text()?;
        }
        Ok(page)
    }
    fn validate_text(&self) -> Result<()> {
        let url = Url::parse(&self.url).map_err(|_| Error::Invalid("正文地址无效".into()))?;
        if !crate::account_navigation_allowed(&self.platform, &url)
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(Error::Invalid("正文地址不能包含凭据或登录参数".into()));
        }
        if !numeric_id(&self.book_id) || !numeric_id(&self.chapter_id) {
            return Err(Error::Invalid("章节身份无效".into()));
        }
        let valid = match self.platform.as_str() {
            "ciweimao" => {
                url.host_str() == Some("www.ciweimao.com")
                    && url.path() == format!("/chapter/{}", self.chapter_id)
            }
            "faloo" => {
                url.host_str() == Some("b.faloo.com")
                    && url.path() == format!("/{}_{}.html", self.book_id, self.chapter_id)
            }
            // No verified Migu reader layout yet. Do not guess a reader route.
            _ => false,
        };
        if !valid {
            return Err(Error::SourceChanged("当前平台正文路由尚未验证".into()));
        }
        crate::domain::validate_label(&self.book_title, "书名", 500)?;
        let d = ImportDocument {
            id: self.book_id.clone(),
            title: self.book_title.clone(),
            author: String::new(),
            chapters: vec![ImportChapter {
                id: self.chapter_id.clone(),
                title: self.chapter_title.clone(),
                paragraphs: self.paragraphs.clone(),
            }],
        }
        .validated()?;
        let paragraphs = &d.chapters[0].paragraphs;
        let count: usize = paragraphs
            .iter()
            .map(|p| p.chars().filter(|c| !c.is_whitespace()).count())
            .sum();
        if paragraphs.iter().any(|p| {
            p.chars()
                .any(|c| matches!(c, '\u{e000}'..='\u{f8ff}' | '\u{fffd}'))
                || [
                    "订阅本章",
                    "试读结束",
                    "购买本章",
                    "解锁本章",
                    "本章为付费章节",
                ]
                .iter()
                .any(|s| p.contains(s))
        }) {
            return Err(Error::Restricted("正文含限制提示或未知字形".into()));
        }
        if let Some(words) = self.declared_words
            && (words == 0 || count * 100 < words as usize * 80)
        {
            return Err(Error::Restricted("正文与官网声明长度不符，未保存".into()));
        }
        Ok(())
    }
    pub fn save_txt(&self, directory: &Path) -> Result<String> {
        if self.status != "text" {
            return Err(Error::Restricted("当前页面没有可保存的普通文字".into()));
        }
        self.validate_text()?;
        std::fs::create_dir_all(directory)?;
        let name = format!(
            "{}-{}-{}-visible-{}.txt",
            self.platform,
            self.book_id,
            self.chapter_id,
            uuid::Uuid::new_v4()
        );
        let path = directory.join(name);
        let mut file = tempfile::NamedTempFile::new_in(directory)?;
        writeln!(
            file,
            "{}\n{}\n来源：{}\n当前页可见文字采集；未核验全章或全书完整性。\n",
            self.book_title, self.chapter_title, self.url
        )?;
        for paragraph in &self.paragraphs {
            writeln!(file, "{}\n", paragraph.trim())?;
        }
        file.flush()?;
        file.as_file().sync_all()?;
        file.persist_noclobber(&path)
            .map_err(|e| Error::Io(e.error))?;
        Ok(path.to_string_lossy().into_owned())
    }
}
