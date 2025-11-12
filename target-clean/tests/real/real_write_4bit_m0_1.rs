use std::env;
use std::time::Duration;

use serde_json::json;

use melsec_mc::{init_defaults};
use melsec_mc::mc_client::McClient;
use melsec_mc::mc_define::Protocol;
use melsec_mc::endpoint::ConnectionTarget;
use melsec_mc::request::McRequest;
use melsec_mc::command_registry::{GLOBAL_COMMAND_REGISTRY};
use melsec_mc::device::parse_device_and_address;
use melsec_mc::commands::Command;

fn should_run() -> bool {
    env::var("RUN_REAL_TESTS").map(|v| v == "1").unwrap_or(false)
}
fn allow_write() -> bool {
    env::var("RUN_REAL_WRITE").map(|v| v == "1").unwrap_or(false)
}

fn env_addr() -> String {
    env::var("PLC_ADDR").unwrap_or_else(|_| "192.168.1.40".to_string())
}
fn env_tcp_port() -> u16 {
    env::var("PLC_TCP_PORT").ok().and_then(|s| s.parse::<u16>().ok()).unwrap_or(4020)
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
        .with_client_name("real_write_4bit_m0_1_client")
}

#[tokio::test]
async fn real_write_4bit_m0_1() {
    if !should_run() { eprintln!("skipping real_write_4bit_m0_1 (set RUN_REAL_TESTS=1 to enable)"); return; }
    if !allow_write() { eprintln!("skipping write test (set RUN_REAL_WRITE=1 to enable)"); return; }

    let client = build_client();
    melsec_mc::announce("real_write_4bit_m0_1", "Write 4-bit (nibble) payload (high-first) to M0..M1");
    let device = "M0";
    // parse device
    let (dev, addr) = parse_device_and_address(device).expect("parse device");

    // Build params manually for high-first nibble ordering: M0=ON, M1=OFF -> byte 0x10
    let params = json!({
        "start_addr": u64::from(addr),
        "device_code": u64::from(dev.device_code_q()),
        "count": 2u64,
        // payload is array of bytes (rest). single byte 0x10 encodes [M0=1, M1=0] in high-first order
        "payload": [0x10u8]
    });

    let reg = GLOBAL_COMMAND_REGISTRY.get().expect("global registry");
    let spec = reg.get(Command::WriteBits).expect("write_bits spec");
    let request_data = spec.build_request(&params, Some(client.plc_series)).expect("build_request");

    let mc_payload = McRequest::new()
        .with_access_route(client.target.access_route)
    .try_with_request_data(request_data).expect("build request")
        .build();

    let timeout = if client.monitoring_timer > 0 { Some(Duration::from_secs(u64::from(client.monitoring_timer))) } else { None };
    let raw_buf = match client.protocol {
        Protocol::Tcp => melsec_mc::transport::send_and_recv_tcp(&client.target.addr, &mc_payload, timeout).await.expect("tcp send/recv"),
        Protocol::Udp => melsec_mc::transport::send_and_recv_udp(&client.target.addr, &mc_payload, timeout).await.expect("udp send/recv"),
    };

    let send_hex = mc_payload.iter().map(|b| format!("{b:02X}")).collect::<Vec<String>>().join(" ");
    let recv_hex = raw_buf.iter().map(|b| format!("{b:02X}")).collect::<Vec<String>>().join(" ");
    eprintln!("SENT: {send_hex}");
    eprintln!("RECV: {recv_hex}");
}
