use crate::error::MelsecError;
use crate::mc_define::{MC_SUBHEADER_REQUEST, MC_SUBHEADER_RESPONSE};

/// Result of parsing a frame header (MC3E/MC4E compatible).
pub struct FrameParseResult {
    pub subheader: [u8; 2],
    pub access_route: [u8; 5],
    pub request_data_len: u16,
    /// Offset into the original payload where the data bytes start
    pub data_offset: usize,
    /// Number of data bytes (not including end_code)
    pub data_bytes: usize,
    pub has_end_code: bool,
    pub end_code: Option<u16>,
    /// Optional monitor timer (present in some subheader+MC3E request formats)
    pub monitor_timer: Option<u16>,
    pub serial_number: Option<u16>,
}

/// Try to parse a complete frame header and compute the expected full frame length.
/// Returns Ok(Some((frame_len, header_len, serial_opt))) when a valid header can be
/// determined (even if the buffer doesn't yet contain the full frame). Returns
/// Ok(None) when the header is incomplete or unrecognized. Returns Err on malformed header.
pub fn detect_frame(buf: &[u8]) -> Result<Option<(usize, usize, Option<u16>)>, MelsecError> {
    if buf.len() < 2 {
        return Ok(None);
    }
    // MC4E style: need at least 15 bytes header to compute length
    if buf.len() >= 15 && (buf[0] == MC_SUBHEADER_REQUEST[0] || buf[0] == MC_SUBHEADER_RESPONSE[0])
    {
        let data_len = u16::from_le_bytes([buf[11], buf[12]]) as usize;
        if data_len < 2 {
            return Err(MelsecError::Protocol(format!(
                "MC4E invalid data_len (must be >=2): {data_len}"
            )));
        }
        let data_bytes = data_len - 2;
        let header_len = 15usize;
        let frame_len = header_len.saturating_add(data_bytes);
        let serial = if buf.len() >= 4 {
            Some(u16::from_le_bytes([buf[2], buf[3]]))
        } else {
            None
        };
        return Ok(Some((frame_len, header_len, serial)));
    }

    // Subheader + MC3E-style header: subheader(2) + access_route(5) + data_len(2) + end_code(2)
    // Total header length = 11. This variant appears when a subheader prefix is present
    // but the frame otherwise follows MC3E layout (no serial).
    if buf.len() >= 11 {
        // data_len at offsets 7..8 (LE)
        let data_len = u16::from_le_bytes([buf[7], buf[8]]) as usize;
        if data_len >= 2 {
            let data_bytes = data_len - 2;
            let header_len = 11usize;
            let frame_len = header_len.saturating_add(data_bytes);
            return Ok(Some((frame_len, header_len, None)));
        }
    }

    // MC3E style: header 9 bytes with data_len at 5..6
    if buf.len() >= 9 {
        let data_len = u16::from_le_bytes([buf[5], buf[6]]) as usize;
        if data_len >= 2 {
            let data_bytes = data_len - 2;
            let header_len = 9usize;
            let frame_len = header_len.saturating_add(data_bytes);
            return Ok(Some((frame_len, header_len, None)));
        }
    }

    // Not enough context to identify a frame yet
    Ok(None)
}

