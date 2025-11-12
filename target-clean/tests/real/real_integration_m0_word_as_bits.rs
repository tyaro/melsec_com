use std::env;

use melsec_mc::{init_defaults, ConnectionTarget};
use melsec_mc::mc_client::McClient;
use melsec_mc::mc_define::Protocol;
use serde_json::Value as JsonValue;

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
        .with_client_name("m0_word_bits_integration")
}

fn env_tcp_port() -> u16 {
    env::var("PLC_TCP_PORT").ok().and_then(|s| s.parse::<u16>().ok()).unwrap_or(4020)
}
fn env_udp_port() -> u16 {
    env::var("PLC_UDP_PORT").ok().and_then(|s| s.parse::<u16>().ok()).unwrap_or(4021)
}

fn bits_from_word_le(w: u16) -> Vec<bool> {
    (0..16).map(|i| ((w >> i) & 1) != 0).collect()
}

fn validate_response(resp: &JsonValue) {
    // Expect data_blocks[0][0] present and bit_blocks[0] present
    let data_blocks = resp.get("data_blocks").and_then(|v| v.as_array()).expect("data_blocks missing");
    assert!(!data_blocks.is_empty(), "data_blocks empty");
    let first_block = data_blocks[0].as_array().expect("first data block not array");
    assert!(!first_block.is_empty(), "first data block empty");
    let word = u16::try_from(first_block[0].as_u64().expect("word not numeric")).unwrap_or(0);

    let bit_blocks = resp.get("bit_blocks").and_then(|v| v.as_array()).expect("bit_blocks missing");
    assert!(!bit_blocks.is_empty(), "bit_blocks empty");
    let b0 = bit_blocks[0].as_array().expect("first bit block not array");
    assert_eq!(b0.len(), 16, "bit_blocks[0] must have 16 entries");

    let expected = bits_from_word_le(word);
    for i in 0..16 {
        let b = b0[i].as_bool().expect("bit not bool");
        assert_eq!(b, expected[i], "mismatch at bit {}: got {} expected {}", i, b, expected[i]);
    }
}

#[tokio::test]
async fn real_integration_m0_word_as_bits_tcp() {
    if !should_run() { eprintln!("skipping real_integration_m0_word_as_bits_tcp (set RUN_REAL_TESTS=1 to enable)"); return; }
    let client = test_client_for(Protocol::Tcp, env_tcp_port());
    melsec_mc::announce("real_integration_m0_word_as_bits_tcp", "Integration: read M0 as 1 word and validate generated bit_blocks (TCP)");
    eprintln!("integration test: read M0 as 1 word over TCP and validate bit_blocks");
    let res = match client.read_words("M0", 1).await {
        Ok(r) => r,
        Err(e) => { panic!("read_words failed: {e}"); }
    };
    validate_response(&res);
}

#[tokio::test]
async fn real_integration_m0_word_as_bits_udp() {
    if !should_run() { eprintln!("skipping real_integration_m0_word_as_bits_udp (set RUN_REAL_TESTS=1 to enable)"); return; }
    let client = test_client_for(Protocol::Udp, env_udp_port());
    melsec_mc::announce("real_integration_m0_word_as_bits_udp", "Integration: read M0 as 1 word and validate generated bit_blocks (UDP)");
    eprintln!("integration test: read M0 as 1 word over UDP and validate bit_blocks");
    let res = match client.read_words("M0", 1).await {
        Ok(r) => r,
        Err(e) => { panic!("read_words failed: {e}"); }
    };
    validate_response(&res);
}
