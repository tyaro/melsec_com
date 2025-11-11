use crate::error::MelsecError;
use crate::mc_frame::parse_frame;

pub struct McResponse {
    pub subheader: [u8; 2],
    pub access_route: [u8; 5],
    pub request_data_len: u16,
    pub data: Vec<u8>,
    pub end_code: Option<u16>,
    pub has_end_code: bool,
    pub serial_number: Option<u16>,
}
impl McResponse {
    /// Non-panicking constructor which returns a Result.
    /// Prefer this in runtime code paths where malformed payloads should
    /// be handled instead of panicking.
    ///
    /// # Errors
    ///
    /// Returns `Err(MelsecError)` when `parse_mc_payload` fails.
    pub fn try_new(payload: &[u8]) -> Result<Self, MelsecError> {
        parse_mc_payload(payload)
    }
    #[must_use]
    pub const fn is_success(&self) -> bool {
        match self.end_code {
            Some(code) => code == 0,
            None => true,
        }
    }
}

/// Parse helper for external users: `parse_mc_payload` は生の MC フレームを受け取り、
/// 汎用的な `McResponse` を返します。パーサは MC3E/MC4E 両方に対応しており、
/// end-code の存在やシリアル番号の有無を `McResponse` のフィールドで表現します。
pub fn parse_mc_payload(payload: &[u8]) -> Result<McResponse, MelsecError> {
    let pr = parse_frame(payload)?;
    let data_slice = if payload.len() >= pr.data_offset {
        &payload[pr.data_offset..std::cmp::min(payload.len(), pr.data_offset + pr.data_bytes)]
    } else {
        &[]
    };
    Ok(McResponse {
        subheader: pr.subheader,
        access_route: pr.access_route,
        request_data_len: pr.request_data_len,
        data: data_slice.to_vec(),
        end_code: pr.end_code,
        has_end_code: pr.has_end_code,
        serial_number: pr.serial_number,
    })
}

// NOTE: the old `parse_mc3e_payload` compatibility wrapper was removed. Use
// `parse_mc_payload` directly.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple() {
        let p = vec![0x50, 0x00, 0x00, 0x00];
        let r = parse_mc_payload(&p).expect("parse_mc_payload should succeed for simple payload");
        assert_eq!(r.subheader, [0x50, 0x00]);
    }
}
