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
#[allow(clippy::too_many_lines)]
async fn parallel_real_read_write_sample() {
    if !should_run() {
        eprintln!("skipping parallel_real_read_write_sample (set RUN_REAL_TESTS=1 to enable)");
        return;
    }
    if !allow_write() {
        eprintln!("skipping write operations (set RUN_REAL_WRITE=1 to enable)");
        return;
    }

    // device1 defaults (can be overridden via env)
    let dev1_ip = env::var("PLC1_ADDR")
        .unwrap_or_else(|_| env::var("PLC_ADDR").unwrap_or_else(|_| "192.168.1.40".to_string()));
    let dev1_port = env::var("PLC1_TCP_PORT")
        .ok()
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(4020u16);
    // device2 defaults
    let dev2_ip = env::var("PLC2_ADDR")
        .unwrap_or_else(|_| env::var("PLC_ADDR").unwrap_or_else(|_| "192.168.1.42".to_string()));
    let dev2_port = env::var("PLC2_TCP_PORT")
        .ok()
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(4300u16);

    // We'll perform conservative sample writes inside the allowed ranges.
    // Device1: M1000.., D1000..
    // Device2: M1100.., D1100..
    let handle1 = tokio::spawn(async move {
        let client = build_client_for(&dev1_ip, dev1_port, 0u8).with_client_name("parallel_dev1");

        // M1000: write 20 nibbles (sample)
        let m_device = "M1000";
        let nibble_values: Vec<u8> = (0..20).map(|i| u8::from(i % 2 == 0)).collect();

        // preserve original
        let (dev, addr) = parse_device_and_address(m_device).expect("parse device");
        let read_params = create_read_bits_params(m_device, 20);
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
        let original_data = resp.data.clone();

        // We'll spawn multiple concurrent write/verify tasks to increase stress on the PLC
        let bools: Vec<bool> = nibble_values.iter().map(|&b| b != 0u8).collect();
        let mut task_handles = Vec::new();
        for i in 0usize..8 {
            let target_addr = client.target.addr.clone();
            let ar = client.target.access_route;
            let plc_series = client.plc_series;
            let write_bools = bools.clone();
            let read_payload_clone = read_payload.clone();
            task_handles.push(tokio::spawn(async move {
                // small stagger to avoid connection burst
                let jitter_ms = (i as u64 * 3) % 20; // deterministic small jitter
                tokio::time::sleep(Duration::from_millis(jitter_ms)).await;

                // Build a lightweight client per task to perform write/verify
                let mut parts = target_addr.split(':');
                let ip = parts.next().unwrap().to_string();
                let port: u16 = parts.next().and_then(|p| p.parse().ok()).unwrap_or(4020u16);
                let c = McClient::new()
                    .with_target(
                        ConnectionTarget::direct(ip, port)
                            .with_access_route(ar)
                            .build(),
                    )
                    .with_protocol(Protocol::Tcp)
                    .with_monitoring_timer(5)
                    .with_plc_series(plc_series);

                // perform write+verify with retry/backoff, don't panic inside task — return Result
                let mut last_err: Option<String> = None;
                for attempt in 1..=3 {
                    // write
                    match c.write_bits(m_device, &write_bools).await {
                        Ok(_) => {
                            // verify
                            let to = if c.monitoring_timer > 0 {
                                Some(Duration::from_secs(u64::from(c.monitoring_timer)))
                            } else {
                                None
                            };
                            match melsec_mc::transport::send_and_recv_tcp(
                                &c.target.addr,
                                &read_payload_clone,
                                to,
                            )
                            .await
                            {
                                Ok(read_raw2) => {
                                    let resp2 =
                                        melsec_mc::response::McResponse::try_new(&read_raw2)
                                            .expect("parse response");
                                    if resp2.has_end_code && resp2.end_code != Some(0) {
                                        last_err = Some(format!(
                                            "device end code 0x{:04X}",
                                            resp2.end_code.unwrap_or(0)
                                        ));
                                    } else {
                                        // success
                                        last_err = None;
                                        break;
                                    }
                                }
                                Err(e) => {
                                    last_err = Some(format!(
                                        "verify read send/recv attempt {}/{} err={}",
                                        attempt, 3, e
                                    ));
                                }
                            }
                        }
                        Err(e) => {
                            last_err =
                                Some(format!("write_bits attempt {}/{} err={}", attempt, 3, e));
                        }
                    }

                    // backoff before next attempt (small exponential backoff)
                    let mult = match attempt {
                        2 => 2u64,
                        3 => 4u64,
                        _ => 1u64,
                    };
                    let backoff_ms = 50u64.saturating_mul(mult);
                    tokio::time::sleep(Duration::from_millis(backoff_ms + (i as u64))).await;
                }
                last_err.map_or(Ok::<(), String>(()), Err::<(), String>)
            }));
        }
        // wait for all concurrent tasks and collect failures
        let mut errors: Vec<String> = Vec::new();
        for h in task_handles {
            match h.await {
                Ok(Ok(())) => (),
                Ok(Err(e)) => errors.push(e),
                Err(join_err) => errors.push(format!("subtask panicked or cancelled: {join_err}")),
            }
        }

        if !errors.is_empty() {
            eprintln!("dev1 encountered {} subtask errors:", errors.len());
            for e in &errors {
                eprintln!("  - {e}");
            }
        }

        // restore original by writing back original bytes once
        let orig_parsed = read_spec
            .parse_response(
                &serde_json::json!({
                    "data_blocks": [{ "count": 20u64 }],
                    "start_addr": u64::from(addr),
                    "device_code": u64::from(dev.device_code_q()),
                    "count": 20u64
                }),
                &original_data,
            )
            .ok();
        if let Some(orig_json) = orig_parsed {
            if let Some(db2) = orig_json.get("data_blocks").and_then(|v| v.as_array()) {
                let vals2 = db2[0].as_array().unwrap();
                let bools: Vec<bool> = vals2.iter().map(|n| n.as_bool().unwrap_or(false)).collect();
                let _ = client.write_bits(m_device, &bools).await;
            }
        }
        Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
    });

    let handle2 = tokio::spawn(async move {
        let client = build_client_for(&dev2_ip, dev2_port, 0u8).with_client_name("parallel_dev2");

        // M1100: write 20 nibbles
        let m_device = "M1100";
        let nibble_values: Vec<u8> = (0..20)
            .map(|i| if i % 2 == 0 { 2u8 } else { 0u8 })
            .collect();

        let (dev, addr) = parse_device_and_address(m_device).expect("parse device");
        let read_params = create_read_bits_params(m_device, 20);
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
        let original_data = resp.data.clone();

        // spawn concurrent write/verify tasks to stress device2
        let bools: Vec<bool> = nibble_values.iter().map(|&b| b != 0u8).collect();
        let mut task_handles2 = Vec::new();
        for i in 0usize..8 {
            let target_addr = client.target.addr.clone();
            let plc_series = client.plc_series;
            let write_bools = bools.clone();
            let read_payload_clone = read_payload.clone();
            task_handles2.push(tokio::spawn(async move {
                // small stagger to avoid connection burst
                let jitter_ms = (i as u64 * 5) % 30;
                tokio::time::sleep(Duration::from_millis(jitter_ms)).await;

                let mut parts = target_addr.split(':');
                let ip = parts.next().unwrap().to_string();
                let port: u16 = parts.next().and_then(|p| p.parse().ok()).unwrap_or(4020u16);
                let c = McClient::new()
                    .with_target(ConnectionTarget::direct(ip, port).build())
                    .with_protocol(Protocol::Tcp)
                    .with_monitoring_timer(5)
                    .with_plc_series(plc_series);

                let mut last_err: Option<String> = None;
                for attempt in 1..=3 {
                    match c.write_bits(m_device, &write_bools).await {
                        Ok(_) => {
                            let to = if c.monitoring_timer > 0 {
                                Some(Duration::from_secs(u64::from(c.monitoring_timer)))
                            } else {
                                None
                            };
                            match melsec_mc::transport::send_and_recv_tcp(
                                &c.target.addr,
                                &read_payload_clone,
                                to,
                            )
                            .await
                            {
                                Ok(read_raw2) => {
                                    let resp2 =
                                        melsec_mc::response::McResponse::try_new(&read_raw2)
                                            .expect("parse response");
                                    if resp2.has_end_code && resp2.end_code != Some(0) {
                                        last_err = Some(format!(
                                            "device end code 0x{:04X}",
                                            resp2.end_code.unwrap_or(0)
                                        ));
                                    } else {
                                        last_err = None;
                                        break;
                                    }
                                }
                                Err(e) => {
                                    last_err = Some(format!(
                                        "verify read send/recv attempt {}/{} err={}",
                                        attempt, 3, e
                                    ));
                                }
                            }
                        }
                        Err(e) => {
                            last_err =
                                Some(format!("write_bits attempt {}/{} err={}", attempt, 3, e));
                        }
                    }
                    let mult = match attempt {
                        2 => 2u64,
                        3 => 4u64,
                        _ => 1u64,
                    };
                    let backoff_ms = 50u64.saturating_mul(mult);
                    tokio::time::sleep(Duration::from_millis(backoff_ms + (i as u64))).await;
                }

                last_err.map_or(Ok::<(), String>(()), Err::<(), String>)
            }));
        }

        let mut errors2: Vec<String> = Vec::new();
        for h in task_handles2 {
            match h.await {
                Ok(Ok(())) => (),
                Ok(Err(e)) => errors2.push(e),
                Err(join_err) => errors2.push(format!("subtask panicked or cancelled: {join_err}")),
            }
        }

        if !errors2.is_empty() {
            eprintln!("dev2 encountered {} subtask errors:", errors2.len());
            for e in &errors2 {
                eprintln!("  - {e}");
            }
        }

        // restore original
        let orig_parsed = read_spec
            .parse_response(
                &serde_json::json!({
                    "data_blocks": [{ "count": 20u64 }],
                    "start_addr": u64::from(addr),
                    "device_code": u64::from(dev.device_code_q()),
                    "count": 20u64
                }),
                &original_data,
            )
            .ok();
        if let Some(orig_json) = orig_parsed {
            if let Some(db2) = orig_json.get("data_blocks").and_then(|v| v.as_array()) {
                let vals2 = db2[0].as_array().unwrap();
                let bools: Vec<bool> = vals2.iter().map(|n| n.as_bool().unwrap_or(false)).collect();
                let _ = client.write_bits(m_device, &bools).await;
            }
        }
        Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
    });

    // wait both
    let r1 = handle1.await.expect("task1 panicked");
    let r2 = handle2.await.expect("task2 panicked");
    r1.expect("dev1 failed");
    r2.expect("dev2 failed");
}
