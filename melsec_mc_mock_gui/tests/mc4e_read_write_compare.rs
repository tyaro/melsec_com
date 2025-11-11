use std::error::Error;

use melsec_mc_mock::server::MockServer;

fn hex_dump(b: &[u8]) -> String {
    b.iter()
        .map(|x| format!("{:02X}", x))
        .collect::<Vec<_>>()
        .join(" ")
}

#[tokio::test]
async fn mc4e_read_write_words_bits_compare() -> Result<(), Box<dyn Error>> {
    // Ensure command registry and error codes are loaded
    let _ = melsec_mc::init_defaults();

    let mock = MockServer::new();

    // Initialize mock device map explicitly if a default assignment exists
    // (MockServer::new already attempts defaults, but ensure store is ready)
    {
        let _s = mock.store.write().await;
    }

    // Resolve device codes for D and M
    let d_dev = melsec_mc::device::device_by_symbol("D").ok_or("device D not found")?;
    let m_dev = melsec_mc::device::device_by_symbol("M").ok_or("device M not found")?;
    let d_key = format!("0x{:02X}", d_dev.device_code_q());
    let m_key = format!("0x{:02X}", m_dev.device_code_q());

    // seed store: D1000.. with incremental words, M0.. with bits stored as 0/1 words
    let d_start = 1000usize;
    let mut d_vals_10: Vec<u16> = Vec::new();
    for i in 0..10u16 {
        d_vals_10.push(0x1000u16.wrapping_add(i));
    }
    mock.set_words(&d_key, d_start, &d_vals_10).await;

    let m_start = 0usize;
    let mut m_vals_10: Vec<u16> = Vec::new();
    for i in 0..10usize {
        m_vals_10.push(if i % 2 == 0 { 1u16 } else { 0u16 });
    }
    mock.set_words(&m_key, m_start, &m_vals_10).await;

    // Start mock TCP listener on ephemeral port and run it in background
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let local_addr = listener.local_addr()?;
    let bind_addr = format!("{}", local_addr);
    let _server_task = tokio::spawn(mock.clone().run_listener_on(listener));

    // scenarios: (name, is_bit, device_str, counts)
    // Added W0..W1F (32 words) and B0..B1F (32 bits) tests using hex addresses
    let scenarios = vec![
        // write scenarios first
        ("write_words_1", false, "D1000", 1usize),
        ("write_words_10", false, "D1000", 10usize),
        ("write_bits_1", true, "M0", 1usize),
        ("write_bits_10", true, "M0", 10usize),
        // large hex-address ranges (W 0x0..0x1F = 32 words, B 0x0..0x1F = 32 bits)
        ("write_words_w_0_1f", false, "W0", 0x20usize),
        ("write_bits_b_0_1f", true, "B0", 0x20usize),
        // then read scenarios
        ("read_words_1", false, "D1000", 1usize),
        ("read_words_10", false, "D1000", 10usize),
        ("read_bits_1", true, "M0", 1usize),
        ("read_bits_10", true, "M0", 10usize),
        ("read_words_w_0_1f", false, "W0", 0x20usize),
        ("read_bits_b_0_1f", true, "B0", 0x20usize),
    ];

    for (name, is_bit, device, count) in scenarios {
        println!("=== scenario: {} ===", name);
        // build params and request_data
        let params = if !is_bit {
            // read or write words
            if name.starts_with("read") {
                melsec_mc::command_registry::create_read_words_params(device, count as u16)
            } else {
                // write: prepare values
                let values: Vec<u16> = (0..count)
                    .map(|i| 0x2000u16.wrapping_add(i as u16))
                    .collect();
                melsec_mc::command_registry::create_write_words_params(device, &values)
            }
        } else if name.starts_with("read") {
            melsec_mc::command_registry::create_read_bits_params(device, count as u16)
        } else {
            // write bits: prepare boolean vector
            let bools: Vec<bool> = (0..count).map(|i| i % 2 == 0).collect();
            melsec_mc::command_registry::create_write_bits_params(device, &bools)
        };

        let reg = melsec_mc::command_registry::CommandRegistry::global()
            .ok_or("global registry not set")?;
        let spec = if !is_bit {
            if name.starts_with("read") {
                reg.get(melsec_mc::commands::Command::ReadWords)
                    .ok_or("spec ReadWords not found")?
            } else {
                reg.get(melsec_mc::commands::Command::WriteWords)
                    .ok_or("spec WriteWords not found")?
            }
        } else if name.starts_with("read") {
            reg.get(melsec_mc::commands::Command::ReadBits)
                .ok_or("spec ReadBits not found")?
        } else {
            reg.get(melsec_mc::commands::Command::WriteBits)
                .ok_or("spec WriteBits not found")?
        };

        let req_data = spec.build_request(&params, None)?;
        let mc_req = melsec_mc::request::McRequest::new()
            .with_access_route(melsec_mc::mc_define::AccessRoute::default())
            .try_with_request_data(req_data)?;
        let mc_payload = mc_req.build();

        // Send to mock via transport (MC4E over TCP)
        let timeout = Some(std::time::Duration::from_secs(5));
        let buf = melsec_mc::transport::send_and_recv_tcp(&bind_addr, &mc_payload, timeout).await?;
        let mock_resp = melsec_mc::response::McResponse::try_new(&buf)?;
        let mock_data = mock_resp.data.clone();

        // For write scenarios, verify mock store updated (parse device and check stored values)
        if name.starts_with("write") {
            // parse device and address to locate store key
            if let Ok((dev, addr)) = melsec_mc::device::parse_device_and_address(device) {
                let key = format!("0x{:02X}", dev.device_code_q());
                if !is_bit {
                    // expected words sequence used when building the request
                    let expected: Vec<u16> = (0..count)
                        .map(|i| 0x2000u16.wrapping_add(i as u16))
                        .collect();
                    let got = mock.get_words(&key, addr as usize, count).await;
                    assert_eq!(
                        got.len(),
                        expected.len(),
                        "mock store missing written words for {}",
                        device
                    );
                    assert_eq!(
                        got, expected,
                        "written word values do not match for {}",
                        device
                    );
                } else {
                    // expected alternating bits -> stored as u16 words (1/0)
                    let expected_bits: Vec<u16> = (0..count)
                        .map(|i| if i % 2 == 0 { 1u16 } else { 0u16 })
                        .collect();
                    let got = mock.get_words(&key, addr as usize, count).await;
                    assert_eq!(
                        got.len(),
                        expected_bits.len(),
                        "mock store missing written bits for {}",
                        device
                    );
                    assert_eq!(
                        got, expected_bits,
                        "written bit values do not match for {}",
                        device
                    );
                }
            } else {
                // If parsing failed (shouldn't for our test inputs), still fail explicitly
                panic!("failed to parse device string for verification: {}", device);
            }
        }

        // If REAL_PLC_ADDR set, send to real PLC via melsec_mc transport and compare only data payloads
        let mut mismatches: Vec<String> = Vec::new();
        if let Ok(addr) = std::env::var("REAL_PLC_ADDR") {
            let addr_with_port = if addr.contains(':') {
                addr
            } else {
                format!(
                    "{}:{}",
                    addr,
                    std::env::var("REAL_PLC_PORT").unwrap_or_else(|_| "4020".to_string())
                )
            };
            let real_buf =
                melsec_mc::transport::send_and_recv_tcp(&addr_with_port, &mc_payload, timeout)
                    .await?;
            let real_resp = melsec_mc::response::McResponse::try_new(&real_buf)?;
            let real_data = real_resp.data.clone();
            if real_data != mock_data {
                eprintln!(
                    "SCENARIO {}: MOCK DATA ({} bytes) mock\n{}\nREAL DATA ({} bytes) real\n{}\n",
                    name,
                    mock_data.len(),
                    hex_dump(&mock_data),
                    real_data.len(),
                    hex_dump(&real_data)
                );
                mismatches.push(name.to_string());
            } else {
                println!(
                    "SCENARIO {}: data payloads match ({} bytes)",
                    name,
                    mock_data.len()
                );
            }
        } else {
            println!(
                "SCENARIO {}: MOCK REQ {} bytes -> MOCK DATA {} bytes",
                name,
                mc_payload.len(),
                mock_data.len()
            );
            println!("  REQ: {}", hex_dump(&mc_payload));
            println!("  DATA: {}", hex_dump(&mock_data));
        }

        // If REAL_PLC_STRICT=1 is set, fail on any mismatch after all scenarios
        if let Ok(strict) = std::env::var("REAL_PLC_STRICT") {
            if strict == "1" && !mismatches.is_empty() {
                return Err(format!("mismatches detected: {:?}", mismatches).into());
            }
        }
    }

    Ok(())
}
