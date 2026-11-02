//! Original stdio adapter. Linked GPL component is distributed separately with source.
//! No HTTP client, device pool, listener, credentials, or filesystem is used here.
#![cfg_attr(windows, windows_subsystem = "windows")]
use base64::{engine::general_purpose::STANDARD, Engine};
use fqapi_core::{endpoints::dhcontent, sign::Manager, upstream::dh::generate_y};
use serde::Deserialize;
use std::io::{BufRead, Read, Write};

const LIMIT: u64 = 3 * 1024 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request { sequence: u64, item_id: String }

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Response { sequence: u64, status: u16, headers: Vec<(String, String)>, body: String }

fn read<T: serde::de::DeserializeOwned>(input: &mut impl BufRead) -> Result<Option<T>, ()> {
    let mut raw = Vec::new();
    let size = Read::by_ref(input).take(LIMIT + 1).read_until(b'\n', &mut raw).map_err(|_| ())?;
    if size == 0 { return Ok(None); }
    if size as u64 > LIMIT || raw.last() != Some(&b'\n') { return Err(()); }
    serde_json::from_slice(&raw).map(Some).map_err(|_| ())
}

fn emit(value: serde_json::Value) -> Result<(), ()> {
    println!("{value}");
    std::io::stdout().flush().map_err(|_| ())
}

fn number(v: &serde_json::Value) -> Option<u64> {
    v.as_u64().or_else(|| v.as_str()?.parse().ok())
}

fn run() -> Result<(), ()> {
    let mut input = std::io::BufReader::new(std::io::stdin());
    let mut previous = 0;
    while let Some(request) = read::<Request>(&mut input)? {
        if request.sequence != previous + 1 || !(10..=24).contains(&request.item_id.len())
            || request.item_id.starts_with('0') || !request.item_id.bytes().all(|c| c.is_ascii_digit()) { return Err(()); }
        previous = request.sequence;
        let url = dhcontent::novel_reader_content_url(&request.item_id);
        let parsed = url::Url::parse(&url).map_err(|_| ())?;
        let query = parsed.query().ok_or(())?;
        let (y, state) = generate_y().map_err(|_| ())?;
        let mut headers = dhcontent::novel_reader_content_headers(&y);
        // The broker forwards raw bytes; no hidden decompression or expanded-size risk.
        headers.retain(|(key, _)| !key.eq_ignore_ascii_case("accept-encoding"));
        headers.push(("accept-encoding".into(), "identity".into()));
        headers.extend(Manager::new().generate_headers(query, None));
        emit(serde_json::json!({"op":"fetch","sequence":previous,"request":{
            "method":"GET","target":format!("{}?{}",parsed.path(),query),"headers":headers}}))?;
        let response = read::<Response>(&mut input)?.ok_or(())?;
        if response.sequence != previous || response.status != 200 || response.headers.len() > 16 { return Err(()); }
        if response.headers.iter().any(|(key,value)| key.eq_ignore_ascii_case("content-encoding") && value != "identity") { return Err(()); }
        let raw = STANDARD.decode(response.body).map_err(|_| ())?;
        if raw.len() > 2 * 1024 * 1024 { return Err(()); }
        let value = dhcontent::set_dh_content_in_place(&raw, &response.headers, &state).map_err(|_| ())?;
        let data = &value["data"];
        let meta = &data["novel_data"];
        let content = data["content"].as_str().ok_or(())?;
        if value["code"].as_i64() != Some(0) || data["item_id"].as_str() != Some(&request.item_id)
            || number(&meta["need_pay"]) != Some(0) || content.len() > 2 * 1024 * 1024 { return Err(()); }
        emit(serde_json::json!({"sequence":previous,"ok":true,"envelope":{
            "schema_version":1,"book_id":meta["book_id"],"item_id":data["item_id"],
            "title":meta["chapter_title"],"need_pay":number(&meta["need_pay"]),
            "word_number":number(&meta["word_number"]),"content":content}}))?;
    }
    Ok(())
}

fn main() { if run().is_err() { std::process::exit(3); } }
