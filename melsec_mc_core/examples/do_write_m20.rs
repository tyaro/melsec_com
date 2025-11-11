use std::env;

use melsec_mc::endpoint::ConnectionTarget;
use melsec_mc::init_defaults;
use melsec_mc::mc_client::McClient;
use melsec_mc::mc_define::Protocol;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // initialize embedded defaults
    init_defaults()?;

    let addr = env::var("PLC_ADDR").unwrap_or_else(|_| "192.168.1.40".into());
    let port: u16 = env::var("PLC_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(4020);

    let target = ConnectionTarget::direct(addr.clone(), port);
    let client = McClient::new()
        .with_target(target)
        .with_protocol(Protocol::Tcp)
        .with_monitoring_timer(5u16);

    // This example acts like a small reversible test:
    // 1. Read current values at M20..M30 (11 points)
    // 2. Invert each bit
    // 3. Write inverted values back
    // 4. Read back and verify the write succeeded

    // step 1: read current values
    println!("Reading current M20..M30 on {addr}:{port} (11 points)...");
    let current = match client.read_bits("M20", 11).await {
        Ok(r) => {
            // The returned JSON looks like: { "data_blocks": [ [ true, false, ... ] ] }
            // Try to extract the first data block as an array of bools.
            let block0 = r.get("data_blocks").and_then(|db| db.get(0));
            if let Some(arr) = block0.and_then(|b| b.as_array()) {
                let vec: Vec<bool> = arr.iter().map(|v| v.as_bool().unwrap_or(false)).collect();
                vec
            } else {
                eprintln!("Unexpected response format: cannot extract bool vector");
                return Ok(());
            }
        }
        Err(e) => {
            eprintln!("Initial read_bits failed: {e}");
            return Ok(());
        }
    };

    println!("Current values: {current:?}");

    // step 2: invert
    let inverted: Vec<bool> = current.iter().map(|b| !*b).collect();
    println!("Inverted values to write: {inverted:?}");

    // step 3: write inverted values
    println!("Writing inverted values to M20..M30...");
    match client.write_bits("M20", &inverted).await {
        Ok(v) => println!("write_bits succeeded: {v}"),
        Err(e) => {
            eprintln!("write_bits failed: {e}");
            return Ok(());
        }
    }

    // step 4: read back and verify
    println!("Reading back to verify...");
    match client.read_bits("M20", 11).await {
        Ok(r) => {
            let block0 = r.get("data_blocks").and_then(|db| db.get(0));
            if let Some(arr) = block0.and_then(|b| b.as_array()) {
                let v: Vec<bool> = arr.iter().map(|v| v.as_bool().unwrap_or(false)).collect();
                if v == inverted {
                    println!("Verification succeeded: readback matches inverted values");
                } else {
                    eprintln!("Verification FAILED: readback does not match inverted values\nreadback={v:?}\nexpected={inverted:?}");
                }
            } else {
                eprintln!("Unexpected response format on verification: cannot extract bool vector");
            }
        }
        Err(e) => eprintln!("Verification read_bits failed: {e}"),
    }

    Ok(())
}
