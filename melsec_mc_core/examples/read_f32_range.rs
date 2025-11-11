use melsec_mc::endpoint::ConnectionTarget;
use melsec_mc::init_defaults;
use melsec_mc::mc_client::{FromWords, McClient};
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // initialize
    init_defaults()?;
    env_logger::init();

    let addr = env::var("PLC_ADDR").unwrap_or_else(|_| "192.168.1.40".into());
    let port: u16 = env::var("PLC_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(4020);
    let target = ConnectionTarget::direct(addr.clone(), port);
    let client = McClient::new().with_target(target).with_monitoring_timer(5);

    // range D1020..D1052 inclusive
    let start = 1020u32;
    let end = 1052u32;
    let total_words = (end - start + 1) as usize; // 33
    let elem_count = total_words / 2; // 16 elements (pairs)
    println!(
        "Reading {} words ({} elements f32) from D{}..D{}",
        total_words, elem_count, start, end
    );

    // request elements using read_words (we'll parse locally to detect failures)
    let required_words = (elem_count * 2) as u16;
    let resp_json = client.read_words("D1020", required_words).await?;
    // extract words
    let mut words: Vec<u16> = Vec::new();
    if let Some(db) = resp_json.get("data_blocks").and_then(|v| v.as_array()) {
        for block in db {
            if let Some(arr) = block.as_array() {
                for it in arr {
                    if let Some(n) = it.as_u64() {
                        if let Ok(w) = u16::try_from(n) {
                            words.push(w);
                        }
                    }
                }
            }
        }
    }

    println!("received {} words", words.len());

    // parse with FromWords for f32 using same tolerant logic
    let mut idx = 0usize;
    let mut parsed_idx = 0usize;
    while parsed_idx < elem_count && idx + 2usize <= words.len() {
        match <f32 as FromWords>::from_words_slice(&words[idx..]) {
            Ok((val, used)) => {
                println!(
                    "element {}: OK -> {} (words {}..{})",
                    parsed_idx,
                    val,
                    idx,
                    idx + used - 1
                );
                idx += used;
                parsed_idx += 1;
            }
            Err(e) => {
                println!(
                    "element {}: PARSE ERROR at word idx {}: {}",
                    parsed_idx, idx, e
                );
                idx += 1; // resync
            }
        }
    }

    if parsed_idx < elem_count {
        println!(
            "stopped early: parsed {} of {} elements",
            parsed_idx, elem_count
        );
    }

    Ok(())
}
