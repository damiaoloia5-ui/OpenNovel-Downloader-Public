//! Explicit, single-request protocol research. Not a registered download source.
use crate::{Error, Result};
use reqwest::{Url, blocking::Client};
use scraper::{Html, Selector};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{io::Read, time::Duration};

const MAX_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug, Serialize)]
pub struct OfficialProbeReport {
    pub channel: String,
    pub endpoint: String,
    pub http_status: u16,
    pub bytes: usize,
    pub sha256: String,
    pub outcome: String,
    pub identity_matches: Option<bool>,
    pub declared_characters: Option<u64>,
    pub observed_characters: Option<usize>,
    pub evidence: String,
}

fn identity(value: &str) -> bool {
    (10..=24).contains(&value.len())
        && !value.starts_with('0')
        && value.bytes().all(|c| c.is_ascii_digit())
}

/// Fixed official origins only. Never accepts credentials, arbitrary hosts or URL parameters.
fn target(channel: &str, book: &str, chapter: &str) -> Result<Url> {
    if !identity(book) || !identity(chapter) {
        return Err(Error::Invalid("作品和章节 ID 必须是 10～24 位数字".into()));
    }
    let mut url = match channel {
        "web-api" => Url::parse("https://fanqienovel.com/api/reader/full"),
        "app-content" => Url::parse("https://novel.snssdk.com/api/novel/book/reader/content/v1"),
        _ => return Err(Error::Invalid("原型仅支持 web-api 或 app-content".into())),
    }
    .expect("fixed official URL");
    if channel == "web-api" {
        url.query_pairs_mut().append_pair("itemId", chapter);
    } else {
        url.query_pairs_mut().extend_pairs([
            ("aid", "13"),
            ("app_name", "novelapp"),
            ("channel", "0"),
            ("device_platform", "android"),
            ("item_id", chapter),
            ("support_image", "1"),
        ]);
    }
    Ok(url)
}

