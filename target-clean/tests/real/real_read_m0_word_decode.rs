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
        .with_client_name("read_m0_word_decode_client")
}

fn env_tcp_port() -> u16 {
    env::var("PLC_TCP_PORT").ok().and_then(|s| s.parse::<u16>().ok()).unwrap_or(4020)
}

fn word_to_bits_le(word: u16) -> Vec<bool> {
    // bit0 -> M0, bit1 -> M1, ... bit15 -> M15
    (0..16).map(|i| ((word >> i) & 1) != 0).collect()
}

#[tokio::test]
async fn real_read_m0_word_decode_tcp() {
    if !should_run() { eprintln!("skipping real_read_m0_word_decode_tcp (set RUN_REAL_TESTS=1 to enable)"); return; }
    init_defaults().expect("init defaults");
    let client = test_client_for(Protocol::Tcp, env_tcp_port());

    melsec_mc::announce("real_read_m0_word_decode_tcp", "Read 1 word at M0 and decode into M0..M15 bits");
    eprintln!("reading 1 word at M0 and decoding bits to M0..M15");
    let res = match client.read_words("M0", 1).await {
        Ok(r) => r,
        Err(e) => { eprintln!("read_words failed: {e}"); panic!("read failed"); }
    };

    // extract first word
    let nums = match res.get("data_blocks") {
        Some(db) => db.as_array().and_then(|arr| arr.first()).and_then(|block| block.as_array()).cloned(),
        None => None,
    };
    if nums.is_none() { eprintln!("unexpected response shape: {res}"); panic!("bad response"); }
    let nums = nums.unwrap();
    if nums.is_empty() { eprintln!("no words returned: {res}"); panic!("no words"); }
    let w = u16::try_from(nums[0].as_u64().unwrap_or(0)).unwrap_or(0);
    eprintln!("word at M0 = {w} (0x{w:04X})");
    let bits = word_to_bits_le(w);
    for (i, val) in bits.iter().enumerate().take(16) {
        println!("M{i} = {val}");
    }
}