/// Parse a complete MC frame payload into header fields and the data slice offsets.
pub fn parse_frame(payload: &[u8]) -> Result<FrameParseResult, MelsecError> {
    if payload.len() < 2 {
        return Err(MelsecError::Protocol("payload too short".into()));
    }
    let subheader = [payload[0], payload[1]];

    // MC4E frame (serial + access_route + len + end_code)
    if payload.len() >= 15
        && (payload[0] == MC_SUBHEADER_REQUEST[0] || payload[0] == MC_SUBHEADER_RESPONSE[0])
    {
        let serial = u16::from_le_bytes([payload[2], payload[3]]);
        let mut access_route = [0u8; 5];
        access_route.copy_from_slice(&payload[6..11]);
        let data_len_bytes = u16::from_le_bytes([payload[11], payload[12]]) as usize;
        if data_len_bytes < 2 {
            return Err(MelsecError::Protocol(format!(
                "MC4E invalid data_len (must be >=2): {data_len_bytes}"
            )));
        }
        let end_code_val = u16::from_le_bytes([payload[13], payload[14]]);
        let header_len = 15usize;
        let data_bytes = data_len_bytes - 2;
        let data_offset = header_len;
        let _data = if payload.len() >= header_len {
            &payload[data_offset..std::cmp::min(payload.len(), data_offset + data_bytes)]
        } else {
            &[]
        };
        // Accept truncated data if end_code==0 per legacy behavior
        if payload.len() >= header_len || end_code_val == 0x0000 {
            return Ok(FrameParseResult {
                subheader,
                access_route,
                request_data_len: u16::try_from(data_len_bytes).map_err(|_| {
                    MelsecError::Protocol(format!("data_len too large: {data_len_bytes}"))
                })?,
                data_offset,
                data_bytes,
                has_end_code: true,
                end_code: Some(end_code_val),
                monitor_timer: None,
                serial_number: Some(serial),
            });
        }
        return Err(MelsecError::Protocol(format!(
            "MC4E payload length mismatch: have {} bytes, expected header+{}",
            payload.len(),
            data_bytes
        )));
    }

    // Subheader + MC3E-like frame (subheader followed by 9-byte MC3E header => total 11 bytes header)
    if payload.len() >= 11 {
        // subheader at 0..2, access_route at 2..7, data_len at 7..9, end_code at 9..11
        let mut access_route = [0u8; 5];
        access_route.copy_from_slice(&payload[2..7]);
        let data_len_bytes = u16::from_le_bytes([payload[7], payload[8]]) as usize;
        if data_len_bytes >= 2 {
            let end_code_val = u16::from_le_bytes([payload[9], payload[10]]);
            let header_len = 11usize;
            let data_bytes = data_len_bytes - 2;
            let data_offset = header_len;
            if payload.len() >= header_len || end_code_val == 0x0000 {
                // For subheader+MC3E request frames, bytes[9..10] are treated as monitor timer
                let monitor = u16::from_le_bytes([payload[9], payload[10]]);
                return Ok(FrameParseResult {
                    subheader: [payload[0], payload[1]],
                    access_route,
                    request_data_len: u16::try_from(data_len_bytes).map_err(|_| {
                        MelsecError::Protocol(format!("data_len too large: {data_len_bytes}"))
                    })?,
                    data_offset,
                    data_bytes,
                    has_end_code: false,
                    end_code: None,
                    monitor_timer: Some(monitor),
                    serial_number: None,
                });
            }
        }
    }

    // MC3E-like frame (no serial)
    if payload.len() >= 9 {
        let mut access_route = [0u8; 5];
        access_route.copy_from_slice(&payload[0..5]);
        let data_len_bytes = u16::from_le_bytes([payload[5], payload[6]]) as usize;
        if data_len_bytes >= 2 {
            let end_code_val = u16::from_le_bytes([payload[7], payload[8]]);
            let header_len = 9usize;
            let data_bytes = data_len_bytes - 2;
            let data_offset = header_len;
            if payload.len() >= header_len || end_code_val == 0x0000 {
                return Ok(FrameParseResult {
                    subheader: [0xD0u8, 0x00u8],
                    access_route,
                    request_data_len: u16::try_from(data_len_bytes).map_err(|_| {
                        MelsecError::Protocol(format!("data_len too large: {data_len_bytes}"))
                    })?,
                    data_offset,
                    data_bytes,
                    has_end_code: true,
                    end_code: Some(end_code_val),
                    monitor_timer: None,
                    serial_number: None,
                });
            }
        }
    }

    // Fallback: treat remaining bytes as raw payload without explicit end_code
    let data = if payload.len() > 2 {
        &payload[2..]
    } else {
        &[]
    };
    let request_data_len = u16::try_from(data.len())
        .map_err(|_| MelsecError::Protocol(format!("data_len too large: {}", data.len())))?;
    Ok(FrameParseResult {
        subheader,
        access_route: [0u8; 5],
        request_data_len,
        data_offset: 2,
        data_bytes: data.len(),
        has_end_code: false,
        end_code: None,
        monitor_timer: None,
        serial_number: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mc_define::MC_SUBHEADER_REQUEST;

    #[test]
    fn test_detect_frame_incomplete_small() {
        let buf: Vec<u8> = vec![MC_SUBHEADER_REQUEST[0]]; // only 1 byte
        let res = detect_frame(&buf).expect("detect_frame call");
        assert!(res.is_none(), "expected None for too-small buffer");
    }

    #[test]
    fn test_detect_frame_mc4e_header_present() {
        // craft header: subheader(2)=MC_SUBHEADER_REQUEST, serial(2)=1, reserved(2)=0,
        // access_route(5)=zeros, data_len(2)=6 (meaning data_bytes=4)
        let buf = vec![
            MC_SUBHEADER_REQUEST[0],
            MC_SUBHEADER_REQUEST[1],
            1,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            6,
            0,
            0,
            0,
        ];
        // buf length is 15 (header) but frame_len should be 15 + (6-2) = 19
        let res = detect_frame(&buf).expect("detect_frame returned Err");
        assert!(
            res.is_some(),
            "expected detect_frame to return Some for full header"
        );
        let (frame_len, header_len, serial_opt) =
            res.expect("frame length/result should be present");
        assert_eq!(header_len, 15);
        assert_eq!(frame_len, 19);
        assert_eq!(serial_opt, Some(1u16));
    }

    #[test]
    fn test_parse_frame_end_code_zero_truncation() {
        // Build MC4E header with data_len=6 (4 data bytes expected) but only header present;
        // end_code set to 0 so truncated data should be accepted.
        let mut payload = vec![
            MC_SUBHEADER_REQUEST[0],
            MC_SUBHEADER_REQUEST[1],
            2,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            6,
            0,
            0,
            0,
        ];
        // set end_code bytes to 0x0000
        payload[13] = 0x00;
        payload[14] = 0x00;
        let pr =
            parse_frame(&payload).expect("parse_frame should accept truncated with end_code 0");
        assert!(pr.has_end_code);
        assert_eq!(pr.end_code, Some(0));
        assert_eq!(pr.data_bytes, 4);
        assert_eq!(pr.data_offset, 15);
    }

    #[test]
    fn test_parse_frame_too_short_error() {
        // Very short payload (less than 2 bytes) should error
        let payload: Vec<u8> = vec![0x50];
        assert!(parse_frame(&payload).is_err());
    }

    #[test]
    fn test_subheader_mc3e_parse_monitor_timer() {
        // the user-supplied example: subheader(2) + access_route(5) + data_len(2) + monitor_timer(2) + 10 bytes data
        let buf = vec![
            0x50, 0x00, // subheader
            0x00, 0xFF, 0xFF, 0x03, 0x00, // access_route
            0x0C, 0x00, // data_len = 12
            0x0A, 0x00, // monitor_timer = 10
            // 10 bytes data
            0x01, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0xA8, 0x01, 0x00,
        ];
        let pr = parse_frame(&buf).expect("parse_frame should handle subheader+mc3e");
        assert_eq!(pr.subheader, [0x50, 0x00]);
        assert_eq!(pr.access_route, [0x00, 0xFF, 0xFF, 0x03, 0x00]);
        assert_eq!(pr.request_data_len, 12);
        assert_eq!(pr.monitor_timer, Some(10u16));
        assert_eq!(pr.data_bytes, 10usize);
        assert_eq!(pr.data_offset, 11usize);
    }
}
