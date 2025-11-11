use std::env;
use std::time::Duration;

use melsec_mc::command_registry::{create_read_words_params, GLOBAL_COMMAND_REGISTRY};
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
async fn single_write_words() {
    if !should_run() {
        eprintln!("skipping single_write_words (set RUN_REAL_TESTS=1 to enable)");
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

    let client = build_client_for(&dev_ip, dev_port, 0u8).with_client_name("single_write_words");

    // use word device D1000
    let device = "D1000";
    let (dev, addr) = parse_device_and_address(device).expect("parse device");

    // read original 2 words
    let read_params = create_read_words_params(device, 2);
    let reg = GLOBAL_COMMAND_REGISTRY.get().expect("global registry");
    let read_spec = reg.get(Command::ReadWords).expect("read_words spec");
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

    // prepare write values
    let write_vals: Vec<u16> = vec![0x1234u16, 0x5678u16];
    let _ = client
        .write_words(device, &write_vals)
        .await
        .expect("write_words");

    // verify read back
    let read_raw2 =
        melsec_mc::transport::send_and_recv_tcp(&client.target.addr, &read_payload, timeout)
            .await
            .expect("verify read send/recv");
    let resp2 = melsec_mc::response::McResponse::try_new(&read_raw2).expect("parse response");
    assert!(!resp2.has_end_code || resp2.end_code == Some(0));

    let parsed_json = read_spec.parse_response(&serde_json::json!({ "data_blocks": [{ "count": 2u64 }], "start_addr": u64::from(addr), "device_code": u64::from(dev.device_code_q()), "count": 2u64 }), &resp2.data).expect("parse response");
    let db = parsed_json
        .get("data_blocks")
        .and_then(|v| v.as_array())
        .expect("data_blocks array");
    let vals = db[0].as_array().unwrap();
    let read_words: Vec<u16> = vals
        .iter()
        .map(|v| u16::try_from(v.as_u64().unwrap_or(0)).unwrap_or(0))
        .collect();

    println!("sent words: {write_vals:04X?}");
    println!("read words: {read_words:04X?}");

    // equality check
    assert_eq!(read_words, write_vals, "word readback mismatch");

    // restore original if possible
    let orig_parsed = read_spec.parse_response(&serde_json::json!({ "data_blocks": [{ "count": 2u64 }], "start_addr": u64::from(addr), "device_code": u64::from(dev.device_code_q()), "count": 2u64 }), &original).ok();
    if let Some(orig_json) = orig_parsed {
        if let Some(db2) = orig_json.get("data_blocks").and_then(|v| v.as_array()) {
            let vals2 = db2[0].as_array().unwrap();
            let orig_words: Vec<u16> = vals2
                .iter()
                .map(|n| u16::try_from(n.as_u64().unwrap_or(0)).unwrap_or(0))
                .collect();
            let _ = client.write_words(device, &orig_words).await;
        }
    }
}