/// Exactly one ordinary GET, no cookie store, signing, device registration, retries or fallback.
/// The report never includes the response body, arbitrary response headers, IDs or credentials.
pub fn probe_official_chapter(
    channel: &str,
    book: &str,
    chapter: &str,
) -> Result<OfficialProbeReport> {
    let url = target(channel, book, chapter)?;
    let client = Client::builder()
        .user_agent(concat!(
            "OpenNovelDownloader/",
            env!("CARGO_PKG_VERSION"),
            " (single-chapter protocol research)"
        ))
        .connect_timeout(Duration::from_secs(8))
        .timeout(Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .build()
        .map_err(|_| Error::Network("无法初始化官方协议探测客户端".into()))?;
    let mut request = client.get(url.clone()).header("accept", "application/json");
    if channel == "web-api" {
        request = request.header("ismobile", "0");
    }
    let response = request
        .send()
        .map_err(|_| Error::Network("单次官方探测连接失败或超时；未保存正文".into()))?;
    let status = response.status().as_u16();
    let verification = response.headers().contains_key("bdturing-verify");
    let encrypted = response.headers().contains_key("y")
        || response.headers().get("c").is_some_and(|v| v != "0");
    // Do not download a verification page, denied body or redirect destination.
    let mut body = Vec::new();
    if !verification && (200..300).contains(&status) {
        response.take(MAX_BYTES as u64 + 1).read_to_end(&mut body)?;
    }
    let mut report = analyze(
        channel,
        book,
        chapter,
        status,
        verification,
        encrypted,
        &body,
    );
    report.endpoint = format!(
        "https://{}{}",
        url.host_str().expect("fixed host"),
        url.path()
    );
    Ok(report)
}

fn analyze(
    channel: &str,
    book: &str,
    chapter: &str,
    status: u16,
    verification: bool,
    encrypted: bool,
    body: &[u8],
) -> OfficialProbeReport {
    let mut report = OfficialProbeReport {
        channel: channel.into(),
        endpoint: String::new(),
        http_status: status,
        bytes: body.len(),
        sha256: format!("{:x}", Sha256::digest(body)),
        outcome: "unknown_schema".into(),
        identity_matches: None,
        declared_characters: None,
        observed_characters: None,
        evidence: "响应未达到原型的完整正文校验条件，未保存".into(),
    };
    let (outcome, evidence) = if verification || matches!(status, 401 | 403) {
        (
            "access_challenge",
            "官网要求处理访问，未重试、换身份或继续请求",
        )
    } else if status == 429 {
        ("rate_limited", "官网限流；原型已停止，没有后续请求")
    } else if (300..400).contains(&status) {
        ("redirect_stopped", "原型禁止重定向，没有访问目标地址")
    } else if !(200..300).contains(&status) {
        ("http_error", "HTTP 请求未成功，没有将错误正文作为章节")
    } else if body.len() > MAX_BYTES {
        ("oversized", "响应超过 2 MiB，原型拒绝继续解析")
    } else if encrypted {
        (
            "protected_response",
            "响应带加密通道标记，没有把密文作为正文",
        )
    } else if body.is_empty() {
        ("empty_response", "HTTP 成功但正文为空，未取得章节")
    } else {
        return analyze_json(report, book, chapter, body);
    };
    report.outcome = outcome.into();
    report.evidence = evidence.into();
    report
}

fn analyze_json(
    mut report: OfficialProbeReport,
    book: &str,
    chapter: &str,
    body: &[u8],
) -> OfficialProbeReport {
    let value: Value = match serde_json::from_slice(body) {
        Ok(v) => v,
        Err(_) => {
            report.outcome = "not_json".into();
            report.evidence = "响应不是 JSON，未按章节解析".into();
            return report;
        }
    };
    if value.get("code").and_then(Value::as_i64) != Some(0) {
        report.outcome = "business_error".into();
        report.evidence = "业务状态不是明确成功；报告不回显任意服务端消息".into();
        return report;
    }
    // This is the actual web client's response path. App data of unknown shape stays unsupported.
    let data = &value["data"]["chapterData"];
    let (Some(actual_book), Some(actual_chapter)) =
        (data["bookId"].as_str(), data["itemId"].as_str())
    else {
        return report;
    };
    let matches = actual_book == book && actual_chapter == chapter;
    report.identity_matches = Some(matches);
    if !matches {
        report.outcome = "identity_mismatch".into();
        report.evidence = "响应作品或章节与本次请求不一致".into();
        return report;
    }
    let (Some(locked), Some(pay)) = (data["isChapterLock"].as_bool(), data["needPay"].as_i64())
    else {
        return report;
    };
    if locked || pay != 0 || data["isPaidPublication"] == true || data["isPaidStory"] == true {
        report.outcome = "restricted_or_preview".into();
        report.evidence = "响应标记锁定或付费，未将预览计为完整正文".into();
        return report;
    }
    let Some(content) = data["content"].as_str() else {
        return report;
    };
    if content
        .chars()
        .any(|c| matches!(c, '\u{e000}'..='\u{f8ff}' | '\u{fffd}'))
    {
        report.outcome = "encoded_text".into();
        report.evidence = "正文含受保护字形或替换字符，需要独立验证字体，未判成功".into();
        return report;
    }
    let declared = data["chapterWordNumber"]
        .as_str()
        .and_then(|v| v.parse::<u64>().ok());
    report.declared_characters = declared;
    let html = Html::parse_fragment(content);
    if html
        .select(
            &Selector::parse("script,style,iframe,img,canvas,svg,video,audio,[hidden],[style]")
                .expect("static selector"),
        )
        .next()
        .is_some()
    {
        return report;
    }
    let paragraphs: Vec<String> = html
        .select(&Selector::parse("p").expect("static selector"))
        .map(|p| p.text().collect::<String>())
        .collect();
    if paragraphs.iter().any(|p| {
        [
            "订阅本章",
            "本章为付费章节",
            "试读结束",
            "试看结束",
            "请完成验证",
        ]
        .iter()
        .any(|marker| p.contains(marker))
    }) {
        report.outcome = "restricted_or_preview".into();
        report.evidence = "正文含订阅、试读或验证提示，未判为完整章节".into();
        return report;
    }
    let count = paragraphs
        .iter()
        .map(|p| p.chars().filter(|c| !c.is_whitespace()).count())
        .sum::<usize>();
    report.observed_characters = Some(count);
    if declared.is_none_or(|d| {
        d == 0
            || d > 200_000
            || (count as u128) * 100 < (d as u128) * 80
            || (count as u128) * 100 > (d as u128) * 150
    }) {
        report.outcome = "length_mismatch".into();
        report.evidence = "正文为空、缺声明字数或比例异常；未将截断内容判成功".into();
        return report;
    }
    let Some(title) = data["title"].as_str().filter(|s| !s.trim().is_empty()) else {
        return report;
    };
    let validated = crate::ImportDocument {
        id: book.into(),
        title: "官方单章协议原型".into(),
        author: String::new(),
        chapters: vec![crate::ImportChapter {
            id: chapter.into(),
            title: title.into(),
            paragraphs,
        }],
    }
    .validated();
    if validated.is_ok() {
        report.outcome = "readable_candidate".into();
        report.evidence =
            "身份、开放标记与长度通过；尚需目录标题及逐字对照，未写入书库或宣称全本接入".into();
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    const BOOK: &str = "10000000001";
    const CHAPTER: &str = "10000000002";
    fn response() -> Value {
        json!({"code":0,"data":{"chapterData":{"bookId":BOOK,"itemId":CHAPTER,"isChapterLock":false,"needPay":0,"title":"原创测试章","chapterWordNumber":"12","content":"<p>山间的小屋亮起一盏灯。</p>"}}})
    }
    fn check(v: &Value) -> OfficialProbeReport {
        analyze(
            "web-api",
            BOOK,
            CHAPTER,
            200,
            false,
            false,
            &serde_json::to_vec(v).unwrap(),
        )
    }

    #[test]
    fn report_accepts_only_a_readable_candidate_and_never_serializes_body_or_ids() {
        let report = check(&response());
        assert_eq!(report.outcome, "readable_candidate");
        let raw = serde_json::to_string(&report).unwrap();
        for sensitive in [BOOK, CHAPTER, "山间", "原创测试章"] {
            assert!(!raw.contains(sensitive));
        }
    }
    #[test]
    fn empty_denied_redirected_rate_limited_and_encrypted_never_pass() {
        for (status, verify, encrypted, outcome) in [
            (200, false, false, "empty_response"),
            (200, true, false, "access_challenge"),
            (403, false, false, "access_challenge"),
            (429, false, false, "rate_limited"),
            (302, false, false, "redirect_stopped"),
            (200, false, true, "protected_response"),
        ] {
            assert_eq!(
                analyze("web-api", BOOK, CHAPTER, status, verify, encrypted, b"").outcome,
                outcome
            );
        }
        assert_eq!(
            analyze(
                "web-api",
                BOOK,
                CHAPTER,
                200,
                false,
                false,
                &vec![0; MAX_BYTES + 1]
            )
            .outcome,
            "oversized"
        );
    }
    #[test]
    fn altered_identity_permission_font_and_length_are_rejected() {
        for (field, value, expected) in [
            ("bookId", json!("10000000009"), "identity_mismatch"),
            ("itemId", json!(7), "unknown_schema"),
            ("isChapterLock", json!(true), "restricted_or_preview"),
            ("needPay", json!(1), "restricted_or_preview"),
            ("needPay", Value::Null, "unknown_schema"),
            ("content", json!("<p>\u{e001}</p>"), "encoded_text"),
            ("chapterWordNumber", json!("900"), "length_mismatch"),
            ("content", json!("<p>少</p>"), "length_mismatch"),
        ] {
            let mut v = response();
            v["data"]["chapterData"][field] = value;
            assert_eq!(check(&v).outcome, expected);
        }
        for content in [
            "<p hidden>山间的小屋亮起一盏灯。</p>",
            "<p style='display:none'>山间的小屋亮起一盏灯。</p>",
            "<p>山间的小屋亮起一盏灯。</p><script>secret</script>",
        ] {
            let mut v = response();
            v["data"]["chapterData"]["content"] = json!(content);
            assert_ne!(check(&v).outcome, "readable_candidate");
        }
        let mut v = response();
        v["data"]["chapterData"]["content"] = json!("<p>试读结束请订阅本章继续阅读。</p>");
        assert_eq!(check(&v).outcome, "restricted_or_preview");
        let mut v = response();
        v["code"] = json!(-1);
        v["message"] = json!("secret-token");
        assert_eq!(check(&v).outcome, "business_error");
        assert!(
            !serde_json::to_string(&check(&v))
                .unwrap()
                .contains("secret-token")
        );
    }
    #[test]
    fn fixed_targets_never_accept_url_or_credential_injection() {
        for invalid in [
            "https://example.com",
            "123?key=secret",
            "user:pass",
            "0012345678",
            "123",
            "1/../../secret",
        ] {
            assert!(target("web-api", invalid, CHAPTER).is_err());
        }
        assert!(target("https://example.com", BOOK, CHAPTER).is_err());
        let url = target("web-api", BOOK, CHAPTER).unwrap();
        assert_eq!(url.host_str(), Some("fanqienovel.com"));
        assert_eq!(url.query(), Some("itemId=10000000002"));
    }
}
