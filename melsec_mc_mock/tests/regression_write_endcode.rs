use melsec_mc::request::McRequest;
use melsec_mc_mock::handler;
use melsec_mc_mock::MockServer;

#[tokio::test]
async fn direct_handler_write_bits_and_words_return_empty_payload() {
    // Prepare a write_bits request (command 0x1401, sub 0x0001)
    // start=0, device_code=0xA8, count=2 bits, payload one byte nibble-packed (hi=1, lo=1)
    let mut bits_req: Vec<u8> = Vec::new();
    bits_req.extend_from_slice(&0x1401u16.to_le_bytes());
    bits_req.extend_from_slice(&0x0001u16.to_le_bytes());
    bits_req.extend_from_slice(&[0x00, 0x00, 0x00]); // start addr 3le
    bits_req.push(0xA8u8); // device code
    bits_req.extend_from_slice(&2u16.to_le_bytes()); // count = 2 bits
    bits_req.push(0x11u8); // payload: high nibble=1, low nibble=1

    let req_bits = McRequest::new()
        .try_with_request_data(&bits_req)
        .expect("build bits req");
    let parsed_bits = McRequest::try_from_payload(&req_bits.build()).expect("parse bits req");

    let server = MockServer::new();

    // Call handler directly and expect an empty logical payload (transport adds end-code)
    let resp_bits = handler::handle_request_and_apply_store(&server.store, &parsed_bits)
        .await
        .expect("handler ok");
    assert!(
        resp_bits.is_empty(),
        "expected empty logical payload for write_bits, got: {:?}",
        resp_bits
    );

    // Verify the store received two written words (bits mapped to words 0/1)
    let got = server.get_words("0xA8", 0, 2).await;
    assert_eq!(got.len(), 2);
    assert_eq!(got, vec![1u16, 1u16]);

    // Prepare a write_words request (command 0x1401, sub 0x0000)
    let mut wreq: Vec<u8> = Vec::new();
    wreq.extend_from_slice(&0x1401u16.to_le_bytes());
    wreq.extend_from_slice(&0x0000u16.to_le_bytes());
    wreq.extend_from_slice(&[0x00, 0x00, 0x00]); // start addr 3le
    wreq.push(0xA8u8); // device code
    wreq.extend_from_slice(&1u16.to_le_bytes()); // count = 1 word
    wreq.extend_from_slice(&0x5566u16.to_le_bytes()); // data

    let req_words = McRequest::new()
        .try_with_request_data(&wreq)
        .expect("build wreq");
    let parsed_words = McRequest::try_from_payload(&req_words.build()).expect("parse wreq");

    let resp_words = handler::handle_request_and_apply_store(&server.store, &parsed_words)
        .await
        .expect("handler ok");
    assert!(
        resp_words.is_empty(),
        "expected empty logical payload for write_words, got: {:?}",
        resp_words
    );

    let got2 = server.get_words("0xA8", 0, 1).await;
    assert_eq!(got2, vec![0x5566u16]);
}
