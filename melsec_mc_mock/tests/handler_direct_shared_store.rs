use melsec_mc_mock::MockServer;
use tracing::debug;

#[tokio::test]
async fn handler_write_then_read_same_store() {
    // ensure registry
    let _ = melsec_mc::command_registry::CommandRegistry::load_and_set_global_from_src();

    let server = MockServer::new();

    // prepare a write_words McRequest for D200 addr 0, two words
    use melsec_mc::command_registry::create_write_words_params;
    use melsec_mc::plc_series::PLCSeries;

    let device = "D200";
    let words = vec![0x1111u16, 0x2222u16];
    let params = create_write_words_params(device, &words);
    let spec = melsec_mc::command_registry::GLOBAL_COMMAND_REGISTRY
        .get()
        .unwrap()
        .get(melsec_mc::commands::Command::WriteWords)
        .unwrap();
    let rd = spec
        .build_request(&params, Some(PLCSeries::R))
        .expect("build");
    let write_req = melsec_mc::request::McRequest::new()
        .try_with_request_data(rd.clone())
        .expect("mk req");
    debug!("write request_data len={} bytes: {:02X?}", rd.len(), rd);

    // apply via handler
    let _ = melsec_mc_mock::handler::handle_request_and_apply_store(&server.store, &write_req)
        .await
        .expect("write handler");

    // now build read request and call handler
    use melsec_mc::command_registry::create_read_words_params;
    let params_r = create_read_words_params("D200", 2u16);
    let spec_r = melsec_mc::command_registry::GLOBAL_COMMAND_REGISTRY
        .get()
        .unwrap()
        .get(melsec_mc::commands::Command::ReadWords)
        .unwrap();
    let rd_r = spec_r
        .build_request(&params_r, Some(PLCSeries::R))
        .expect("build read");
    let read_req = melsec_mc::request::McRequest::new()
        .try_with_request_data(rd_r)
        .expect("mk req r");
    let resp_data =
        melsec_mc_mock::handler::handle_request_and_apply_store(&server.store, &read_req)
            .await
            .expect("read handler");

    assert!(resp_data.len() >= 4);
    let v0 = u16::from_le_bytes([resp_data[0], resp_data[1]]);
    let v1 = u16::from_le_bytes([resp_data[2], resp_data[3]]);
    assert_eq!(v0, words[0]);
    assert_eq!(v1, words[1]);
}
