use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, UdpSocket};

use melsec_mc::error::MelsecError;
use melsec_mc::transport::{send_and_recv_tcp, send_and_recv_udp};

#[tokio::test]
#[ignore = "integration test that talks to a real device"]
async fn roundtrip_tcp_local_echo() -> Result<(), MelsecError> {
    // start a Tokio TCP listener on ephemeral port
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let local_addr = listener.local_addr().unwrap();

    // spawn server task: accept one connection, read some bytes, then write a fixed reply
    let server = tokio::spawn(async move {
        if let Ok((mut sock, _peer)) = listener.accept().await {
            let mut buf = [0u8; 1024];
            // read with a timeout
            if let Ok(Ok(_n)) =
                tokio::time::timeout(Duration::from_secs(2), sock.read(&mut buf)).await
            {
                // craft a simple MC3E-like response payload: access_route(5) + data_len(2) + end_code(2) + data
                let mut resp: Vec<u8> = vec![0, 0, 0, 0, 0];
                resp.extend_from_slice(&4u16.to_le_bytes());
                resp.extend_from_slice(&0u16.to_le_bytes());
                resp.extend_from_slice(&[0xAA, 0xBB]);
                let _ = sock.write_all(&resp).await;
            }
        }
    });

    // client: call send_and_recv_tcp
    // small delay to give server task a moment to start accepting (avoid rare races)
    tokio::time::sleep(Duration::from_millis(20)).await;
    let payload = vec![0x01u8, 0x02, 0x03];
    let addr = format!("127.0.0.1:{}", local_addr.port());
    let resp = send_and_recv_tcp(&addr, &payload, Some(Duration::from_secs(3))).await?;
    // expect to contain the data bytes 0xAA 0xBB at the end
    assert!(resp.ends_with(&[0xAA, 0xBB]));
    let _ = server.await;
    Ok(())
}

#[tokio::test]
#[ignore = "integration test that talks to a real device"]
async fn roundtrip_udp_local_echo() -> Result<(), MelsecError> {
    // start a Tokio UDP listener on ephemeral port
    let sock = UdpSocket::bind("127.0.0.1:0").await.expect("bind udp");
    let local_addr = sock.local_addr().unwrap();

    // spawn server task (move the bound socket into the task)
    let server = {
        let srv_sock = sock;
        tokio::spawn(async move {
            let mut buf = [0u8; 2048];
            if let Ok(Ok((_n, src))) =
                tokio::time::timeout(Duration::from_secs(3), srv_sock.recv_from(&mut buf)).await
            {
                let _ = srv_sock.send_to(&[0, 0, 0, 0, 0, 0x11, 0x22], &src).await;
            }
        })
    };

    let payload = vec![0x10u8, 0x20, 0x30];
    let addr = format!("127.0.0.1:{}", local_addr.port());
    let resp = send_and_recv_udp(&addr, &payload, Some(Duration::from_secs(3))).await?;
    assert!(resp.ends_with(&[0x11, 0x22]));
    let _ = server.await;
    Ok(())
}
