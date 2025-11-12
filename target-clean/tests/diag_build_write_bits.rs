use melsec_mc::command_registry::create_write_bits_params;
use melsec_mc::command_registry::CommandRegistry;
use melsec_mc::commands::Command;
use tracing::debug;

#[test]
fn diag_build_write_bits_params() {
    // Build params for write_bits targeting M0 with 3 bits
    let params = create_write_bits_params("M0", &[true, false, true]);
    // Load command spec from embedded commands.toml
    let reg = include_str!("../src/commands.toml")
        .parse::<CommandRegistry>()
        .expect("load commands");
    let spec = reg.get(Command::WriteBits).expect("spec");
    let request_bytes = spec.build_request(&params, None).expect("build_request");

    // Print hex
    let hex = request_bytes
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(" ");
    debug!("REQUEST BYTES: {hex}");

    // According to request_format: command(2) subcommand(2) start_addr(3) device_code(1) count(2) payload...
    if request_bytes.len() >= 10 {
        let device_code = request_bytes[7];
        let count = u16::from_le_bytes([request_bytes[8], request_bytes[9]]);
        let payload = &request_bytes[10..];
        debug!(
            "device_code=0x{device_code:02X}, count={count}, payload_len={}",
            payload.len()
        );
    } else {
        debug!("request too short to decode header");
    }
}
