use std::env;

use melsec_mc::{endpoint::ConnectionTarget, init_defaults, mc_client::McClient};
use tracing::{debug, error};

#[tokio::test]
async fn parse_read_bits_via_client() {
    init_defaults().ok();

    let addr = env::var("PLC_ADDR").unwrap_or_else(|_| "192.168.1.40".to_string());
    let tcp_port = env::var("PLC_TCP_PORT")
        .ok()
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(4020);

    let target = ConnectionTarget::new()
        .with_ip(addr)
        .with_port(tcp_port)
        .build();

    let client = McClient::new()
        .with_target(target)
        .with_protocol(melsec_mc::mc_define::Protocol::Tcp)
        .with_plc_series(melsec_mc::plc_series::PLCSeries::R);

    match client.read_bits("M100", 8).await {
        Ok(json) => debug!("PARSED JSON: {json}"),
        Err(e) => error!("READ_BITS ERROR: {e}"),
    }
}
