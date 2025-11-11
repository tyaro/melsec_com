use std::net::TcpListener;
use std::process::Command;
use tokio::io::AsyncWriteExt;

fn log_netstat(label: &str) {
    // Run `netstat -ano` (Windows) and print captured output to tracing for test debugging.
    // Only run when MELSEC_DUMP_NETSTAT=1 is set to avoid noisy CI/test runs.
    if std::env::var("MELSEC_DUMP_NETSTAT").ok().as_deref() != Some("1") {
        return;
    }

    match Command::new("netstat").arg("-ano").output() {
        Ok(out) => {
            if let Ok(s) = String::from_utf8(out.stdout) {
                tracing::debug!(netstat_label = %label, stdout = %s, "netstat stdout");
            }
            if let Ok(e) = String::from_utf8(out.stderr) {
                if !e.is_empty() {
                    tracing::debug!(netstat_label = %label, stderr = %e, "netstat stderr");
                }
            }
        }
        Err(err) => {
            tracing::debug!(netstat_label = %label, error = ?err, "failed to run netstat");
        }
    }
}

#[tokio::test]
async fn partial_send_then_close_server_remains_healthy() {
    // short TIM_AWAIT for fast test
    std::env::set_var("MELSEC_MOCK_TIM_AWAIT_MS", "500");

    // pick ephemeral port
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral");
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    log_netstat("before_server_start");

    // start mock server
    let server = melsec_mc_mock::MockServer::new();
    let srv = server.clone();
    tokio::spawn(async move {
        let _ = srv.run_listener(&format!("127.0.0.1:{}", port)).await;
    });

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    log_netstat("after_server_start");

    // connect and send only partial bytes, then shutdown
    let mut s = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("connect");
    // send a few arbitrary bytes (incomplete frame)
    let _ = s.write_all(&[0x00u8, 0xFFu8, 0xFFu8]).await;
    // close write side and drop
    let _ = s.shutdown().await;
    drop(s);

    log_netstat("after_partial_close");

    // wait a bit for server to process
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    log_netstat("before_real_client");

    // Now ensure server still processes a normal request: write then read
    let mut client =
        melsec_mc::mc_client::McClient::new().with_plc_series(melsec_mc::plc_series::PLCSeries::Q);
    let target = melsec_mc::endpoint::ConnectionTarget::direct("127.0.0.1", port);
    client = client.with_target(target);

    let write_res = client.write_words("D0", &[0x4321u16]).await;
    assert!(
        write_res.is_ok(),
        "write failed after partial-close: {:?}",
        write_res
    );

    let read_res = client.read_words_as::<u16>("D0", 1).await;
    assert!(
        read_res.is_ok(),
        "read failed after partial-close: {:?}",
        read_res
    );
    let vals = read_res.unwrap();
    assert_eq!(vals.len(), 1);
    assert_eq!(vals[0], 0x4321u16);
}

#[tokio::test]
async fn immediate_close_then_server_remains_healthy() {
    std::env::set_var("MELSEC_MOCK_TIM_AWAIT_MS", "500");

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral");
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    log_netstat("before_server_start");

    let server = melsec_mc_mock::MockServer::new();
    let srv = server.clone();
    tokio::spawn(async move {
        let _ = srv.run_listener(&format!("127.0.0.1:{}", port)).await;
    });

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    log_netstat("after_server_start");

    // connect and immediately drop without sending
    let s = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("connect");
    drop(s);

    log_netstat("after_immediate_drop");

    // give server a moment
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    log_netstat("before_real_client");

    // ensure server still processes requests normally
    let mut client =
        melsec_mc::mc_client::McClient::new().with_plc_series(melsec_mc::plc_series::PLCSeries::Q);
    let target = melsec_mc::endpoint::ConnectionTarget::direct("127.0.0.1", port);
    client = client.with_target(target);

    let write_res = client.write_words("D0", &[0x5555u16]).await;
    assert!(
        write_res.is_ok(),
        "write failed after immediate-close: {:?}",
        write_res
    );

    let read_res = client.read_words_as::<u16>("D0", 1).await;
    assert!(
        read_res.is_ok(),
        "read failed after immediate-close: {:?}",
        read_res
    );
    let vals = read_res.unwrap();
    assert_eq!(vals.len(), 1);
    assert_eq!(vals[0], 0x5555u16);
}
