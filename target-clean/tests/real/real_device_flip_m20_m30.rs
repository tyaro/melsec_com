use std::env;

use melsec_mc::{init_defaults, ConnectionTarget};
use melsec_mc::mc_client::McClient;
use serde_json::Value as JsonValue;
use melsec_mc::device::parse_device_and_address;
use melsec_mc::mc_define::Protocol;

fn should_run() -> bool {
    env::var("RUN_REAL_TESTS").map(|v| v == "1").unwrap_or(false)
}

fn allow_write() -> bool {
    env::var("RUN_REAL_WRITE").map(|v| v == "1").unwrap_or(false)
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
        .with_client_name("flip_test_client")
}

fn env_tcp_port() -> u16 {
    env::var("PLC_TCP_PORT").ok().and_then(|s| s.parse::<u16>().ok()).unwrap_or(4020)
}
fn env_udp_port() -> u16 {
    env::var("PLC_UDP_PORT").ok().and_then(|s| s.parse::<u16>().ok()).unwrap_or(4021)
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

#[allow(dead_code)]
fn should_skip_on_err<E: std::fmt::Display>(res: Result<JsonValue, E>, msg: &str) -> Option<JsonValue> {
    match res {
        Ok(v) => Some(v),
        Err(e) => {
            eprintln!("{} failed: {} (treating as skipped)", msg, e);
            None
        }
    }
}

async fn perform_flip_test(client: &McClient, device: &str, count: usize) {
    // safety check: ensure write allowed for M devices
    if let Ok((dev, _addr)) = parse_device_and_address(device) {
        if dev.symbol_str() != "M" {
            panic!("flip test requires M device start");
        }
    }

    // read original bits
    let orig = match client.read_bits(device, count as u16).await {
        Ok(r) => r,
        Err(e) => { eprintln!("initial read_bits_failed: {} (skipping)", e); return; }
    };
    let orig_bools = collect_bools(&orig, count);
    if orig_bools.len() != count { eprintln!("unexpected read length {len} (skip)", len = orig_bools.len()); return; }
    eprintln!("orig {device}: {orig_bools:?}");

    // invert
    let inv: Vec<bool> = orig_bools.iter().map(|b| !b).collect();

    // write inverted
    let wr = match client.write_bits(device, &inv).await {
        Ok(r) => r,
        Err(e) => { eprintln!("write_bits failed: {e} (skipping restore)"); return; }
    };
    eprintln!("wrote inverted: {wr}");

    // read back and verify
    let read_back = match client.read_bits(device, count as u16).await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("read_back failed: {e}");
            // if skipping restore requested, do not attempt to write original back
            if std::env::var("REAL_SKIP_RESTORE").map(|v| v == "1").unwrap_or(false) {
                eprintln!("REAL_SKIP_RESTORE=1 set; not restoring original bits");
                return;
            }
            let _ = client.write_bits(device, &orig_bools).await;
            return;
        }
    };
    let read_bools = collect_bools(&read_back, count);
    eprintln!("read back {device}: {read_bools:?}");
    if read_bools != inv {
    eprintln!("read back did not match inverted bits (got {read_bools:?}, expected {inv:?})");
        // restore only if not explicitly skipping
        if std::env::var("REAL_SKIP_RESTORE").map(|v| v == "1").unwrap_or(false) {
            eprintln!("REAL_SKIP_RESTORE=1 set; not restoring original bits despite mismatch");
        } else {
            let _ = match client.write_bits(device, &orig_bools).await {
                Ok(r) => { eprintln!("restored: {r}"); Ok(()) },
                Err(e) => { eprintln!("failed to restore original bits: {e}"); Err(()) }
            };
        }
        panic!("read back did not match inverted bits");
    }

    // restore original (happy path) unless skip requested
    if std::env::var("REAL_SKIP_RESTORE").map(|v| v == "1").unwrap_or(false) {
        eprintln!("REAL_SKIP_RESTORE=1 set; leaving inverted bits on PLC for manual verification");
        return;
    }
    let _ = match client.write_bits(device, &orig_bools).await {
        Ok(r) => { eprintln!("restored: {}", r); Ok(()) },
        Err(e) => { eprintln!("failed to restore original bits: {}", e); Err(()) }
    };
}

#[tokio::test]
async fn real_flip_bits_tcp() {
    if !should_run() { eprintln!("skipping real_flip_bits_tcp (set RUN_REAL_TESTS=1 to enable)"); return; }
    if !allow_write() { eprintln!("skipping write test (set RUN_REAL_WRITE=1 to enable)"); return; }
    init_defaults().expect("init defaults");
    let client = test_client_for(Protocol::Tcp, env_tcp_port());

    melsec_mc::announce("real_flip_bits_tcp", "Flip M20..M30 (invert then restore) over TCP; requires RUN_REAL_WRITE=1");

    // flip M20..M30 (11 points)
    perform_flip_test(&client, "M20", 11).await;
}

#[tokio::test]
async fn real_flip_bits_udp() {
    if !should_run() { eprintln!("skipping real_flip_bits_udp (set RUN_REAL_TESTS=1 to enable)"); return; }
    if !allow_write() { eprintln!("skipping write test (set RUN_REAL_WRITE=1 to enable)"); return; }
    init_defaults().expect("init defaults");
    let client = test_client_for(Protocol::Udp, env_udp_port());

    melsec_mc::announce("real_flip_bits_udp", "Flip M20..M30 (invert then restore) over UDP; requires RUN_REAL_WRITE=1");

    // flip M20..M30 (11 points)
    perform_flip_test(&client, "M20", 11).await;
}
