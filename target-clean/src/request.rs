use crate::error::MelsecError;
use crate::mc_define::AccessRoute;
use crate::mc_define::McFrameFormat;
use crate::mc_define::MC_SUBHEADER_REQUEST;
use std::sync::atomic::{AtomicU16, Ordering};

// Global serial counter for MC4E requests. Starts at 1 and increments per-request.
static SERIAL_COUNTER: AtomicU16 = AtomicU16::new(1);

fn next_serial() -> u16 {
    // Atomically increment, wrapping from 0xFFFF back to 1.
    match SERIAL_COUNTER.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |v| {
        if v == 0xFFFF {
            Some(1)
        } else {
            Some(v.wrapping_add(1))
        }
    }) {
        Ok(prev) => {
            if prev == 0xFFFF {
                1
            } else {
                prev.wrapping_add(1)
            }
        }
        Err(curr) => curr, // fallback to current value
    }
}

// McRequest
pub struct McRequest {
    pub subheader: [u8; 2],
    pub access_route: AccessRoute,
    pub request_data_len: u16,
    pub monitoring_timer: u16, // unit: 0.25 seconds, valid range: 0x0001 to 0xFFFF
    pub request_data: Vec<u8>,
    pub serial_number: u16,
}

impl Default for McRequest {
    fn default() -> Self {
        Self::new()
    }
}

impl McRequest {
    /// Builder for MC request payloads.
    ///
    /// `McRequest` は低レイヤーの MC3E/MC4E ペイロード（サブヘッダ、アクセス経路、
    /// モニタリングタイマ、要求データ等）を組み立てるための構造体です。
    /// `build_with_format` により MC3E / MC4E それぞれのバイト列を生成します。
    ///
    /// 例:
    /// ```no_run
    /// let req = melsec_mc::request::McRequest::new()
    ///     .with_access_route(melsec_mc::mc_define::AccessRoute::default())
    ///     .try_with_request_data([0x01u8, 0x02u8]).unwrap();
    /// let payload = req.build_with_format(melsec_mc::mc_define::McFrameFormat::MC4E);
    /// ```
    // builder pattern で McRequest を構築するためのメソッド群を追加
    #[must_use]
    pub fn new() -> Self {
        Self {
            subheader: MC_SUBHEADER_REQUEST,
            access_route: AccessRoute::default(),
            request_data_len: 0,
            monitoring_timer: 0x1000,
            request_data: Vec::new(),
            // assign an incremented serial number for each new request
            serial_number: next_serial(),
        }
    }
    #[must_use]
    pub const fn with_subheader(mut self, subheader: [u8; 2]) -> Self {
        self.subheader = subheader;
        self
    }
    #[must_use]
    pub const fn with_access_route(mut self, access_route: AccessRoute) -> Self {
        self.access_route = access_route;
        self
    }
    // Deprecated backward-compat wrapper `with_request_data` removed. Use
    // `try_with_request_data(...) -> Result<McRequest, MelsecError>` instead.

    /// Fallible variant of `with_request_data` which returns a `Result`.
    /// Prefer this in runtime code paths to avoid panics when the request data
    /// would overflow the u16 length field.
    pub fn try_with_request_data<R: AsRef<[u8]>>(
        mut self,
        request_data: R,
    ) -> Result<Self, MelsecError> {
        let slice = request_data.as_ref();
        // length includes 2 bytes of header in this framing
        let total_len = slice
            .len()
            .checked_add(2)
            .ok_or_else(|| MelsecError::Protocol("request_data length overflow".into()))?;
        let len_u16 = u16::try_from(total_len)
            .map_err(|_| MelsecError::Protocol("request_data too large to fit into u16".into()))?;
        self.request_data_len = len_u16;
        self.request_data = slice.to_vec();
        Ok(self)
    }
    #[must_use]
    pub const fn with_serial_number(mut self, serial_number: u16) -> Self {
        self.serial_number = serial_number;
        self
    }
    #[must_use]
    pub const fn with_monitoring_timer(mut self, monitoring_timer: u16) -> Self {
        self.monitoring_timer = monitoring_timer;
        self
    }
    #[must_use]
    pub fn build(self) -> Vec<u8> {
        // keep backward compatible behavior: default to MC4E framing
        self.build_with_format(McFrameFormat::MC4E)
    }
    /// Build the mc payload according to chosen frame format.
    pub fn build_with_format(self, format: McFrameFormat) -> Vec<u8> {
        Self::build_mc_payload_with_format(&self, format)
    }

