use melsec_mc::response::parse_mc_payload;

fn hex_to_bytes(s: &str) -> Vec<u8> {
    s.split_whitespace()
        .map(|b| u8::from_str_radix(b, 16).unwrap())
        .collect()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Example bytes from the user
    let s = "50 00 00 FF FF 03 00 0C 00 0A 00 01 04 00 00 00 00 00 A8 01 00";
    let buf = hex_to_bytes(s);
    println!("input ({} bytes): {}", buf.len(), s);
    match parse_mc_payload(&buf) {
        Ok(resp) => {
            println!(
                "subheader: {:02X} {:02X}",
                resp.subheader[0], resp.subheader[1]
            );
            println!(
                "access_route: {:02X} {:02X} {:02X} {:02X} {:02X}",
                resp.access_route[0],
                resp.access_route[1],
                resp.access_route[2],
                resp.access_route[3],
                resp.access_route[4]
            );
            println!("request_data_len: {}", resp.request_data_len);
            println!("has_end_code: {}", resp.has_end_code);
            println!("end_code: {:?}", resp.end_code);
            println!("serial_number: {:?}", resp.serial_number);
            println!(
                "data (len={}): {}",
                resp.data.len(),
                resp.data
                    .iter()
                    .map(|b| format!("{:02X}", b))
                    .collect::<Vec<_>>()
                    .join(" ")
            );
        }
        Err(e) => println!("parse failed: {}", e),
    }
    Ok(())
}
