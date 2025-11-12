use std::env;
use std::time::Duration;

use melsec_mc::{command_registry::create_read_bits_params, init_defaults, request::McRequest};

#[tokio::test]
async fn decode_read_bits_m0_20() {
    init_defaults().ok();

    if !env::var("RUN_REAL_TESTS")
        .map(|v| v == "1")
        .unwrap_or(false)
    {
        eprintln!("skipping decode_read_bits_m0_20 (set RUN_REAL_TESTS=1)");
        return;
    }

    let addr = env::var("PLC_ADDR").unwrap_or_else(|_| "192.168.1.40".to_string());
    let tcp_port = env::var("PLC_TCP_PORT")
        .ok()
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(4020);

    let device = "M0";
    let count: u16 = 20;

    // Build request_data using command registry
    let reg = melsec_mc::command_registry::GLOBAL_COMMAND_REGISTRY
        .get()
        .expect("global registry");
    let spec = reg
        .get(melsec_mc::commands::Command::ReadBits)
        .expect("ReadBits spec");
    let params = create_read_bits_params(device, count);
    let request_data = spec
        .build_request(&params, Some(melsec_mc::plc_series::PLCSeries::R))
        .expect("build request");
    let mc_payload = McRequest::new()
        .try_with_request_data(request_data.clone())
        .expect("build request")
        .build();

    let send_hex = mc_payload
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(" ");
    eprintln!("SENT: {send_hex}");

    let timeout = Some(Duration::from_secs(5));
    let tcp_addr = format!("{addr}:{tcp_port}");
    let buf = match melsec_mc::transport::send_and_recv_tcp(&tcp_addr, &mc_payload, timeout).await {
        Ok(b) => b,
        Err(e) => {
            eprintln!("TCP ERROR: {e}");
            return;
        }
    };
    let recv_hex = buf
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(" ");
    eprintln!("RECV: {recv_hex}");

    let resp = melsec_mc::response::McResponse::try_new(&buf).expect("parse response");
    eprintln!(
        "parsed end_code: {end:?}, data_len: {len}",
        end = resp.end_code,
        len = resp.request_data_len
    );

    let data = resp.data;
    eprintln!(
        "data bytes: {}",
        data.iter()
            .map(|b| format!("{b:02X}"))
            .collect::<Vec<_>>()
            .join(" ")
    );

    // decode bits LSB-first
    let mut on_indices = Vec::new();
    for i in 0..(count as usize) {
        let byte_idx = i / 8;
        let bit_idx = i % 8;
        let bit = if byte_idx < data.len() {
            ((data[byte_idx] >> bit_idx) & 0x01) != 0
        } else {
            false
        };
        if bit {
            on_indices.push(i);
        }
    }
    eprintln!("ON indices among M{}..M{}: {on_indices:?}", 0, count - 1);
}
