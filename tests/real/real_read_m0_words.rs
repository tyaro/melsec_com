use std::env;

use melsec_mc::{init_defaults, ConnectionTarget};
use melsec_mc::mc_client::McClient;
use melsec_mc::mc_define::Protocol;

fn should_run() -> bool {
    env::var("RUN_REAL_TESTS").map(|v| v == "1").unwrap_or(false)
}

fn test_client_for(proto: melsec_mc::mc_define::Protocol, port: u16) -> McClient {
    init_defaults().expect("init defaults");

    let access_route = melsec_mc::mc_define::AccessRoute::default()
        .with_network_number(0x00u8)
        .with_pc_number(0xffu8)
        .with_io_number(0x03ff)
        .with_station_number(0x00u8);

    let addr = env::var("PLC_ADDR").unwrap_or_else(|_| "192.168.1.40".to_string());

    let taget = ConnectionTarget::new()
        .with_ip(&addr)
        .with_port(port)
        .with_access_route(access_route)
        .build();

    McClient::new()
        .with_target(taget)
        .with_protocol(proto)
        .with_monitoring_timer(5)
        .with_client_name("read_m0_words_client")
}

fn env_tcp_port() -> u16 {
    env::var("PLC_TCP_PORT").ok().and_then(|s| s.parse::<u16>().ok()).unwrap_or(4020)
}

#[tokio::test]
async fn real_read_m0_words_tcp() {
    if !should_run() { eprintln!("skipping real_read_m0_words_tcp (set RUN_REAL_TESTS=1 to enable)"); return; }
    init_defaults().expect("init defaults");
    let client = test_client_for(Protocol::Tcp, env_tcp_port());

    melsec_mc::announce("real_read_m0_words_tcp", "Read M0..M15 as 16 words over TCP");
    eprintln!("reading M0..M15 as words (16 words)");
    let res = match client.read_words("M0", 16).await {
        Ok(r) => r,
        Err(e) => { eprintln!("read_words failed: {e}"); panic!("read failed"); }
    };
    println!("read M0..M15 as words: {res}");
}
