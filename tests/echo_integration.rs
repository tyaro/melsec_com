use std::time::Duration;

use melsec_mc_mock::MockServer;

#[tokio::test]
async fn echo_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    // start mock server on ephemeral port
    let server = MockServer::new();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    let srv_clone = server.clone();
    tokio::spawn(async move {
        let _ = srv_clone.run_listener_on(listener).await;
    });

    // give server a moment to start
    tokio::time::sleep(Duration::from_millis(50)).await;

    let client = melsec_mc::mc_client::McClient::new().with_target(
        melsec_mc::endpoint::ConnectionTarget::direct("127.0.0.1", port),
    );

    let payload = "ABCDEF012345"; // allowed chars 0-9,A-F
    let res = client.echo(payload).await?;
    assert_eq!(res, payload);
    Ok(())
}

#[tokio::test]
async fn echo_roundtrip_1234() -> Result<(), Box<dyn std::error::Error>> {
    // start mock server on ephemeral port
    let server = MockServer::new();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    let srv_clone = server.clone();
    tokio::spawn(async move {
        let _ = srv_clone.run_listener_on(listener).await;
    });

    // give server a moment to start
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let client = melsec_mc::mc_client::McClient::new().with_target(
        melsec_mc::endpoint::ConnectionTarget::direct("127.0.0.1", port),
    );

    let payload = "1234"; // allowed chars 0-9
    let res = client.echo(payload).await?;
    assert_eq!(res, payload);
    Ok(())
}