    fn build_mc_payload_with_format(request: &Self, format: McFrameFormat) -> Vec<u8> {
        match format {
            McFrameFormat::MC4E => {
                // MC4E: subheader(2) + serial(2) + reserved(2) + access_route(5) + data_len(2) + data...
                let mut payload = Vec::new();
                payload.extend_from_slice(&request.subheader);
                payload.extend_from_slice(&request.serial_number.to_le_bytes());
                // padding/reserved 2 bytes
                payload.extend_from_slice(&0x0000u16.to_le_bytes());
                let ar_bytes = request.access_route.to_bytes();
                payload.extend_from_slice(&ar_bytes);
                payload.extend_from_slice(&request.request_data_len.to_le_bytes());
                payload.extend_from_slice(&request.monitoring_timer.to_be_bytes());
                payload.extend_from_slice(&request.request_data);
                payload
            }
            McFrameFormat::MC3E => {
                // MC3E: subheader(2) + access_route(5) + data_len(2) + monitor_timer(2, LE) + data...
                let mut payload = Vec::new();
                payload.extend_from_slice(&request.subheader);
                let ar_bytes = request.access_route.to_bytes();
                payload.extend_from_slice(&ar_bytes);
                payload.extend_from_slice(&request.request_data_len.to_le_bytes());
                payload.extend_from_slice(&request.monitoring_timer.to_le_bytes());
                payload.extend_from_slice(&request.request_data);
                payload
            }
        }
    }

    /// Try to construct an McRequest from a raw MC frame payload (incoming).
    /// This is the inverse of `build_mc_payload` and is useful for servers
    /// which receive requests and want to interpret them as a typed request
    /// object. It uses `mc_frame::parse_frame` to extract header fields.
    pub fn try_from_payload(payload: &[u8]) -> Result<Self, MelsecError> {
        let pr = crate::mc_frame::parse_frame(payload)?;
        // extract request data slice
        let data_slice = if payload.len() >= pr.data_offset {
            &payload[pr.data_offset..std::cmp::min(payload.len(), pr.data_offset + pr.data_bytes)]
        } else {
            &[]
        };
        // construct AccessRoute from parsed 5-byte array
        let ar = crate::mc_define::AccessRoute {
            network_number: pr.access_route[0],
            pc_number: pr.access_route[1],
            io_number: u16::from_le_bytes([pr.access_route[2], pr.access_route[3]]),
            station_number: pr.access_route[4],
        };
        Ok(McRequest {
            subheader: pr.subheader,
            access_route: ar,
            request_data_len: pr.request_data_len,
            monitoring_timer: pr.monitor_timer.unwrap_or(0x1000),
            request_data: data_slice.to_vec(),
            serial_number: pr.serial_number.unwrap_or(0),
        })
    }

    // シリアルナンバーは
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mc_define::MC_SUBHEADER_REQUEST;

    #[test]
    fn test_try_from_payload_subheader_mc3e_monitor_timer() {
        // subheader(2) + access_route(5) + data_len(2) + monitor_timer(2) + 10 bytes data
        let buf = vec![
            0x50, 0x00, // subheader
            0x00, 0xFF, 0xFF, 0x03, 0x00, // access_route
            0x0C, 0x00, // data_len = 12
            0x0A,
            0x00, // monitor_timer = 10(2.5秒) ※設定範囲は0x0001〜0xFFFF(単位0.25秒)
            // 10 bytes data
            0x01, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0xA8, 0x01, 0x00,
        ];
        let r = McRequest::try_from_payload(&buf).expect("should parse subheader+mc3e request");
        assert_eq!(r.subheader, [0x50, 0x00]);
        assert_eq!(r.access_route.to_bytes(), [0x00, 0xFF, 0xFF, 0x03, 0x00]);
        assert_eq!(r.request_data_len, 12);
        assert_eq!(r.monitoring_timer, 10u16);
        assert_eq!(r.request_data.len(), 10usize);
    }

    #[test]
    fn test_try_from_payload_mc3e_no_subheader() {
        // MC3E-style frame: access_route(5) + data_len(2) + end_code(2) + data(2)
        let buf = vec![
            0x00, 0xFF, 0xFF, 0x03, 0x00, // access_route
            0x04, 0x00, // data_len = 4 -> data_bytes = 2
            0x00, 0x00, // end_code
            0x11, 0x22, // 2 bytes data
        ];
        let r =
            McRequest::try_from_payload(&buf).expect("should parse mc3e frame without subheader");
        // parse_frame assigns default subheader 0xD0 0x00 for this variant
        assert_eq!(r.subheader, [0xD0, 0x00]);
        assert_eq!(r.access_route.to_bytes(), [0x00, 0xFF, 0xFF, 0x03, 0x00]);
        assert_eq!(r.request_data_len, 4);
        // monitoring_timer should default to 0x1000 when not present
        assert_eq!(r.monitoring_timer, 0x1000);
        assert_eq!(r.request_data, vec![0x11u8, 0x22u8]);
    }

