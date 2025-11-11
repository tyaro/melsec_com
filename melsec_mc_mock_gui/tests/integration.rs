use std::net::TcpListener;

#[tokio::test]
async fn write_then_read_roundtrip() {
    // pick an available port
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind to ephemeral");
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    // start mock server
    let server = melsec_mc_mock::MockServer::new();
    let srv = server.clone();
    tokio::spawn(async move {
        let _ = srv.run_listener(&format!("127.0.0.1:{}", port)).await;
    });

    // give server a moment to start
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // create client
    let mut client =
        melsec_mc::mc_client::McClient::new().with_plc_series(melsec_mc::plc_series::PLCSeries::Q);
    let target = melsec_mc::endpoint::ConnectionTarget::direct("127.0.0.1", port);
    client = client.with_target(target);

    // write one word to D0
    let write_res = client.write_words("D0", &[0x1234u16]).await;
    assert!(write_res.is_ok(), "write_words failed: {:?}", write_res);

    // read it back as u16
    let read_res = client.read_words_as::<u16>("D0", 1).await;
    assert!(read_res.is_ok(), "read_words_as failed: {:?}", read_res);
    let vals = read_res.unwrap();
    assert_eq!(vals.len(), 1);
    assert_eq!(vals[0], 0x1234u16);
}
