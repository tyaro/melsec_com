use std::env;

use melsec_mc::endpoint::ConnectionTarget;
use melsec_mc::init_defaults;
use melsec_mc::mc_client::McClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize embedded defaults (commands & error codes). This is safe to call
    // even if the application already initialized them.
    init_defaults()?;

    // initialize logging so transport debug dumps are visible
    env_logger::init();

    let addr = env::var("PLC_ADDR").unwrap_or_else(|_| "127.0.0.1".into());
    let port: u16 = env::var("PLC_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(5000);

    let target = ConnectionTarget::direct(addr, port);

    let client = McClient::new().with_target(target).with_monitoring_timer(3);

    println!(
        "Attempting to read D0 (1 word) from {addr}",
        addr = client.target.addr
    );
    match client.read_words("D", 1).await {
        Ok(v) => println!("Read result: {v:?}"),
        Err(e) => eprintln!("Read failed: {e}"),
    }

    Ok(())
}
