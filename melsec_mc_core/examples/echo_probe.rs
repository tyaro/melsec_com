use std::time::Duration;

use melsec_mc::endpoint::ConnectionTarget;
use melsec_mc::init_defaults;
use melsec_mc::mc_client::McClient;

fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{:02X}", b))
        .collect::<Vec<_>>()
        .join(" ")
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ensure registry/error codes are initialized
    init_defaults()?;

    // TCP probe using high-level McClient::echo
    let target = ConnectionTarget::direct("127.0.0.1", 5000);
    let client = McClient::new().with_target(target).with_monitoring_timer(3);

    println!("TCP: calling echo with payload '1234'");
    match client.echo("1234").await {
        Ok(resp) => println!("TCP echo response: '{}'", resp),
        Err(e) => eprintln!("TCP echo failed: {}", e),
    }

    // UDP probe: build raw MC request payload and send via transport::send_and_recv_udp
    // Request data: command (0x0619 little-endian), subcommand (0x0000 little-endian), payload ascii bytes
    let mut req_data: Vec<u8> = vec![0x19, 0x06, 0x00, 0x00];
    req_data.extend_from_slice(b"1234");

    let mc_payload = melsec_mc::request::McRequest::new()
        .with_access_route(ConnectionTarget::new().access_route)
        .try_with_request_data(req_data.clone())?
        .build();

    println!("UDP: sending MC payload (raw) {}", hex(&mc_payload));
    match melsec_mc::transport::send_and_recv_udp(
        "127.0.0.1:5001",
        &mc_payload,
        Some(Duration::from_secs(3)),
    )
    .await
    {
        Ok(buf) => {
            println!("UDP recv raw: {}", hex(&buf));
            if let Ok(resp) = melsec_mc::response::McResponse::try_new(&buf) {
                println!(
                    "UDP parsed response has_end_code={}, end_code={:?}",
                    resp.has_end_code, resp.end_code
                );
                // print data payload as ASCII
                println!(
                    "UDP response data (as ASCII): '{}'",
                    String::from_utf8_lossy(&resp.data)
                );
            } else {
                eprintln!("failed to parse UDP response");
            }
        }
        Err(e) => eprintln!("UDP send/recv failed: {}", e),
    }

    Ok(())
}
