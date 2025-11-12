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
async fn single_write_nibbles() {
    if !should_run() {
        eprintln!("skipping single_write_nibbles (set RUN_REAL_TESTS=1 to enable)");
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

    // use network_number 0 for device
    let client = build_client_for(&dev_ip, dev_port, 0u8).with_client_name("single_write_dev1");

    // target device/address
    let device = "M1000"; // conservative allowed area
    let (dev, addr) = parse_device_and_address(device).expect("parse device");

    // read original 4 points
    let read_params = create_read_bits_params(device, 4);
    let reg = GLOBAL_COMMAND_REGISTRY.get().expect("global registry");
    let read_spec = reg.get(Command::ReadBits).expect("read_bits spec");
    let read_data = read_spec
        .build_request(&read_params, Some(client.plc_series))
        .expect("build read request");
    let read_payload = McRequest::new()
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
    let original = resp.data.clone();

    // M is a bit device; use write_bits (booleans) instead of write_nibbles
    let bools: Vec<bool> = vec![true, false, true, false];

    // perform write_bits via McClient
    let _ = client.write_bits(device, &bools).await.expect("write_bits");

    // verify read back
    let read_raw2 =
        melsec_mc::transport::send_and_recv_tcp(&client.target.addr, &read_payload, timeout)
            .await
            .expect("verify read send/recv");
    let resp2 = melsec_mc::response::McResponse::try_new(&read_raw2).expect("parse response");
    assert!(!resp2.has_end_code || resp2.end_code == Some(0));

    // parse and check values as booleans
    let parsed_json = read_spec.parse_response(&serde_json::json!({ "data_blocks": [{ "count": 4u64 }], "start_addr": u64::from(addr), "device_code": u64::from(dev.device_code_q()), "count": 4u64 }), &resp2.data).expect("parse response");
    let db = parsed_json
        .get("data_blocks")
        .and_then(|v| v.as_array())
        .expect("data_blocks array");
    let vals = db[0].as_array().unwrap();
    let read_bools: Vec<bool> = vals.iter().map(|v| v.as_bool().unwrap_or(false)).collect();

    println!("sent bools: {bools:?}");
    println!("read bools: {read_bools:?}");

    // basic equality check
    assert_eq!(read_bools, bools, "bool readback mismatch");

    // restore original (best-effort) by writing bits interpreted from original
    let orig_parsed = read_spec.parse_response(&serde_json::json!({ "data_blocks": [{ "count": 4u64 }], "start_addr": u64::from(addr), "device_code": u64::from(dev.device_code_q()), "count": 4u64 }), &original).ok();
    if let Some(orig_json) = orig_parsed {
        if let Some(db2) = orig_json.get("data_blocks").and_then(|v| v.as_array()) {
            let vals2 = db2[0].as_array().unwrap();
            let bools: Vec<bool> = vals2.iter().map(|n| n.as_bool().unwrap_or(false)).collect();
            let _ = client.write_bits(device, &bools).await;
        }
    }
}
