//! Original evaluation harness, linked only inside an isolated GPL candidate build.
//! Not part of the Apache-licensed application or a redistributable helper package.
use std::{io::Read, sync::Arc, time::Duration};

use fqapi_core::{
    config::Config,
    device::DevicePool,
    endpoints::{dhcontent::fetch_novel_reader_content, Ctx},
    filter::FilterManager,
    sign::Manager,
    upstream::UpstreamClient,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ChapterRequest {
    item_id: String,
}

fn number(value: &serde_json::Value) -> Option<u64> {
    value.as_u64().or_else(|| value.as_str()?.parse().ok())
}

fn canonical(content: &str, title: &str) -> Option<String> {
    let forbidden =
        regex::Regex::new(r"(?i)<\s*(script|style|img|iframe|canvas|svg|audio|video)\b").ok()?;
    if forbidden.is_match(content)
        || content
            .chars()
            .any(|c| matches!(c, '\u{e000}'..='\u{f8ff}' | '\u{fffd}'))
    {
        return None;
    }
    let tags = regex::Regex::new(r"<[^>]*>").ok()?;
    let plain = tags
        .replace_all(content, "")
        .replace("&nbsp;", " ")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&");
    let text: String = plain.chars().filter(|c| !c.is_whitespace()).collect();
    let title: String = title.chars().filter(|c| !c.is_whitespace()).collect();
    Some(text.strip_prefix(&title).unwrap_or(&text).to_owned())
}

async fn probe() -> Result<(), ()> {
    let mut input = Vec::new();
    std::io::stdin()
        .take(4097)
        .read_to_end(&mut input)
        .map_err(|_| ())?;
    if input.len() > 4096 {
        return Err(());
    }
    let request: ChapterRequest = serde_json::from_slice(&input).map_err(|_| ())?;
    // Default remains the original fixture; the explicit host approval pins one public sample.
    let approved =
        std::env::var("OPENNOVEL_APPROVED_ITEM").unwrap_or_else(|_| "10000000002".into());
    if !(10..=24).contains(&approved.len())
        || approved.starts_with('0')
        || !approved.bytes().all(|c| c.is_ascii_digit())
        || request.item_id != approved
    {
        return Err(());
    }
    let capability = std::env::var("OPENNOVEL_REVIEW_CAPABILITY").map_err(|_| ())?;
    if capability.len() != 64
        || !capability
            .bytes()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
    {
        return Err(());
    }
    let origin = format!("http://127.0.0.1:8123/{capability}");
    let root = std::path::PathBuf::from(format!("/tmp/opennovel-review-{}", std::process::id()));
    std::fs::create_dir(&root).map_err(|_| ())?;
    // Both paths use the broker. No normal registrar or generic route dispatcher is exposed.
    let pool = DevicePool::with_mock_origin(root.join("empty-pool.json"), origin.clone())
        .await
        .map_err(|_| ())?;
    let up =
        UpstreamClient::with_mock_origin(pool.clone(), Manager::new(), origin).map_err(|_| ())?;
    let filters = FilterManager::load_with_base(
        &root.join("missing-filter.json").to_string_lossy(),
        &root.to_string_lossy(),
    )
    .map_err(|_| ())?;
    let ctx = Ctx {
        up: Arc::new(up),
        pool,
        cfg: Arc::new(Config::default()),
        src_dir: String::new(),
        filters: Arc::new(filters),
    };
    let value = tokio::time::timeout(
        Duration::from_secs(25),
        fetch_novel_reader_content(&ctx, &request.item_id),
    )
    .await
    .map_err(|_| ())?
    .map_err(|_| ())?;
    if approved != "10000000002" {
        let data = &value["data"];
        let text = data["content"].as_str().unwrap_or("");
        let title = data["novel_data"]["chapter_title"].as_str().unwrap_or("");
        let normalized = canonical(text, title);
        let novel_metadata: serde_json::Map<String, serde_json::Value> = ["book_id", "item_id", "chapter_word_number", "word_number", "title", "chapter_title", "need_pay", "is_paid_publication", "is_paid_story"].iter().map(|key| {
            let v = &data["novel_data"][*key];
            (key.to_string(), serde_json::json!({"number":v.as_i64(), "boolean":v.as_bool(), "string_length":v.as_str().map(str::len)}))
        }).collect();
        let mut report = serde_json::json!({
            "outcome":"live_response_unverified",
            "code":value["code"].as_i64(),
            "content_characters":text.chars().count(),
            "identity_matches":data["item_id"].as_str().map(|s|s == approved),
        "book_identity_matches":data["novel_data"]["book_id"].as_str().map(|s|s == "7672011528131857470"),
        "nested_identity_matches":data["novel_data"]["item_id"].as_str().map(|s|s == approved),
        "canonical_characters":normalized.as_ref().map(|s|s.chars().count()),
        "canonical_sha256":normalized.as_ref().map(|s|format!("{:x}",Sha256::digest(s.as_bytes()))),
        "declared_characters":number(&data["novel_data"]["word_number"]),
            "has_novel_data":data["novel_data"].is_object(),
        "novel_metadata": novel_metadata,
        "need_pay":number(&data["novel_data"]["need_pay"]),
            "is_locked":data["isChapterLock"].as_bool(),
            "device_count":ctx.pool.count().await,
            "pool_file_exists":root.join("empty-pool.json").exists()
        });
        if std::env::var("OPENNOVEL_ACCEPT_ENVELOPE").as_deref() == Ok("1") {
            report["envelope"] = serde_json::json!({
                "schema_version":1,
                "book_id":data["novel_data"]["book_id"],
                "item_id":data["item_id"],
                "title":title,
                "word_number":number(&data["novel_data"]["word_number"]),
                "need_pay":number(&data["novel_data"]["need_pay"]),
                "content":text
            });
        }
        println!("{report}");
        // A body is not accepted for download without an independent permissions/schema contract.
        return Err(());
    }
    if value["code"].as_i64() != Some(0)
        || value["data"]["item_id"].as_str() != Some(&request.item_id)
    {
        return Err(());
    }
    let content = value["data"]["content"]
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= 1024)
        .ok_or(())?;
    // Fail if a future engine revision makes this path create or populate device state.
    if ctx.pool.count().await != 0 || root.join("empty-pool.json").exists() {
        return Err(());
    }
    println!(
        "{}",
        serde_json::json!({"outcome":"controlled_fixture", "sha256":format!("{:x}", Sha256::digest(content.as_bytes())), "device_count":0})
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normalize_original_fixture_and_reject_non_text() {
        assert_eq!(
            canonical("<h1>原创测试</h1><p> 山间 &amp; 灯火。 </p>", "原创测试"),
            Some("山间&灯火。".into())
        );
        for text in [
            "<p>原文</p><script>extra</script>",
            "<img src='x'>",
            "<p>\u{e001}</p>",
        ] {
            assert!(canonical(text, "").is_none());
        }
        assert_eq!(number(&serde_json::json!("2256")), Some(2256));
        assert_eq!(number(&serde_json::json!(-1)), None);
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    if probe().await.is_err() {
        eprintln!("isolated evaluation refused or failed; no content saved");
        std::process::exit(3);
    }
}
