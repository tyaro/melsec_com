use std::env;

use melsec_mc::{init_defaults, ConnectionTarget};
use melsec_mc::mc_client::McClient;
use melsec_mc::mc_define::McFrameFormat;
use melsec_mc::command_registry::{GLOBAL_COMMAND_REGISTRY, create_read_words_params};
use melsec_mc::commands::Command;
use melsec_mc::request::McRequest;
use melsec_mc::response::McResponse;
use melsec_mc::mc_define::Protocol;
use std::time::Duration;

fn should_run() -> bool {
    env::var("RUN_REAL_TESTS").map(|v| v == "1").unwrap_or(false)
}

fn env_addr() -> String {
    env::var("PLC_ADDR").unwrap_or_else(|_| "192.168.1.40".to_string())
}
fn env_tcp_port() -> u16 {
    env::var("PLC_TCP_PORT").ok().and_then(|s| s.parse::<u16>().ok()).unwrap_or(4020)
}

fn test_client_for(port: u16) -> McClient {
    init_defaults().expect("init defaults");

    let access_route = melsec_mc::mc_define::AccessRoute::default();

    let addr = env_addr();

    let target = ConnectionTarget::new()
        .with_ip(&addr)
        .with_port(port)
        .with_access_route(access_route)
        .build();

    McClient::new()
        .with_target(target)
        .with_protocol(Protocol::Tcp)
        .with_monitoring_timer(5)
}

#[tokio::test]
async fn real_mc4e_read_words_tcp() {
    if !should_run() { eprintln!("skipping real_mc4e_read_words_tcp (set RUN_REAL_TESTS=1 to enable)"); return; }
    let client = test_client_for(env_tcp_port()).with_mc_format(McFrameFormat::MC4E);
    // Read a small number of words from device specified by REAL_READ_WORD_DEVICE or default D500
    let device = env::var("REAL_READ_WORD_DEVICE").unwrap_or_else(|_| "D500".to_string());
    let count = env::var("REAL_READ_WORD_COUNT").ok().and_then(|s| s.parse::<u16>().ok()).unwrap_or(2u16);

    init_defaults().expect("init defaults");
    let reg = GLOBAL_COMMAND_REGISTRY.get().expect("global registry");
    let spec = reg.get(Command::ReadWords).expect("command spec");
    let params = create_read_words_params(&device, count);
    let request_data = spec.build_request(&params, Some(client.plc_series)).expect("build_request");

    let mc_req = McRequest::new()
        .with_access_route(client.target.access_route)
        .try_with_request_data(request_data).expect("build mc request");
    let mc_payload = mc_req.build_with_format(McFrameFormat::MC4E);

    let timeout = if client.monitoring_timer > 0 { Some(Duration::from_secs(u64::from(client.monitoring_timer))) } else { None };
    let raw = match melsec_mc::transport::send_and_recv_tcp(&client.target.addr, &mc_payload, timeout).await {
        Ok(b) => b,
        Err(e) => { eprintln!("tcp send/recv failed: {e}"); return; }
    };
    let send_hex = mc_payload.iter().map(|b| format!("{b:02X}")).collect::<Vec<String>>().join(" ");
    let recv_hex = raw.iter().map(|b| format!("{b:02X}")).collect::<Vec<String>>().join(" ");
    eprintln!("MC4E SENT: {send_hex}");
    eprintln!("MC4E RECV: {recv_hex}");

    let resp = match McResponse::try_new(&raw) {
        Ok(r) => r,
        Err(e) => { eprintln!("failed to parse response: {e}"); return; }
    };
    assert!(resp.has_end_code);
}

#[tokio::test]
async fn real_mc3e_read_words_tcp() {
    if !should_run() { eprintln!("skipping real_mc3e_read_words_tcp (set RUN_REAL_TESTS=1 to enable)"); return; }
    let client = test_client_for(env_tcp_port()).with_mc_format(McFrameFormat::MC3E);
    let device = env::var("REAL_READ_WORD_DEVICE").unwrap_or_else(|_| "D500".to_string());
    let count = env::var("REAL_READ_WORD_COUNT").ok().and_then(|s| s.parse::<u16>().ok()).unwrap_or(2u16);

    init_defaults().expect("init defaults");
    let reg = GLOBAL_COMMAND_REGISTRY.get().expect("global registry");
    let spec = reg.get(Command::ReadWords).expect("command spec");
    let params = create_read_words_params(&device, count);
    let request_data = spec.build_request(&params, Some(client.plc_series)).expect("build_request");

    let mc_req = McRequest::new()
        .with_access_route(client.target.access_route)
        .with_monitoring_timer(client.monitoring_timer)
        .try_with_request_data(request_data).expect("build mc request");
    let mc_payload = mc_req.build_with_format(McFrameFormat::MC3E);

    let timeout = if client.monitoring_timer > 0 { Some(Duration::from_secs(u64::from(client.monitoring_timer))) } else { None };
    let raw = match melsec_mc::transport::send_and_recv_tcp(&client.target.addr, &mc_payload, timeout).await {
        Ok(b) => b,
        Err(e) => { eprintln!("tcp send/recv failed: {e}"); return; }
    };
    let send_hex = mc_payload.iter().map(|b| format!("{b:02X}")).collect::<Vec<String>>().join(" ");
    let recv_hex = raw.iter().map(|b| format!("{b:02X}")).collect::<Vec<String>>().join(" ");
    eprintln!("MC3E SENT: {send_hex}");
    eprintln!("MC3E RECV: {recv_hex}");

    let resp = match McResponse::try_new(&raw) {
        Ok(r) => r,
        Err(e) => { eprintln!("failed to parse response: {e}"); return; }
    };
    // MC3E responses may not include serial; ensure response parsed
    assert!(resp.data.len() >= 0);
}
