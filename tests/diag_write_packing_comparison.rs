use melsec_mc::command_registry::create_write_bits_params;
use melsec_mc::command_registry::CommandRegistry;
use melsec_mc::commands::Command;
use tracing::debug;

fn pack_nibbles_high_first(vals: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < vals.len() {
        let high = vals[i] & 0x0F;
        let low = if i + 1 < vals.len() {
            vals[i + 1] & 0x0F
        } else {
            0
        };
        out.push((high << 4) | low);
        i += 2;
    }
    out
}

fn pack_nibbles_low_first(vals: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < vals.len() {
        let low = vals[i] & 0x0F;
        let high = if i + 1 < vals.len() {
            vals[i + 1] & 0x0F
        } else {
            0
        };
        out.push((high << 4) | low);
        i += 2;
    }
    out
}

fn pack_bits_lsb_first(bools: &[bool]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut byte = 0u8;
    for (i, &v) in bools.iter().enumerate() {
        let bit_idx = i % 8;
        if v {
            byte |= 1 << bit_idx;
        }
        if bit_idx == 7 || i == bools.len() - 1 {
            out.push(byte);
            byte = 0;
        }
    }
    out
}

fn pack_bits_msb_first(bools: &[bool]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut byte = 0u8;
    for (i, &v) in bools.iter().enumerate() {
        let bit_idx = 7 - (i % 8);
        if v {
            byte |= 1 << bit_idx;
        }
        if (i % 8) == 7 || i == bools.len() - 1 {
            out.push(byte);
            byte = 0;
        }
    }
    out
}

#[test]
fn diag_compare_write_bits_packings() {
    // target: M0, booleans: [true, false, true]
    let bools = [true, false, true];
    let nib_vals: Vec<u8> = bools.iter().map(|&b| u8::from(b)).collect();

    let payload_byte_per_bit: Vec<u8> = bools.iter().map(|&b| u8::from(b)).collect();
    let payload_nibble_high = pack_nibbles_high_first(&nib_vals);
    let payload_nibble_low = pack_nibbles_low_first(&nib_vals);
    let payload_bits_lsb = pack_bits_lsb_first(&bools);
    let payload_bits_msb_v = pack_bits_msb_first(&bools);

    let registry = include_str!("../src/commands.toml")
        .parse::<CommandRegistry>()
        .expect("load commands");
    let spec = registry.get(Command::WriteBits).expect("spec");

    let variants = vec![
        ("byte_per_bit", payload_byte_per_bit),
        ("nibble_high_first", payload_nibble_high),
        ("nibble_low_first", payload_nibble_low),
        ("bits_lsb_first", payload_bits_lsb),
        ("bits_msb_first", payload_bits_msb_v),
    ];

    for (name, payload) in variants {
        let params = {
            // create params map similar to create_write_bits_params but with arbitrary payload
            let mut p = create_write_bits_params("M0", &bools);
            if let Some(obj) = p.as_object_mut() {
                let arr = payload
                    .iter()
                    .map(|b| serde_json::Value::Number(serde_json::Number::from(*b)))
                    .collect::<Vec<_>>();
                obj.insert("payload".to_string(), serde_json::Value::Array(arr));
            }
            p
        };
        let request_bytes = spec.build_request(&params, None).expect("build_request");
        let hex = request_bytes
            .iter()
            .map(|b| format!("{b:02X}"))
            .collect::<Vec<_>>()
            .join(" ");
        debug!("Variant {name} -> REQUEST: {hex}");
        if request_bytes.len() >= 10 {
            let device_code = request_bytes[7];
            let count = u16::from_le_bytes([request_bytes[8], request_bytes[9]]);
            debug!(
                "  device_code=0x{device_code:02X}, count={count}, payload_len={}",
                request_bytes.len() - 10
            );
        }
    }
}
