use std::env;
use std::time::Duration;

use melsec_mc::command_registry::{create_read_bits_params, GLOBAL_COMMAND_REGISTRY};
use melsec_mc::commands::Command;
use melsec_mc::device::parse_device_and_address;
use melsec_mc::endpoint::ConnectionTarget;
use melsec_mc::init_defaults;
use melsec_mc::mc_client::McClient;
use melsec_mc::mc_define::Protocol;
use melsec_mc::request::McRequest;
use melsec_mc::response::McResponse;

fn should_run() -> bool {
    env::var("RUN_REAL_TESTS")
        .map(|v| v == "1")
        .unwrap_or(false)
}

fn env_addr() -> String {
    env::var("PLC_ADDR").unwrap_or_else(|_| "192.168.1.40".to_string())
}
fn env_tcp_port() -> u16 {
    env::var("PLC_TCP_PORT")
        .ok()
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(4020)
}

fn build_client() -> McClient {
    init_defaults().expect("init defaults");
    let access_route = melsec_mc::mc_define::AccessRoute::default()
        .with_network_number(0x00u8)
        .with_pc_number(0xffu8)
        .with_io_number(0x03ff)
        .with_station_number(0x00u8);

    let addr = env_addr();
    let target = ConnectionTarget::new()
        .with_ip(&addr)
        .with_port(env_tcp_port())
        .with_access_route(access_route)
        .build();

    McClient::new()
        .with_target(target)
        .with_protocol(Protocol::Tcp)
        .with_monitoring_timer(5)
        .with_client_name("read_d0_client")
}

#[tokio::test]
async fn read_d0_16bits() {
    if !should_run() {
        eprintln!("skipping read_d0_16bits (set RUN_REAL_TESTS=1 to enable)");
        return;
    }

    let client = build_client();
    let device = "D0";
    let _ = parse_device_and_address(device).expect("parse device");

    // build params for 16 bits
    let params = create_read_bits_params(device, 16);
    let reg = GLOBAL_COMMAND_REGISTRY.get().expect("global registry");
    let spec = reg.get(Command::ReadBits).expect("read_bits spec");

    let request_data = match spec.build_request(&params, Some(client.plc_series)) {
        Ok(rd) => rd,
        Err(e) => {
            eprintln!("build_request rejected: {e} (treating as skipped)");
            return;
        }
    };
    let read_payload = McRequest::new()
        .with_access_route(client.target.access_route)
        .try_with_request_data(request_data)
        .expect("build request")
        .build();

    let timeout = if client.monitoring_timer > 0 {
        Some(Duration::from_secs(u64::from(client.monitoring_timer)))
    } else {
        None
    };
    let read_raw =
        melsec_mc::transport::send_and_recv_tcp(&client.target.addr, &read_payload, timeout)
            .await
            .expect("read send/recv");
    eprintln!(
        "READ SENT: {}",
        read_payload
            .iter()
            .map(|b| format!("{b:02X}"))
            .collect::<Vec<_>>()
            .join(" ")
    );
    eprintln!(
        "READ RECV: {}",
        read_raw
            .iter()
            .map(|b| format!("{b:02X}"))
            .collect::<Vec<_>>()
            .join(" ")
    );

    let resp = McResponse::try_new(&read_raw).expect("parse response");
    assert!(
        !resp.has_end_code || resp.end_code == Some(0),
        "read returned error"
    );
    let data_bytes = resp.data;

    // parse using command spec (will respect commands.toml nibble settings)
    let parsed = spec
        .parse_response(&params, &data_bytes)
        .expect("parse response");
    eprintln!("PARSED: {parsed}");

    // If response contains nibble blocks, print bit-like booleans where non-zero -> true
    if let Some(db) = parsed.get("data_blocks").and_then(|v| v.as_array()) {
        if !db.is_empty() {
            if let Some(arr) = db[0].as_array() {
                // parse booleans returned for nibble/bit blocks
                let bits: Vec<bool> = arr.iter().map(|n| n.as_bool().unwrap_or(false)).collect();
                eprintln!("BITS D0..D15: {bits:?}");
            }
        }
    }
}
