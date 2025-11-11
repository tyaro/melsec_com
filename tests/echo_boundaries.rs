use std::time::Duration;

use melsec_mc_mock::MockServer;

#[tokio::test]
async fn echo_empty_payload_should_error() -> Result<(), Box<dyn std::error::Error>> {
    let server = MockServer::new();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    let srv_clone = server.clone();
    tokio::spawn(async move {
        let _ = srv_clone.run_listener_on(listener).await;
    });
    tokio::time::sleep(Duration::from_millis(50)).await;

    let client = melsec_mc::mc_client::McClient::new().with_target(
        melsec_mc::endpoint::ConnectionTarget::direct("127.0.0.1", port),
    );

    let res = client.echo("").await;
    assert!(res.is_err(), "expected error for empty payload");
    Ok(())
}

#[tokio::test]
async fn echo_too_long_payload_should_error() -> Result<(), Box<dyn std::error::Error>> {
    let server = MockServer::new();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    let srv_clone = server.clone();
    tokio::spawn(async move {
        let _ = srv_clone.run_listener_on(listener).await;
    });
    tokio::time::sleep(Duration::from_millis(50)).await;

    let client = melsec_mc::mc_client::McClient::new().with_target(
        melsec_mc::endpoint::ConnectionTarget::direct("127.0.0.1", port),
    );

    let payload = "A".repeat(961);
    let res = client.echo(&payload).await;
    assert!(res.is_err(), "expected error for too-long payload");
    Ok(())
}

#[tokio::test]
async fn echo_invalid_char_payload_should_error() -> Result<(), Box<dyn std::error::Error>> {
    let server = MockServer::new();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    let srv_clone = server.clone();
    tokio::spawn(async move {
        let _ = srv_clone.run_listener_on(listener).await;
    });
    tokio::time::sleep(Duration::from_millis(50)).await;

    let client = melsec_mc::mc_client::McClient::new().with_target(
        melsec_mc::endpoint::ConnectionTarget::direct("127.0.0.1", port),
    );

    let payload = "12G4"; // 'G' is invalid
    let res = client.echo(payload).await;
    assert!(res.is_err(), "expected error for invalid char in payload");
    Ok(())
}

#[tokio::test]
async fn echo_max_length_ok() -> Result<(), Box<dyn std::error::Error>> {
    let server = MockServer::new();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    let srv_clone = server.clone();
    tokio::spawn(async move {
        let _ = srv_clone.run_listener_on(listener).await;
    });
    tokio::time::sleep(Duration::from_millis(50)).await;

    let client = melsec_mc::mc_client::McClient::new().with_target(
        melsec_mc::endpoint::ConnectionTarget::direct("127.0.0.1", port),
    );

    let payload = "A".repeat(960);
    let res = client.echo(&payload).await?;
    assert_eq!(res, payload);
    Ok(())
}

#[tokio::test]
async fn echo_lowercase_ok() -> Result<(), Box<dyn std::error::Error>> {
    let server = MockServer::new();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    let srv_clone = server.clone();
    tokio::spawn(async move {
        let _ = srv_clone.run_listener_on(listener).await;
    });
    tokio::time::sleep(Duration::from_millis(50)).await;

    let client = melsec_mc::mc_client::McClient::new().with_target(
        melsec_mc::endpoint::ConnectionTarget::direct("127.0.0.1", port),
    );

    let payload = "abcdef012345";
    let res = client.echo(payload).await?;
    assert_eq!(res, payload);
    Ok(())
}
