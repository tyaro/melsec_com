use melsec_mc::endpoint::ConnectionTarget;
use melsec_mc::init_defaults;
use melsec_mc::mc_client::McClient;
use melsec_mc::mc_define::Protocol;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_defaults()?;

    let tcp_addr = "127.0.0.1";
    let tcp_port = 5000u16;
    let udp_addr = "127.0.0.1";
    let udp_port = 5001u16;

    // write via TCP
    let ttarget = ConnectionTarget::direct(tcp_addr.to_string(), tcp_port);
    let tcp_client = McClient::new()
        .with_target(ttarget)
        .with_protocol(Protocol::Tcp);
    println!("Writing words [0x1234,0x5678] to D100 via TCP {tcp_addr}:{tcp_port}");
    let write_res = tcp_client
        .write_words("D100", &[0x1234u16, 0x5678u16])
        .await;
    println!("write result: {write_res:?}");

    // read via UDP
    let utarget = ConnectionTarget::direct(udp_addr.to_string(), udp_port);
    let udp_client = McClient::new()
        .with_target(utarget)
        .with_protocol(Protocol::Udp);
    println!("Reading words D100 via UDP {udp_addr}:{udp_port}");
    let read_res = udp_client.read_words("D100", 2).await;
    println!("read result: {read_res:?}");

    Ok(())
}
