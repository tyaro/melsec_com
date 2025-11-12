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
async fn parallel_read_only() {
    if !should_run() {
        eprintln!("skipping parallel_read_only (set RUN_REAL_TESTS=1 to enable)");
        return;
    }

    let dev1_ip = env::var("PLC1_ADDR")
        .unwrap_or_else(|_| env::var("PLC_ADDR").unwrap_or_else(|_| "192.168.1.40".to_string()));
    let dev1_port = env::var("PLC1_TCP_PORT")
        .ok()
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(4020u16);
    let dev2_ip = env::var("PLC2_ADDR")
        .unwrap_or_else(|_| env::var("PLC_ADDR").unwrap_or_else(|_| "192.168.1.42".to_string()));
    let dev2_port = env::var("PLC2_TCP_PORT")
        .ok()
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(4300u16);

    let h1 = tokio::spawn(async move {
        let client = build_client_for(&dev1_ip, dev1_port, 0u8).with_client_name("ronly_dev1");
        let device = "M1000";
        let (_dev, _addr) = parse_device_and_address(device).expect("parse device");
        let read_params = create_read_bits_params(device, 20);
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
        let buf =
            melsec_mc::transport::send_and_recv_tcp(&client.target.addr, &read_payload, timeout)
                .await
                .expect("send/recv dev1");
        let resp = melsec_mc::response::McResponse::try_new(&buf).expect("parse response");
        assert!(!resp.has_end_code || resp.end_code == Some(0));
        Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
    });

    let h2 = tokio::spawn(async move {
        let client = build_client_for(&dev2_ip, dev2_port, 0u8).with_client_name("ronly_dev2");
        let device = "M1100";
        let (_dev, _addr) = parse_device_and_address(device).expect("parse device");
        let read_params = create_read_bits_params(device, 20);
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
        let buf =
            melsec_mc::transport::send_and_recv_tcp(&client.target.addr, &read_payload, timeout)
                .await
                .expect("send/recv dev2");
        let resp = melsec_mc::response::McResponse::try_new(&buf).expect("parse response");
        assert!(!resp.has_end_code || resp.end_code == Some(0));
        Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
    });

    h1.await.expect("h1 panicked").expect("h1 error");
    h2.await.expect("h2 panicked").expect("h2 error");
}
