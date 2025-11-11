use melsec_mc::endpoint::ConnectionTarget;
use melsec_mc::init_defaults;
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_defaults()?;
    env_logger::init();

    let addr = env::var("PLC_ADDR").unwrap_or_else(|_| "192.168.1.40".into());
    let port: u16 = env::var("PLC_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(4020);
    let target = ConnectionTarget::direct(addr.clone(), port);
    let client = melsec_mc::mc_client::McClient::new()
        .with_target(target)
        .with_monitoring_timer(5);

    let required_words = 32u16; // 16 f32 elements
    let resp_json = client.read_words("D1020", required_words).await?;
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

    println!("got {} words", words.len());
    for (i, w) in words.iter().enumerate() {
        println!(
            "word[{:02}] = 0x{:04X} (bytes LE: {:02X} {:02X})",
            i,
            w,
            w & 0xFF,
            (w >> 8) & 0xFF
        );
    }

    println!("\nElements (pairs LE -> u32 hex -> f32 if not NaN):");
    let mut idx = 0usize;
    let mut elem = 0usize;
    while idx + 1 < words.len() {
        let low = words[idx] as u32;
        let high = words[idx + 1] as u32;
        let combined = (high << 16) | low;
        let f = f32::from_bits(combined);
        println!(
            "elem {:02} words[{}..{}] -> u32=0x{:08X} f32={}",
            elem,
            idx,
            idx + 1,
            combined,
            f
        );
        idx += 2;
        elem += 1;
    }

    Ok(())
}
