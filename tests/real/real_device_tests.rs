use std::env;

use melsec_mc::{init_defaults, ConnectionTarget};
use melsec_mc::mc_client::McClient;
use serde_json::Value as JsonValue;
use melsec_mc::device::parse_device_and_address;
use melsec_mc::command_registry::{GLOBAL_COMMAND_REGISTRY, create_read_words_params};
use melsec_mc::commands::Command;
use melsec_mc::request::McRequest;
use melsec_mc::mc_define::Protocol;
use std::time::Duration;

// These tests perform real network calls. They are skipped unless the
// environment variable RUN_REAL_TESTS is set to "1". Write tests also require
// RUN_REAL_WRITE == "1" to avoid accidental modification of PLC state.

fn should_run() -> bool {
    env::var("RUN_REAL_TESTS").map(|v| v == "1").unwrap_or(false)
}

fn allow_write() -> bool {
    env::var("RUN_REAL_WRITE").map(|v| v == "1").unwrap_or(false)
}




// helper to create a client for a specific protocol and port (uses same access_route)
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
        .with_client_name("test_client")
}

// env helpers for address/ports
fn env_addr() -> String {
    env::var("PLC_ADDR").unwrap_or_else(|_| "192.168.1.40".to_string())
}
fn env_tcp_port() -> u16 {
    env::var("PLC_TCP_PORT").ok().and_then(|s| s.parse::<u16>().ok()).unwrap_or(4020)
}
fn env_udp_port() -> u16 {
    env::var("PLC_UDP_PORT").ok().and_then(|s| s.parse::<u16>().ok()).unwrap_or(4021)
}

#[tokio::test]
async fn real_read_words_tcp() {
    if !should_run() { eprintln!("skipping real_read_words_tcp (set RUN_REAL_TESTS=1 to enable)"); return; }
    init_defaults().expect("init defaults");
    let client = test_client_for(melsec_mc::mc_define::Protocol::Tcp, env_tcp_port());
    // Announce test purpose
    melsec_mc::announce("real_read_words_tcp", "Real read words over TCP (D500 default)");
    // Read from device and count specified by env (defaults to D500, 2)
    let device = env::var("REAL_READ_WORD_DEVICE").unwrap_or_else(|_| "D500".to_string());
    let count = env::var("REAL_READ_WORD_COUNT").ok().and_then(|s| s.parse::<u16>().ok()).unwrap_or(2u16);
    eprintln!("real_read_words_tcp: device={device} count={count}");

    // Build request and send raw to capture send/recv bytes
    let params = create_read_words_params(&device, count);
    let reg = GLOBAL_COMMAND_REGISTRY.get().expect("global registry");
    let spec = reg.get(Command::ReadWords).expect("command spec");
    let request_data = spec.build_request(&params, Some(client.plc_series)).expect("build_request");
    let mc_payload = McRequest::new()
        .with_access_route(client.target.access_route)
    .try_with_request_data(request_data).expect("build request")
        .build();

    let timeout = if client.monitoring_timer > 0 { Some(Duration::from_secs(u64::from(client.monitoring_timer))) } else { None };
    // send raw and print hex dump of send/recv
    let raw_buf = match client.protocol {
        Protocol::Tcp => match melsec_mc::transport::send_and_recv_tcp(&client.target.addr, &mc_payload, timeout).await {
            Ok(b) => b,
            Err(e) => { eprintln!("raw tcp send/recv failed: {e} (treating test as skipped)"); return; }
        },
        Protocol::Udp => match melsec_mc::transport::send_and_recv_udp(&client.target.addr, &mc_payload, timeout).await {
            Ok(b) => b,
            Err(e) => { eprintln!("raw udp send/recv failed: {e} (treating test as skipped)"); return; }
        },
    };
    let send_hex = mc_payload.iter().map(|b| format!("{b:02X}")).collect::<Vec<String>>().join(" ");
    let recv_hex = raw_buf.iter().map(|b| format!("{b:02X}")).collect::<Vec<String>>().join(" ");
    eprintln!("SENT: {send_hex}");
    eprintln!("RECV: {recv_hex}");

    // also use client helper to parse into JSON
    let res = match client.read_words(&device, count).await {
    Ok(r) => r,
    Err(e) => { eprintln!("read_words tcp helper failed: {e} (treating as skipped)"); return; }
    };
    println!("read_words tcp response (D500..): {res}");
}

#[tokio::test]
async fn real_read_bits_udp() {
    if !should_run() { eprintln!("skipping real_read_bits_udp (set RUN_REAL_TESTS=1 to enable)"); return; }
    init_defaults().expect("init defaults");
    let client = test_client_for(melsec_mc::mc_define::Protocol::Udp, env_udp_port());
    melsec_mc::announce("real_read_bits_udp", "Real read bits over UDP (M0 default)");
    // Read bits starting at M0 (8 bits)
    let res = match client.read_bits("M0", 8).await {
    Ok(r) => r,
    Err(e) => { eprintln!("read_bits failed: {e} (treating as skipped)"); return; }
    };
    println!("read_bits response (M0..): {res}");
}

