use std::env;

use melsec_mc::{init_defaults, ConnectionTarget};
use melsec_mc::mc_client::McClient;
// serde_json::Value not used in this example; keep imports minimal
use melsec_mc::device::parse_device_and_address;
use melsec_mc::command_registry::{GLOBAL_COMMAND_REGISTRY};
use melsec_mc::commands::Command;
use melsec_mc::request::McRequest;
use melsec_mc::mc_define::Protocol;
use std::time::Duration;

// Opt-in runner: requires RUN_REAL_TESTS=1 and RUN_REAL_WRITE=1
fn should_run() -> bool {
    env::var("RUN_REAL_TESTS").map(|v| v == "1").unwrap_or(false)
}
fn allow_write() -> bool {
    env::var("RUN_REAL_WRITE").map(|v| v == "1").unwrap_or(false)
}

fn env_addr() -> String {
    env::var("PLC_ADDR").unwrap_or_else(|_| "192.168.1.40".to_string())
}
fn env_udp_port() -> u16 {
    env::var("PLC_UDP_PORT").ok().and_then(|s| s.parse::<u16>().ok()).unwrap_or(4021)
}

// same client helper as other real tests
fn test_client_for(proto: melsec_mc::mc_define::Protocol, port: u16) -> McClient {
    init_defaults().expect("init defaults");

    let access_route = melsec_mc::mc_define::AccessRoute::default()
    .with_network_number(0x00u8)
    .with_pc_number(0xffu8)
    .with_io_number(0x03ff)
    .with_station_number(0x00u8);

    let addr = env_addr();

    let taget = ConnectionTarget::new()
        .with_ip(&addr)
        .with_port(port)
        .with_access_route(access_route)
        .build();

    McClient::new()
        .with_target(taget)
        .with_protocol(proto)
        .with_monitoring_timer(5)
        .with_client_name("packing_test_client")
}

// packing helpers
fn pack_high_nibble(values: &[bool]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < values.len() {
        let high = if values[i] { 1u8 } else { 0u8 };
        let low = if i+1 < values.len() { if values[i+1] { 1u8 } else { 0u8 } } else { 0u8 };
        out.push((high << 4) | (low & 0x0F));
        i += 2;
    }
    out
}
fn pack_low_nibble(values: &[bool]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < values.len() {
        let low = if values[i] { 1u8 } else { 0u8 };
        let high = if i+1 < values.len() { if values[i+1] { 1u8 } else { 0u8 } } else { 0u8 };
        out.push((high << 4) | (low & 0x0F));
        i += 2;
    }
    out
}
fn pack_one_byte_per_bit(values: &[bool]) -> Vec<u8> {
    values.iter().map(|b| if *b { 1u8 } else { 0u8 }).collect()
}
fn pack_lsb_bit_packed(values: &[bool]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut byte = 0u8;
    let mut bitpos = 0u8;
    for v in values.iter() {
        if *v { byte |= 1u8 << bitpos; }
        bitpos += 1;
        if bitpos == 8 {
            out.push(byte);
            byte = 0u8;
            bitpos = 0;
        }
    }
    if bitpos != 0 { out.push(byte); }
    out
}
fn pack_msb_bit_packed(values: &[bool]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut byte = 0u8;
    let mut bitpos = 0usize;
    for v in values.iter() {
        let msb_index = 7 - (bitpos % 8);
        if *v { byte |= 1u8 << msb_index; }
        bitpos += 1;
        if bitpos % 8 == 0 {
            out.push(byte);
            byte = 0u8;
        }
    }
    if bitpos % 8 != 0 { out.push(byte); }
    out
}

#[tokio::test]
async fn real_write_packing_variants() {
    if !should_run() { eprintln!("skipping real_write_packing_variants (set RUN_REAL_TESTS=1 to enable)"); return; }
    if !allow_write() { eprintln!("skipping write tests (set RUN_REAL_WRITE=1 to enable)"); return; }

    let client = test_client_for(Protocol::Udp, env_udp_port());

    melsec_mc::announce("real_write_packing_variants", "Write same logical bits using multiple packing variants and verify read-back");

    // target device and test pattern
    let device = "M0";
    let values: Vec<bool> = vec![true,false,true,false,true,false,true,false,true,false,true]; // 11 points
    let count = values.len() as u16;

    // get device code and addr
    let (dev, addr) = parse_device_and_address(device).expect("parse_device");
    let device_code = dev.device_code_q() as u64;

    // get registry and spec for WriteBits
    let reg = GLOBAL_COMMAND_REGISTRY.get().expect("global registry");
    let spec = reg.get(Command::WriteBits).expect("spec");

    // list of variants
    let variants: Vec<(&str, Vec<u8>)> = vec![
        ("high_nibble_first", pack_high_nibble(&values)),
        ("low_nibble_first", pack_low_nibble(&values)),
        ("one_byte_per_bit", pack_one_byte_per_bit(&values)),
        ("lsb_bit_packed", pack_lsb_bit_packed(&values)),
        ("msb_bit_packed", pack_msb_bit_packed(&values)),
    ];

    eprintln!("real_write_packing_variants: device={device} count={count}");

    for (name, payload) in variants.into_iter() {
    eprintln!("--- testing variant: {name} ---");
        // build params
        let params = serde_json::json!({
            "start_addr": u64::from(addr),
            "device_code": device_code,
            "count": count as u64,
            "payload": payload.iter().map(|b| *b as u64).collect::<Vec<u64>>()
        });
        // build request bytes
        let request_data = match spec.build_request(&params, Some(client.plc_series)) {
            Ok(b) => b,
            Err(e) => { eprintln!("build_request error for {}: {}", name, e); continue; }
        };
        let mc_payload = McRequest::new()
            .with_access_route(client.target.access_route)
            .try_with_request_data(request_data.clone()).expect("build request")
            .build();

        let timeout = if client.monitoring_timer > 0 { Some(Duration::from_secs(client.monitoring_timer as u64)) } else { None };
        // send raw and capture
        let raw_buf = match client.protocol {
            Protocol::Tcp => match melsec_mc::transport::send_and_recv_tcp(&client.target.addr, &mc_payload, timeout).await {
                Ok(b) => b,
                Err(e) => { eprintln!("raw tcp send/recv failed for {name}: {e} (skipping)"); continue; }
            },
            Protocol::Udp => match melsec_mc::transport::send_and_recv_udp(&client.target.addr, &mc_payload, timeout).await {
                Ok(b) => b,
                Err(e) => { eprintln!("raw udp send/recv failed for {name}: {e} (skipping)"); continue; }
            },
        };
    let send_hex = mc_payload.iter().map(|b| format!("{b:02X}")).collect::<Vec<String>>().join(" ");
    let recv_hex = raw_buf.iter().map(|b| format!("{b:02X}")).collect::<Vec<String>>().join(" ");
    eprintln!("SENT ({name}) {send_hex}");
    eprintln!("RECV ({name}) {recv_hex}");

        // parse end code via McResponse and attempt a read-back to verify
        let resp = match client.read_bits(device, count).await {
            Ok(r) => r,
            Err(e) => { eprintln!("read-back failed after {name}: {e}"); continue; }
        };
        eprintln!("read back after {name}: {resp}");
    }

    // done
}
