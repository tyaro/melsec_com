use melsec_mc::request::McRequest;
use melsec_mc_mock::handler;
use melsec_mc_mock::MockServer;

#[tokio::test]
async fn direct_handler_write_check() {
    // Build a write_words request payload similar to the integration test
    let mut wreq: Vec<u8> = Vec::new();
    wreq.extend_from_slice(&0x1401u16.to_le_bytes());
    wreq.extend_from_slice(&0x0000u16.to_le_bytes());
    wreq.extend_from_slice(&[0x00, 0x00, 0x00]); // start addr 3le
    wreq.push(0xA8u8); // device code
    wreq.extend_from_slice(&1u16.to_le_bytes()); // count
    wreq.extend_from_slice(&0x5566u16.to_le_bytes()); // data

    let req_obj = McRequest::new()
        .try_with_request_data(&wreq)
        .expect("build wreq");
    // capture request_data length before consuming request object
    let before_len = req_obj.request_data.len();
    // build the raw MC payload and parse it back to simulate network path
    let raw = req_obj.build();
    let parsed_req = McRequest::try_from_payload(&raw).expect("parse from payload");
    tracing::debug!(
        raw_len = raw.len(),
        before_len = before_len,
        parsed_len = parsed_req.request_data.len(),
        "raw/request sizes"
    );
    tracing::debug!(parsed_request_data = ?parsed_req.request_data, "parsed.request_data");

    let server = MockServer::new();
    // call handler directly with parsed request
    let res = handler::handle_request_and_apply_store(&server.store, &parsed_req)
        .await
        .expect("handler ok");
    tracing::debug!(response_bytes = ?res, "handler returned response bytes");

    // inspect store
    let words = server.get_words("0xA8", 0, 1).await;
    tracing::debug!(store_words_after = ?words, "store words after handler");
    assert_eq!(words, vec![0x5566u16]);
}