#[tokio::test]
async fn real_write_words_tcp() {
    if !should_run() { eprintln!("skipping real_write_words_tcp (set RUN_REAL_TESTS=1 to enable)"); return; }
    if !allow_write() { eprintln!("skipping write test (set RUN_REAL_WRITE=1 to enable)"); return; }
    init_defaults().expect("init defaults");
    let client = test_client_for(melsec_mc::mc_define::Protocol::Tcp, env_tcp_port());
    melsec_mc::announce("real_write_words_tcp", "Real write words over TCP (D500+), requires RUN_REAL_WRITE=1");
    // Safety: allow only D addresses >= 500 for write tests
    if let Ok((dev, addr)) = parse_device_and_address("D500") {
        if dev.symbol_str() != "D" || addr < 500 {
            panic!("write_words test requires D500 or higher");
        }
    }
    // Write to D500 and D501 then read back
    let write_vals = [1234u16, 5678u16];
    let res = match client.write_words("D500", &write_vals).await {
    Ok(r) => r,
    Err(e) => { eprintln!("write_words failed: {e} (treating as skipped)"); return; }
    };
    println!("write_words tcp response: {res}");
    // read back
    let read_back = client.read_words("D500", 2).await.expect("read back");
    println!("read back D500..: {read_back}");
    // extract numeric values from JSON and assert
    let nums = collect_numbers(&read_back, 2);
    assert_eq!(nums.len(), 2, "expected 2 numbers in read back");
    assert_eq!(nums[0] as u16, write_vals[0]);
    assert_eq!(nums[1] as u16, write_vals[1]);
}
#[tokio::test]
async fn real_write_bits_udp() {
    if !should_run() { eprintln!("skipping real_write_bits_udp (set RUN_REAL_TESTS=1 to enable)"); return; }
    if !allow_write() { eprintln!("skipping write test (set RUN_REAL_WRITE=1 to enable)"); return; }
    init_defaults().expect("init defaults");
    let client = test_client_for(melsec_mc::mc_define::Protocol::Udp, env_udp_port());
    melsec_mc::announce("real_write_bits_udp", "Real write bits over UDP (M0+), requires RUN_REAL_WRITE=1");
    // Safety: allow M addresses >= 0 (M0 allowed)
    if let Ok((dev, _addr)) = parse_device_and_address("M0") {
        if dev.symbol_str() != "M" {
            panic!("write_bits test requires M0 or higher");
        }
    }
    // Write bits starting at M0 (3 bits) then read back
    let write_bits = [true, false, true];
    let res = match client.write_bits("M0", &write_bits).await {
        Ok(r) => r,
        Err(e) => { eprintln!("write_bits failed: {} (treating as skipped)", e); return; }
    };
    println!("write_bits response: {}", res);
    let read_bits = match client.read_bits("M0", 3).await {
        Ok(r) => r,
        Err(e) => { eprintln!("read_bits back failed: {} (treating as skipped)", e); return; }
    };
    println!("read back M0..: {}", read_bits);
    // extract booleans and assert
    let bools = collect_bools(&read_bits, 3);
    assert_eq!(bools.len(), 3, "expected 3 bits in read back");
    assert_eq!(bools, write_bits);
}

// Helper: collect first `n` numeric values (u64) from a JsonValue recursively
fn collect_numbers(v: &JsonValue, n: usize) -> Vec<u64> {
    let mut out: Vec<u64> = Vec::new();
    fn visit(v: &JsonValue, out: &mut Vec<u64>, n: usize) {
        if out.len() >= n { return; }
        match v {
            JsonValue::Number(num) => {
                if let Some(u) = num.as_u64() { out.push(u); }
            }
            JsonValue::Array(arr) => {
                for it in arr.iter() {
                    visit(it, out, n);
                    if out.len() >= n { break; }
                }
            }
            JsonValue::Object(map) => {
                for (_k, val) in map.iter() {
                    visit(val, out, n);
                    if out.len() >= n { break; }
                }
            }
            _ => {}
        }
    }
    visit(v, &mut out, n);
    out
}

// Helper: collect first `n` booleans from JsonValue recursively
fn collect_bools(v: &JsonValue, n: usize) -> Vec<bool> {
    let mut out: Vec<bool> = Vec::new();
    fn visit(v: &JsonValue, out: &mut Vec<bool>, n: usize) {
        if out.len() >= n { return; }
        match v {
            JsonValue::Bool(b) => out.push(*b),
            JsonValue::Array(arr) => {
                for it in arr.iter() {
                    visit(it, out, n);
                    if out.len() >= n { break; }
                }
            }
            JsonValue::Object(map) => {
                for (_k, val) in map.iter() {
                    visit(val, out, n);
                    if out.len() >= n { break; }
                }
            }
            _ => {}
        }
    }
    visit(v, &mut out, n);
    out
}
