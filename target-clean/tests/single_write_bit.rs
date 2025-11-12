use std::env;
use std::time::Duration;

use melsec_mc::command_registry::{create_read_bits_params, GLOBAL_COMMAND_REGISTRY};
use melsec_mc::commands::Command;
use melsec_mc::device::parse_device_and_address;
use melsec_mc::endpoint::ConnectionTarget;
use melsec_mc::init_defaults;
use melsec_mc::mc_client::McClient;
use melsec_mc::mc_define::Protocol;

fn should_run() -> bool {
    env::var("RUN_REAL_TESTS")
        .map(|v| v == "1")
        .unwrap_or(false)
}
fn allow_write() -> bool {
    env::var("RUN_REAL_WRITE")
        .map(|v| v == "1")
        .unwrap_or(false)
}

fn build_client_for(addr: &str, tcp_port: u16, network_number: u8) -> McClient {
    init_defaults().expect("init defaults");
    let access_route = melsec_mc::mc_define::AccessRoute::default()
        .with_network_number(network_number)
        .with_pc_number(0xffu8)
        .with_io_number(0x03ff)
        .with_station_number(0x00u8);

    let target = ConnectionTarget::new()
        .with_ip(addr)
        .with_port(tcp_port)
        .with_access_route(access_route)
        .build();

    McClient::new()
        .with_target(target)
        .with_protocol(Protocol::Tcp)
        .with_monitoring_timer(5)
}

#[tokio::test]
async fn single_write_bit() {
    if !should_run() {
        eprintln!("skipping single_write_bit (set RUN_REAL_TESTS=1 to enable)");
        return;
    }
    if !allow_write() {
        eprintln!("skipping write (set RUN_REAL_WRITE=1 to enable)");
        return;
    }

    let dev_ip = env::var("PLC1_ADDR")
        .unwrap_or_else(|_| env::var("PLC_ADDR").unwrap_or_else(|_| "192.168.1.40".to_string()));
    let dev_port = env::var("PLC1_TCP_PORT")
        .ok()
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(4020u16);

    let client = build_client_for(&dev_ip, dev_port, 0u8).with_client_name("single_write_bit");

    let device = "M1000";
    let (dev, addr) = parse_device_and_address(device).expect("parse device");

    // read the original single bit
    let read_params = create_read_bits_params(device, 1);
    let reg = GLOBAL_COMMAND_REGISTRY.get().expect("global registry");
    let read_spec = reg.get(Command::ReadBits).expect("read_bits spec");
    let read_data = read_spec
        .build_request(&read_params, Some(client.plc_series))
        .expect("build read request");
    let read_payload = melsec_mc::request::McRequest::new()
        .with_access_route(client.target.access_route)
        .try_with_request_data(read_data)
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
            .expect("initial read send/recv");
    let resp = melsec_mc::response::McResponse::try_new(&read_raw).expect("parse response");
    assert!(!resp.has_end_code || resp.end_code == Some(0));
    let parsed = read_spec.parse_response(&serde_json::json!({ "data_blocks": [{ "count": 1u64 }], "start_addr": u64::from(addr), "device_code": u64::from(dev.device_code_q()), "count": 1u64 }), &resp.data).expect("parse response");
    // parse_response returns booleans for nibble/bit blocks; read as bool rather than u64
    // parse_response returns data_blocks: [ [point0, point1, ...] ] (array of blocks)
    let orig_val = parsed
        .get("data_blocks")
        .and_then(|v| v.as_array())
        .and_then(|a| a.first())
        .and_then(|blk| blk.as_array())
        .and_then(|b| b.first())
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);

    // write the inverse of original (toggle)
    let new_val = !orig_val;
    println!("original: {orig_val}, writing: {new_val}");
    let _ = client
        .write_bits(device, &[new_val])
        .await
        .expect("write_bits");

    // short delay to allow PLC to apply
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // verify
    let read_raw2 =
        melsec_mc::transport::send_and_recv_tcp(&client.target.addr, &read_payload, timeout)
            .await
            .expect("verify read send/recv");
    let resp2 = melsec_mc::response::McResponse::try_new(&read_raw2).expect("parse response");
    assert!(!resp2.has_end_code || resp2.end_code == Some(0));
    let parsed2 = read_spec.parse_response(&serde_json::json!({ "data_blocks": [{ "count": 1u64 }], "start_addr": u64::from(addr), "device_code": u64::from(dev.device_code_q()), "count": 1u64 }), &resp2.data).expect("parse response");
    let read_val = parsed2
        .get("data_blocks")
        .and_then(|v| v.as_array())
        .and_then(|a| a.first())
        .and_then(|blk| blk.as_array())
        .and_then(|b| b.first())
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);

    println!("read back: {read_val}");
    assert_eq!(read_val, new_val, "single bit writeback mismatch");

    // restore
    let _ = client.write_bits(device, &[orig_val]).await;
}
