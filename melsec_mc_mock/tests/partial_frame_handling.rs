use std::net::TcpListener;
use tokio::io::AsyncWriteExt;

#[tokio::test]
async fn partial_close_does_not_apply_write_but_complete_does() {
    // Ensure registry available for building requests
    let _ = melsec_mc::command_registry::CommandRegistry::load_and_set_global_from_src();

    // pick ephemeral port
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral");
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    // start server
    let server = melsec_mc_mock::MockServer::new();
    let srv = server.clone();
    tokio::spawn(async move {
        let _ = srv.run_listener(&format!("127.0.0.1:{}", port)).await;
    });

    // allow server to start
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Build a write_words request for D0 = 0x9ABC
    let values = vec![0x9ABCu16];
    let params = melsec_mc::command_registry::create_write_words_params("D0", &values);
    let reg = melsec_mc::command_registry::GLOBAL_COMMAND_REGISTRY
        .get()
        .expect("registry");
    let spec = reg
        .get(melsec_mc::commands::Command::WriteWords)
        .expect("write command");
    let request_data = spec
        .build_request(&params, Some(melsec_mc::plc_series::PLCSeries::Q))
        .expect("build request");

    let mc_req = melsec_mc::request::McRequest::new()
        .with_access_route(melsec_mc::mc_define::AccessRoute::default())
        .try_with_request_data(request_data)
        .expect("mc request");
    let payload = mc_req.build();

    // split near the end leaving last 2 bytes
    let split = payload.len().saturating_sub(2);

    // 1) partial send then close -> should NOT apply write
    {
        let mut s = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .expect("connect");
        s.write_all(&payload[..split]).await.expect("write partial");
        // gracefully shutdown write side and close
        let _ = s.shutdown().await;
        drop(s);

        // give server time
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        let got = server.get_words("D", 0, 1).await;
        // default is zeros
        assert_eq!(got, vec![0u16]);
    }

    // 2) partial send then complete on same connection -> should apply
    {
        let mut s = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .expect("connect2");
        s.write_all(&payload[..split])
            .await
            .expect("write partial2");
        // wait briefly
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        // send remaining
        s.write_all(&payload[split..]).await.expect("write rest");
        // allow processing
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        let got = server.get_words("D", 0, 1).await;
        assert_eq!(got, vec![0x9ABCu16]);

        let _ = s.shutdown().await;
        drop(s);
    }
}
