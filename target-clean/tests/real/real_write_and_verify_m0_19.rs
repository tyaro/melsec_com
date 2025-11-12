use std::env;
use std::time::Duration;

use melsec_mc::{init_defaults};
use melsec_mc::mc_client::McClient;
use melsec_mc::mc_define::Protocol;
use melsec_mc::endpoint::ConnectionTarget;
use melsec_mc::request::McRequest;
use melsec_mc::command_registry::{GLOBAL_COMMAND_REGISTRY, create_read_bits_params};
use melsec_mc::device::parse_device_and_address;
use melsec_mc::commands::Command;
use melsec_mc::response::McResponse;

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
        .with_client_name("real_write_verify_m0_19_client")
}



#[tokio::test]
async fn real_write_and_verify_m0_19() {
    if !should_run() { eprintln!("skipping real_write_and_verify_m0_19 (set RUN_REAL_TESTS=1 to enable)"); return; }
    if !allow_write() { eprintln!("skipping write test (set RUN_REAL_WRITE=1 to enable)"); return; }

    let client = build_client();
    melsec_mc::announce("real_write_and_verify_m0_19", "Write 20 bits M0..M19 then read back and verify pattern");
    let device = "M0";
    let (dev, addr) = parse_device_and_address(device).expect("parse device");

    // Use McClient::write_bits to perform the write using boolean values.
    let bit_values: Vec<bool> = (0..20).map(|i| (i % 2) == 0).collect();
    let write_resp = client.write_bits(device, &bit_values).await.expect("write_bits");
    // Optionally inspect parsed response (usually empty success block)
    eprintln!("WRITE RESP PARSED: {}", write_resp);

    // Now perform a raw ReadBits for 20 points starting at M0
    let read_params = create_read_bits_params(device, 20);
    let reg = GLOBAL_COMMAND_REGISTRY.get().expect("global registry");
    let read_spec = reg.get(Command::ReadBits).expect("read_bits spec");
    let read_data = read_spec.build_request(&read_params, Some(client.plc_series)).expect("build read request");
    let read_payload = McRequest::new().with_access_route(client.target.access_route).try_with_request_data(read_data).expect("build request").build();
    let timeout = if client.monitoring_timer > 0 { Some(Duration::from_secs(u64::from(client.monitoring_timer))) } else { None };
    let read_raw = melsec_mc::transport::send_and_recv_tcp(&client.target.addr, &read_payload, timeout).await.expect("read send/recv");
    eprintln!("READ SENT: {}", read_payload.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" "));
    eprintln!("READ RECV: {}", read_raw.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" "));

    // Parse MC response and extract data bytes
    let resp = McResponse::try_new(&read_raw).expect("parse response");
    assert!(!resp.has_end_code || resp.end_code == Some(0), "read returned error");
    let data_bytes = resp.data;

    // Use CommandSpec::parse_response to decode nibble blocks as defined in commands.toml
    let parse_params = serde_json::json!({
        "data_blocks": [{ "count": 20u64 }],
        "start_addr": u64::from(addr),
        "device_code": u64::from(dev.device_code_q()),
        "count": 20u64
    });
    let parsed = read_spec.parse_response(&parse_params, &data_bytes).expect("parse response");
    let db = parsed.get("data_blocks").and_then(|v| v.as_array()).expect("data_blocks array");
    let vals = db[0].as_array().unwrap();
    // convert nibble values to booleans (>0 => true)
    let bits: Vec<bool> = vals.iter().map(|n| n.as_bool().unwrap_or(false)).collect();
    eprintln!("READ BITS M0..M19: {bits:?}");

    // expected pattern: true,false,true,false,... starting with M0=true
    for (i, val) in bits.iter().enumerate().take(20) {
        let expected = (i % 2) == 0;
    assert_eq!(*val, expected, "bit {i} mismatch");
    }
}
