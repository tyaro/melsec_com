use std::error::Error;

use melsec_mc_mock::server::MockServer;

fn hex_dump(b: &[u8]) -> String {
    b.iter()
        .map(|x| format!("{:02X}", x))
        .collect::<Vec<_>>()
        .join(" ")
}

#[tokio::test]
async fn compare_mock_and_real_read_words_raw() -> Result<(), Box<dyn Error>> {
    // Initialize embedded defaults so command registry is available
    let _ = melsec_mc::init_defaults();

    // Prepare mock server and seed store with a known word at D0 (device code for D is 0xA8)
    let mock = MockServer::new();
    // set_words expects key like "0xA8"
    mock.set_words("0xA8", 0usize, &[0x1234u16]).await;

    // Build a read_words request for D0 count=1
    let params = melsec_mc::command_registry::create_read_words_params("D0", 1);
    let reg = melsec_mc::command_registry::CommandRegistry::global()
        .ok_or("global CommandRegistry not set")?;
    let spec = reg
        .get(melsec_mc::commands::Command::ReadWords)
        .ok_or("ReadWords spec not found")?;
    let request_data = spec.build_request(&params, None)?;

    // Construct McRequest
    let mc_req = melsec_mc::request::McRequest::new()
        .with_access_route(melsec_mc::mc_define::AccessRoute::default())
        .try_with_request_data(request_data)?;

    // Get mock response logical payload by invoking handler (use &mc_req)
    let resp_data =
        melsec_mc_mock::handler::handle_request_and_apply_store(&mock.store, &mc_req).await?;

    // capture serial and access_route before consuming mc_req with build()
    let req_serial = mc_req.serial_number;
    let req_ar_bytes = mc_req.access_route.to_bytes();
    // build payload after we've used mc_req by-reference
    let mc_payload = mc_req.build();

    // Build mock server full response frame (mirror of MockServer::build_mc_response_from_request)
    let mock_resp_frame = {
        let mut out: Vec<u8> = Vec::new();
        if req_serial != 0 {
            out.extend_from_slice(&melsec_mc::mc_define::MC_SUBHEADER_RESPONSE);
            out.extend_from_slice(&req_serial.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&req_ar_bytes);
            let data_len = u16::try_from(resp_data.len() + 2).unwrap_or(2);
            out.extend_from_slice(&data_len.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&resp_data);
        } else {
            out.extend_from_slice(&[0xD0u8, 0x00u8]);
            out.extend_from_slice(&req_ar_bytes);
            let data_len = u16::try_from(resp_data.len() + 2).unwrap_or(2);
            out.extend_from_slice(&data_len.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&resp_data);
        }
        out
    };

    // If REAL_PLC_ADDR is configured *and* REAL_PLC_STRICT=1, send the same mc_payload
    // to the real PLC and compare raw frames. This keeps CI/default runs safe by
    // requiring an explicit opt-in to contact real hardware.
    let addr_opt = std::env::var("REAL_PLC_ADDR").ok();
    let strict = std::env::var("REAL_PLC_STRICT").unwrap_or_default();
    if addr_opt.is_some() && strict == "1" {
        let addr = addr_opt.unwrap();
        // Optional port may be included in addr; if not, allow REAL_PLC_PORT
        let addr_with_port = if addr.contains(':') {
            addr
        } else {
            let port = std::env::var("REAL_PLC_PORT").unwrap_or_else(|_| "4020".to_string());
            format!("{}:{}", addr, port)
        };

        let timeout = Some(std::time::Duration::from_secs(5));
        let real_resp_frame =
            melsec_mc::transport::send_and_recv_tcp(&addr_with_port, &mc_payload, timeout).await?;

        if real_resp_frame == mock_resp_frame {
            println!("raw frames match: {} bytes", mock_resp_frame.len());
            return Ok(());
        } else {
            eprintln!(
                "=== MOCK RESPONSE FRAME ({}) ===\n{}\n",
                mock_resp_frame.len(),
                hex_dump(&mock_resp_frame)
            );
            eprintln!(
                "=== REAL  RESPONSE FRAME ({}) ===\n{}\n",
                real_resp_frame.len(),
                hex_dump(&real_resp_frame)
            );
            return Err(format!(
                "raw frames differ (mock {} bytes vs real {} bytes)",
                mock_resp_frame.len(),
                real_resp_frame.len()
            )
            .into());
        }
    } else {
        // No real PLC configured; print the mock request/response for manual inspection and skip
        println!("REAL_PLC_ADDR not set; printing mock request/response for inspection");
        println!(
            "REQUEST ({} bytes): {}",
            mc_payload.len(),
            hex_dump(&mc_payload)
        );
        println!(
            "MOCK RESPONSE ({} bytes): {}",
            mock_resp_frame.len(),
            hex_dump(&mock_resp_frame)
        );
        Ok(())
    }
}