    #[test]
    fn test_try_from_payload_mc4e_serial_and_data() {
        // MC4E header with serial(2) + reserved(2) + access_route(5) + data_len(2) + end_code(2)
        // then 4 bytes of data
        let mut buf = vec![
            MC_SUBHEADER_REQUEST[0],
            MC_SUBHEADER_REQUEST[1], // subheader
            0x02,
            0x00, // serial = 2
            0x00,
            0x00, // reserved
            0x00,
            0xFF,
            0xFF,
            0x03,
            0x00, // access_route
            0x06,
            0x00, // data_len = 6 -> data_bytes = 4
            0x00,
            0x00, // end_code
        ];
        buf.extend_from_slice(&[0xAA, 0xBB, 0xCC, 0xDD]);
        let r = McRequest::try_from_payload(&buf).expect("should parse mc4e frame");
        assert_eq!(r.subheader, MC_SUBHEADER_REQUEST);
        assert_eq!(r.serial_number, 2u16);
        assert_eq!(r.access_route.to_bytes(), [0x00, 0xFF, 0xFF, 0x03, 0x00]);
        assert_eq!(r.request_data_len, 6);
        assert_eq!(r.request_data, vec![0xAAu8, 0xBBu8, 0xCCu8, 0xDDu8]);
        // no monitor_timer in MC4E parsed frame -> default
        assert_eq!(r.monitoring_timer, 0x1000);
    }

    #[test]
    fn test_build_with_format_mc4e_includes_monitoring_timer_be_and_offsets() {
        use crate::mc_define::MC_SUBHEADER_REQUEST;
        // Prepare a small request data payload (2 bytes)
        let req = McRequest::new()
            .with_subheader(MC_SUBHEADER_REQUEST)
            .with_serial_number(0x1234)
            .with_monitoring_timer(0x0A0B);
        let req = req
            .try_with_request_data([0xAAu8, 0xBBu8])
            .expect("set data");
        let payload = req.build_with_format(crate::mc_define::McFrameFormat::MC4E);

        // Offsets: 0..2 subheader, 2..4 serial(LE), 4..6 reserved, 6..11 access_route,
        // 11..13 data_len (LE), 13..15 monitoring_timer (BE), 15.. data
        assert_eq!(payload[0], MC_SUBHEADER_REQUEST[0]);
        assert_eq!(payload[1], MC_SUBHEADER_REQUEST[1]);
        assert_eq!(u16::from_le_bytes([payload[2], payload[3]]), 0x1234);
        // reserved bytes 4..5 should be zero
        assert_eq!(payload[4], 0x00);
        assert_eq!(payload[5], 0x00);
        // data_len should be 4 (data_len includes 2 bytes of header per existing semantics)
        let data_len = u16::from_le_bytes([payload[11], payload[12]]);
        // request_data was 2 bytes, request_data_len = data.len() + 2 == 4
        assert_eq!(data_len, 4u16);
        // monitoring_timer inserted as big-endian at 13..15
        assert_eq!([payload[13], payload[14]], 0x0A0B_u16.to_be_bytes());
        // data starts at offset 15
        assert_eq!(&payload[15..], &[0xAAu8, 0xBBu8]);
    }

    #[test]
    fn test_build_with_format_mc3e_includes_monitoring_timer_le_and_offsets() {
        use crate::mc_define::MC_SUBHEADER_REQUEST;
        let req = McRequest::new()
            .with_subheader(MC_SUBHEADER_REQUEST)
            .with_monitoring_timer(0x0A0B);
        let req = req
            .try_with_request_data([0x11u8, 0x22u8, 0x33u8])
            .expect("set data");
        let payload = req.build_with_format(crate::mc_define::McFrameFormat::MC3E);

        // Offsets: 0..2 subheader, 2..7 access_route, 7..9 data_len, 9..11 monitoring_timer (LE), 11.. data
        assert_eq!(payload[0], MC_SUBHEADER_REQUEST[0]);
        assert_eq!(payload[1], MC_SUBHEADER_REQUEST[1]);
        // data_len should be request_data_len (data.len()+2)
        let data_len = u16::from_le_bytes([payload[7], payload[8]]);
        assert_eq!(data_len, 5u16); // 3 bytes data + 2
                                    // monitoring_timer inserted little-endian at 9..11
        assert_eq!(u16::from_le_bytes([payload[9], payload[10]]), 0x0A0B);
        // data starts at 11 and matches original
        assert_eq!(&payload[11..], &[0x11u8, 0x22u8, 0x33u8]);
    }
}
