//! Original persistent evaluation adapter. Candidate GPL code stays outside the main application.
use fqapi_core::{
    config::Config,
    device::DevicePool,
    endpoints::{dhcontent::fetch_novel_reader_content, Ctx},
    filter::FilterManager,
    sign::Manager,
    upstream::UpstreamClient,
};
use serde::Deserialize;
use std::{
    io::{BufRead, Read, Write},
    sync::Arc,
    time::Duration,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    sequence: u64,
    item_id: String,
}

fn number(v: &serde_json::Value) -> Option<u64> {
    v.as_u64().or_else(|| v.as_str()?.parse().ok())
}

async fn run() -> Result<(), ()> {
    let cap = std::env::var("OPENNOVEL_REVIEW_CAPABILITY").map_err(|_| ())?;
    if cap.len() != 64 || !cap.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err(());
    }
    let origin = format!("http://127.0.0.1:8123/{cap}");
    let root = std::path::PathBuf::from(format!("/tmp/opennovel-service-{}", std::process::id()));
    std::fs::create_dir(&root).map_err(|_| ())?;
    let pool = DevicePool::with_mock_origin(root.join("empty-pool.json"), origin.clone())
        .await
        .map_err(|_| ())?;
    let up =
        UpstreamClient::with_mock_origin(pool.clone(), Manager::new(), origin).map_err(|_| ())?;
    let filters = FilterManager::load_with_base(
        &root.join("none.json").to_string_lossy(),
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
    let mut input = std::io::BufReader::new(std::io::stdin());
    let mut previous = 0;
    loop {
        let mut raw = Vec::new();
        let size = Read::by_ref(&mut input)
            .take(4097)
            .read_until(b'\n', &mut raw)
            .map_err(|_| ())?;
        if size == 0 {
            return Ok(());
        }
        if size > 4096 || raw.last() != Some(&b'\n') {
            return Err(());
        }
        let request: Request = serde_json::from_slice(&raw).map_err(|_| ())?;
        if request.sequence != previous + 1
            || !(10..=24).contains(&request.item_id.len())
            || !request.item_id.bytes().all(|c| c.is_ascii_digit())
        {
            return Err(());
        }
        previous = request.sequence;
        let result = tokio::time::timeout(
            Duration::from_secs(25),
            fetch_novel_reader_content(&ctx, &request.item_id),
        )
        .await;
        let value = match result {
            Ok(Ok(v)) => v,
            _ => {
                println!("{}", serde_json::json!({"sequence":previous,"ok":false}));
                std::io::stdout().flush().map_err(|_| ())?;
                return Err(());
            }
        };
        let data = &value["data"];
        let meta = &data["novel_data"];
        if value["code"].as_i64() != Some(0)
            || data["item_id"].as_str() != Some(&request.item_id)
            || ctx.pool.count().await != 0
            || root.join("empty-pool.json").exists()
        {
            return Err(());
        }
        let content = data["content"].as_str().ok_or(())?;
        if content.len() > 2 * 1024 * 1024 {
            return Err(());
        }
        println!(
            "{}",
            serde_json::json!({"sequence":previous,"ok":true,"envelope":{
                "schema_version":1,"book_id":meta["book_id"],"item_id":data["item_id"],
                "title":meta["chapter_title"],"need_pay":number(&meta["need_pay"]),
                "word_number":number(&meta["word_number"]),"content":content
            }})
        );
        std::io::stdout().flush().map_err(|_| ())?;
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    if run().await.is_err() {
        std::process::exit(3);
    }
}
