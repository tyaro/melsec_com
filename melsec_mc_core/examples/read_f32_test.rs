use std::env;

use melsec_mc::endpoint::ConnectionTarget;
use melsec_mc::init_defaults;
use melsec_mc::mc_client::McClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize embedded defaults (commands & error codes). Safe to call repeatedly.
    init_defaults()?;

    let addr = env::var("PLC_ADDR").unwrap_or_else(|_| "192.168.1.40".into());
    let port: u16 = env::var("PLC_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(4020);

    let target = ConnectionTarget::direct(addr.clone(), port);

    let client = McClient::new().with_target(target).with_monitoring_timer(5);

    println!(
        "Connecting to PLC at {}:{}, reading two f32 at D1000/D1002...",
        addr, port
    );

    match client.read_words_as::<f32>("D1000", 2).await {
        Ok(vals) => {
            println!("Read {} f32 values:", vals.len());
            for (i, v) in vals.iter().enumerate() {
                println!("  [{}] {}", i, v);
            }
        }
        Err(e) => eprintln!("Read failed: {e}"),
    }

    Ok(())
}
