use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::timeout;

use melsec_mc::mc_frame::detect_frame;
use melsec_mc::request::McRequest;
use melsec_mc::response::parse_mc_payload;
use melsec_mc_mock::MockServer;
use tracing::debug;

#[tokio::test]
async fn tcp_echo_and_read_write() {
    // start mock server on an ephemeral TCP port
    let server = MockServer::new();
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("local_addr");
    let server_clone = server.clone();
    tokio::spawn(async move {
        let _ = server_clone.run_listener_on(listener).await;
    });

    // --- Echo test ---
    let mut stream = TcpStream::connect(addr).await.expect("connect");
    let mut payload: Vec<u8> = Vec::new();
    // command 0x0619, sub 0x0000
    payload.extend_from_slice(&0x0619u16.to_le_bytes());
    payload.extend_from_slice(&0x0000u16.to_le_bytes());
    payload.extend_from_slice(b"1234");
    let req = McRequest::new()
        .try_with_request_data(&payload)
        .expect("build req");
    let req_bytes = req.build();
    stream.write_all(&req_bytes).await.expect("write");

    // read response frame
    let mut acc: Vec<u8> = Vec::new();
    let mut buf = [0u8; 1024];
    let got = timeout(Duration::from_secs(2), async {
        loop {
            let n = stream.read(&mut buf).await.expect("read");
            if n == 0 {
                break None;
            }
            acc.extend_from_slice(&buf[..n]);
            if let Ok(Some((frame_len, _h, _s))) = detect_frame(&acc) {
                if acc.len() >= frame_len {
                    let frame = acc.drain(..frame_len).collect::<Vec<u8>>();
                    let resp = parse_mc_payload(&frame).expect("parse resp");
                    return Some(resp);
                }
            }
        }
    })
    .await
    .expect("timeout");
    let resp = got.expect("got resp");
    assert_eq!(resp.data, b"1234");

    // --- Write words test (write one word) ---
    // Build write request: command 0x1401, sub 0x0000, start=0x000000, device_code=0xA8, count=1, data=0x5566
    let mut wreq: Vec<u8> = Vec::new();
    wreq.extend_from_slice(&0x1401u16.to_le_bytes());
    wreq.extend_from_slice(&0x0000u16.to_le_bytes());
    wreq.extend_from_slice(&[0x00, 0x00, 0x00]); // start addr 3le
    wreq.push(0xA8u8); // device code
    wreq.extend_from_slice(&1u16.to_le_bytes()); // count
    wreq.extend_from_slice(&0x5566u16.to_le_bytes()); // data
    let wreq_obj = McRequest::new()
        .try_with_request_data(&wreq)
        .expect("build wreq");
    let wbytes = wreq_obj.build();
    stream.write_all(&wbytes).await.expect("write wreq");

    // read write response (expect end-code only)
    let mut acc2: Vec<u8> = Vec::new();
    let mut buf2 = [0u8; 1024];
    let got2 = timeout(Duration::from_secs(2), async {
        loop {
            let n = stream.read(&mut buf2).await.expect("read");
            if n == 0 {
                break None;
            }
            acc2.extend_from_slice(&buf2[..n]);
            if let Ok(Some((frame_len, _h, _s))) = detect_frame(&acc2) {
                if acc2.len() >= frame_len {
                    let frame = acc2.drain(..frame_len).collect::<Vec<u8>>();
                    let resp = parse_mc_payload(&frame).expect("parse resp");
                    return Some(resp);
                }
            }
        }
    })
    .await
    .expect("timeout");
    let resp2 = got2.expect("got resp2");
    // Now REQUIRE end-code-only responses per commands.toml (response_format = []).
    assert!(
        resp2.data.is_empty(),
        "expected empty logical payload for write response, got: {:?}",
        resp2.data
    );
    assert_eq!(resp2.end_code, Some(0x0000));

    // verify store has the written word via server API
    debug!("[TEST] about to save snapshot to ./tmp_debug.json");
    server
        .save_snapshot("./tmp_debug.json")
        .await
        .expect("save snapshot");
    debug!("[TEST] snapshot saved, about to call server.get_words");
    let got_words = server.get_words("0xA8", 0, 1).await;
    debug!("[TEST] got_words returned: {:?}", got_words);
    assert_eq!(got_words, vec![0x5566u16]);
}
