use melsec_mc_mock::MockServer;
use std::net::TcpListener;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::test]
async fn mc4e_invalid_data_len_returns_mc4e_error_response() {
    // start server
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral");
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let server = MockServer::new();
    let srv = server.clone();
    tokio::spawn(async move {
        let _ = srv.run_listener(&format!("127.0.0.1:{}", port)).await;
    });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Build an MC4E header with invalid data_len = 1 (must be >=2) to trigger detect_frame Err
    use melsec_mc::mc_define::{MC_SUBHEADER_REQUEST, MC_SUBHEADER_RESPONSE};
    let mut buf: Vec<u8> = Vec::new();
    buf.push(MC_SUBHEADER_REQUEST[0]);
    buf.push(MC_SUBHEADER_REQUEST[1]);
    // serial = 0x1234
    buf.extend_from_slice(&0x1234u16.to_le_bytes());
    // reserved
    buf.extend_from_slice(&0u16.to_le_bytes());
    // access route (5)
    buf.extend_from_slice(&[0x00u8, 0xFFu8, 0xFFu8, 0x03u8, 0x00u8]);
    // data_len = 1 (invalid)
    buf.extend_from_slice(&1u16.to_le_bytes());
    // pad to 15 bytes so detect_frame treats this as MC4E header and errors
    buf.extend_from_slice(&[0u8, 0u8]);

    let mut s = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("connect");
    s.write_all(&buf).await.expect("send malformed");

    // read response or observe connection reset (RST)
    let mut resp = vec![0u8; 64];
    let read_res = tokio::time::timeout(std::time::Duration::from_secs(1), s.read(&mut resp))
        .await
        .expect("read timeout");

    match read_res {
        Ok(n) => {
            resp.truncate(n);
            // response should start with MC_SUBHEADER_RESPONSE
            assert!(resp.len() >= 15, "response too short: {}", resp.len());
            assert_eq!(resp[0], MC_SUBHEADER_RESPONSE[0]);
            assert_eq!(resp[1], MC_SUBHEADER_RESPONSE[1]);
            // end_code at offset 13..15
            let end_code = u16::from_le_bytes([resp[13], resp[14]]);
            assert_eq!(end_code, 0x0050u16);
        }
        Err(e) => {
            // connection reset from server (expected when RST is forced)
            assert_eq!(e.kind(), std::io::ErrorKind::ConnectionReset);
        }
    }
}

#[tokio::test]
async fn mc3e_echo_request_returns_mc3e_response() {
    // start server
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral");
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let server = MockServer::new();
    let srv = server.clone();
    tokio::spawn(async move {
        let _ = srv.run_listener(&format!("127.0.0.1:{}", port)).await;
    });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Build a simple Echo request (command 0x0619, sub 0x0000) with ASCII hex payload "AB"
    let mut req_data: Vec<u8> = Vec::new();
    // command 0x0619 little-endian
    req_data.extend_from_slice(&0x0619u16.to_le_bytes());
    // subcommand 0x0000
    req_data.extend_from_slice(&0x0000u16.to_le_bytes());
    // ascii-hex payload 'A' 'B'
    req_data.extend_from_slice(b"AB");

    // Build MC3E-style frame (subheader + access_route + data_len + monitor_timer + request_data)
    let mut payload: Vec<u8> = Vec::new();
    // subheader 0x50 0x00 (MC3E-like)
    payload.extend_from_slice(&[0x50u8, 0x00u8]);
    // access route default
    payload.extend_from_slice(&melsec_mc::mc_define::AccessRoute::default().to_bytes());
    // data_len = request_data.len() + 2
    let data_len = u16::try_from(req_data.len() + 2).unwrap();
    payload.extend_from_slice(&data_len.to_le_bytes());
    // monitor_timer (2 bytes) - set 0
    payload.extend_from_slice(&0u16.to_le_bytes());
    // request data
    payload.extend_from_slice(&req_data);

    let mut s = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("connect");
    s.write_all(&payload).await.expect("send request");

    // read response
    let mut resp = vec![0u8; 64];
    let n = tokio::time::timeout(std::time::Duration::from_secs(1), s.read(&mut resp))
        .await
        .expect("read timeout")
        .expect("read error");
    resp.truncate(n);

    // should be MC3E-style response starting with 0xD0 0x00
    assert!(resp.len() >= 11, "response too short: {}", resp.len());
    assert_eq!(resp[0], 0xD0u8);
    assert_eq!(resp[1], 0x00u8);
    // trailing payload should contain the echoed ascii 'A' 'B'
    assert!(
        resp.ends_with(b"AB"),
        "response payload missing echoed bytes"
    );
}
