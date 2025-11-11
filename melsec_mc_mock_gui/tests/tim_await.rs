use std::io::ErrorKind;
use std::net::TcpListener;
use tokio::io::AsyncReadExt;

#[tokio::test]
async fn connection_closed_on_tim_await() {
    // configure a short TIM_AWAIT so the test runs fast
    std::env::set_var("MELSEC_MOCK_TIM_AWAIT_MS", "500");

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

    // connect a tcp client but do not send anything
    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("connect to server");

    // wait longer than tim_await (500ms) to allow server to close the idle connection
    tokio::time::sleep(std::time::Duration::from_millis(800)).await;

    // now attempt to read; if server closed the connection we should get Ok(0)
    let mut buf = [0u8; 8];
    let res = tokio::time::timeout(std::time::Duration::from_secs(1), stream.read(&mut buf)).await;
    match res {
        Ok(Ok(0)) => {
            // expected: server closed connection (graceful EOF)
        }
        Ok(Ok(n)) => panic!("expected connection closed, but read {} bytes", n),
        Ok(Err(e)) => {
            // accept connection reset as a valid outcome when the server forces RST
            if e.kind() == ErrorKind::ConnectionReset {
                // acceptable: server closed with RST
            } else {
                panic!("read error: {}", e);
            }
        }
        Err(_) => panic!("read timed out (no EOF) - server did not close connection"),
    }
}
