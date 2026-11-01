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
    // An original controlled fixture only: no real book or arbitrary URL is accepted.
    if request.item_id != "10000000002" {
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
        Duration::from_secs(5),
        fetch_novel_reader_content(&ctx, &request.item_id),
    )
    .await
    .map_err(|_| ())?
    .map_err(|_| ())?;
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

#[tokio::main(flavor = "current_thread")]
async fn main() {
    if probe().await.is_err() {
        eprintln!("isolated evaluation refused or failed; no content saved");
        std::process::exit(3);
    }
}
