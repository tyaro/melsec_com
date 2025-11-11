use std::time::Duration;

use tokio::time::timeout;

#[tokio::test]
async fn tcp_write_udp_read_shared_store() {
    // ensure registry loaded (same as server does)
    let _ = melsec_mc::command_registry::CommandRegistry::load_and_set_global_from_src();

    let server = melsec_mc_mock::MockServer::new();

    // start TCP listener on ephemeral test ports
    let tcp_addr = "127.0.0.1:6000";
    let udp_addr = "127.0.0.1:6001";

    // spawn tcp listener
    let srv_clone = server.clone();
    let listener = tokio::net::TcpListener::bind(tcp_addr)
        .await
        .expect("bind tcp");
    let tcp_h = tokio::spawn(async move {
        let _ = srv_clone.run_listener_on(listener).await;
    });

    // spawn udp listener
    let srv_clone2 = server.clone();
    let udp_h = tokio::spawn(async move {
        let _ = srv_clone2.run_udp_listener(udp_addr).await;
    });

    // build a write_words request using registry helpers
    use melsec_mc::command_registry::create_write_words_params;
    use melsec_mc::command_registry::GLOBAL_COMMAND_REGISTRY;
    use melsec_mc::plc_series::PLCSeries;

    // choose device key and address
    let key = "D";
    let addr = 100usize;
    let words = [0x1234u16, 0x5678u16];

    // ensure global registry
    if GLOBAL_COMMAND_REGISTRY.get().is_none() {
        let _ = melsec_mc::command_registry::CommandRegistry::load_and_set_global_from_src();
    }

    let params = create_write_words_params(&format!("{}{}", key, addr), words.as_ref());
    let spec = GLOBAL_COMMAND_REGISTRY
        .get()
        .unwrap()
        .get(melsec_mc::commands::Command::WriteWords)
        .unwrap();
    let request_data = spec
        .build_request(&params, Some(PLCSeries::R))
        .expect("build request");
    let req = melsec_mc::request::McRequest::new()
        .try_with_request_data(request_data)
        .expect("make McRequest");
    let payload = req.build();

    // send via TCP to server
    let mut sock = tokio::net::TcpStream::connect(tcp_addr)
        .await
        .expect("connect tcp");
    // write entire payload
    use tokio::io::AsyncWriteExt;
    sock.write_all(&payload).await.expect("write_all payload");

    // try to read a response (best-effort) so server has opportunity to process
    use tokio::io::AsyncReadExt;
    let mut buf = vec![0u8; 4096];
    if let Ok(Ok(n)) = timeout(Duration::from_millis(800), sock.read(&mut buf)).await {
        let _ = n; // ignore contents
    }

    // Allow small time for the TCP handler to apply the write into the store
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Directly read from the server store to ensure the TCP write was applied
    let key_literal = match melsec_mc::device::device_by_symbol(key) {
        Some(dev) => format!("0x{:02X}", dev.device_code_q()),
        None => format!("0x{:02X}", 0u8),
    };
    let got = server.get_words(&key_literal, addr, 2).await;
    assert_eq!(got.len(), 2);
    assert_eq!(got[0], words[0]);
    assert_eq!(got[1], words[1]);

    // Also exercise the handler directly to emulate a UDP read using the same store
    use melsec_mc::command_registry::create_read_words_params;
    use melsec_mc_mock::handler::handle_request_and_apply_store;
    let device = format!("{}{}", key, addr);
    let params_r = create_read_words_params(&device, 2u16);
    let spec_r = GLOBAL_COMMAND_REGISTRY
        .get()
        .unwrap()
        .get(melsec_mc::commands::Command::ReadWords)
        .unwrap();
    let request_data_r = spec_r
        .build_request(&params_r, Some(PLCSeries::R))
        .expect("build read");
    let req_r = melsec_mc::request::McRequest::new()
        .try_with_request_data(request_data_r)
        .expect("mk req r");
    let resp_data = handle_request_and_apply_store(&server.store, &req_r)
        .await
        .expect("handler read");
    // resp_data should contain two little-endian u16 values
    assert!(resp_data.len() >= 4);
    let v0 = u16::from_le_bytes([resp_data[0], resp_data[1]]);
    let v1 = u16::from_le_bytes([resp_data[2], resp_data[3]]);
    assert_eq!(v0, words[0]);
    assert_eq!(v1, words[1]);

    // cleanup
    tcp_h.abort();
    udp_h.abort();
}
