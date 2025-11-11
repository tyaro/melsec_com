use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

use crate::device_map::{DeviceMap, Word};

#[cfg(test)]
mod tests {
    use super::*;
    use melsec_mc::mc_define::{MC_SUBHEADER_REQUEST, MC_SUBHEADER_RESPONSE};

    #[test]
    fn detect_mc4e_by_subheader() {
        // MC4E request subheader at start should result in MC4E detection
        let mut frame: Vec<u8> = Vec::new();
        frame.extend_from_slice(&MC_SUBHEADER_REQUEST);
        // pad to minimal length
        frame.extend_from_slice(&[0u8; 20]);
        let fmt = MockServer::detect_format_from_frame(&frame);
        assert_eq!(fmt, melsec_mc::mc_define::McFrameFormat::MC4E);

        // MC4E response subheader also indicates MC4E
        let mut frame2: Vec<u8> = Vec::new();
        frame2.extend_from_slice(&MC_SUBHEADER_RESPONSE);
        frame2.extend_from_slice(&[0u8; 10]);
        let fmt2 = MockServer::detect_format_from_frame(&frame2);
        assert_eq!(fmt2, melsec_mc::mc_define::McFrameFormat::MC4E);
    }

    #[test]
    fn detect_mc3e_by_access_route_no_subheader() {
        // MC3E no-subheader frame begins with access_route (5 bytes)
        let mut frame: Vec<u8> = Vec::new();
        // access_route default
        frame.extend_from_slice(&melsec_mc::mc_define::AccessRoute::default().to_bytes());
        // data_len = 4 (2 data bytes)
        frame.extend_from_slice(&4u16.to_le_bytes());
        // end_code (2 bytes)
        frame.extend_from_slice(&0u16.to_le_bytes());
        // 2 bytes of data
        frame.extend_from_slice(&[0x11, 0x22]);
        let fmt = MockServer::detect_format_from_frame(&frame);
        assert_eq!(fmt, melsec_mc::mc_define::McFrameFormat::MC3E);
    }
}
// Simple HTTP admin API (minimal, no external HTTP framework) for state injection
use tokio::io::AsyncReadExt;
use tokio::net::UdpSocket;

#[derive(Clone)]
/// Mock server which exposes an in-memory PLC device map over TCP/UDP.
///
/// `MockServer` はユニットテストや統合テスト用に簡易的な PLC ふるまいをエミュレートする
/// サーバ実装です。`DeviceMap` を共有ストアとして TCP と UDP のリスナから共通に参照し、
/// 受信した MC フレームをパースして読み書きを適用します。
///
/// Mock は受信したフレームから MC3E/MC4E を自動判定し、応答も同じフォーマットで返します。
pub struct MockServer {
    pub store: Arc<RwLock<DeviceMap>>,
}

impl Default for MockServer {
    fn default() -> Self {
        Self::new()
    }
}

impl MockServer {
    /// Create a MockServer, optionally populating the device map from a
    /// TOML assignment file when a snapshot is not present. `assignment_path`
    /// may be None to use the built-in default discovery.
    pub fn new_with_assignment(assignment_path: Option<&str>) -> Self {
        // attempt to load persisted device map snapshot if available
        let mut dm = match DeviceMap::load_from_file("./sled_db/device_map_snapshot.json") {
            Ok(Some(m)) => m,
            _ => DeviceMap::new(),
        };
        // if snapshot not present, try to populate from provided assignment file
        if dm.is_empty() {
            if let Some(p) = assignment_path {
                if let Err(e) = dm.populate_from_toml(p) {
                    tracing::warn!(%e, path = %p, "failed to populate device map from provided assignment file");
                }
            } else {
                // try conventional default path relative to workspace root / project layout
                let default_candidates = [
                    "./default_device_assignment.toml",
                    "./melsec_mc_mock/default_device_assignment.toml",
                ];
                for cand in &default_candidates {
                    if std::path::Path::new(cand).exists() {
                        if let Err(e) = dm.populate_from_toml(cand) {
                            tracing::warn!(%e, path = %cand, "failed to populate device map from default candidate");
                        } else {
                            break;
                        }
                    }
                }
            }
        }
        Self {
            store: Arc::new(RwLock::new(dm)),
        }
    }

    pub fn new() -> Self {
        Self::new_with_assignment(None)
    }

    // (old wrapper `build_mc_response_bytes` removed) Use
    // `build_mc_response_from_request` directly when constructing responses.

    /// Build response bytes directly from an outgoing McRequest (the original
    /// request) and response data. This avoids creating a temporary
    /// `McResponse` when the server has a `McRequest` available.
    fn detect_format_from_frame(frame: &[u8]) -> melsec_mc::mc_define::McFrameFormat {
        // Prefer explicit subheader check: if the frame begins with the MC4E
        // request subheader (or MC4E response subheader), treat it as MC4E.
        if frame.len() >= 2 {
            let sub0 = frame[0];
            let sub1 = frame[1];
            if [sub0, sub1] == melsec_mc::mc_define::MC_SUBHEADER_REQUEST
                || [sub0, sub1] == melsec_mc::mc_define::MC_SUBHEADER_RESPONSE
            {
                return melsec_mc::mc_define::McFrameFormat::MC4E;
            }
        }
        // Otherwise fall back to parsing for stronger evidence; if parsing reveals a serial, return MC4E.
        if let Ok(pr) = melsec_mc::mc_frame::parse_frame(frame) {
            if pr.serial_number.is_some() {
                return melsec_mc::mc_define::McFrameFormat::MC4E;
            }
        }
        // Default to MC3E when no MC4E indicators are found.
        melsec_mc::mc_define::McFrameFormat::MC3E
    }

    fn build_mc_response_from_request(
        req: &melsec_mc::request::McRequest,
        resp_data: &[u8],
        format: melsec_mc::mc_define::McFrameFormat,
    ) -> Vec<u8> {
        let mut out: Vec<u8> = Vec::new();
        match format {
            melsec_mc::mc_define::McFrameFormat::MC4E => {
                out.extend_from_slice(&melsec_mc::mc_define::MC_SUBHEADER_RESPONSE);
                out.extend_from_slice(&req.serial_number.to_le_bytes());
                out.extend_from_slice(&0u16.to_le_bytes());
                out.extend_from_slice(&req.access_route.to_bytes());
                let data_len = u16::try_from(resp_data.len() + 2).unwrap_or(2);
                out.extend_from_slice(&data_len.to_le_bytes());
                out.extend_from_slice(&0u16.to_le_bytes());
                out.extend_from_slice(resp_data);
            }
            melsec_mc::mc_define::McFrameFormat::MC3E => {
                out.extend_from_slice(&[0xD0u8, 0x00u8]);
                out.extend_from_slice(&req.access_route.to_bytes());
                let data_len = u16::try_from(resp_data.len() + 2).unwrap_or(2);
                out.extend_from_slice(&data_len.to_le_bytes());
                out.extend_from_slice(&0u16.to_le_bytes());
                out.extend_from_slice(resp_data);
            }
        }
        out
    }

    /// Programmatic helpers for tests and programmatic control
    pub async fn set_words(&self, key: &str, addr: usize, words: &[Word]) {
        let (rk, ra) = crate::device_map::normalize_key_addr(key, addr);
        let mut store = self.store.write().await;
        store.set_words(&rk, ra, words);
    }

    /// Save the current device map to a snapshot file. Intended to be called on shutdown.
    pub async fn save_snapshot(&self, path: &str) -> anyhow::Result<()> {
        let s = self.store.read().await;
        // serialize to bytes while holding the read lock (fast, in-memory)
        let bytes = serde_json::to_vec(&*s)?;
        let path = path.to_string();
        tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
            std::fs::write(&path, &bytes)?;
            Ok(())
        })
        .await??;
        Ok(())
    }

    pub async fn get_words(&self, key: &str, addr: usize, count: usize) -> Vec<Word> {
        let (rk, ra) = crate::device_map::normalize_key_addr(key, addr);
        tracing::debug!(key = %key, addr = addr, rk = %rk, ra = ra, count = count, "mockserver.get_words called");
        let store = self.store.read().await;
        let res = store.get_words(&rk, ra, count);
        tracing::debug!(rk = %rk, ra = ra, result = ?res, "mockserver.get_words result");
        res
    }

    /// Start a TCP listener which accepts MC frames, parses them using the
    /// real `melsec_mc` parsers, performs simple read/write operations against
    /// the in-memory `DeviceMap` and responds with protocol-correct frames.
    pub async fn run_listener(self, bind: &str) -> anyhow::Result<()> {
        tracing::info!(%bind, "mock server binding");
        if melsec_mc::command_registry::CommandRegistry::global().is_none() {
            if let Err(e) =
                melsec_mc::command_registry::CommandRegistry::load_and_set_global_from_src()
            {
                tracing::warn!(%e, "failed to load command registry from src; proceeding without it");
            }
        }
        let listener = tokio::net::TcpListener::bind(bind).await?;
        self.run_listener_on(listener).await
    }

    /// Run the listener accept loop using an already-bound TcpListener.
    pub async fn run_listener_on(self, listener: tokio::net::TcpListener) -> anyhow::Result<()> {
        loop {
            let (socket, peer) = listener.accept().await?;
            let store = self.store.clone();
            tokio::spawn(async move {
                tracing::info!(%peer, "accepted connection");
                // Read buffer for incoming TCP data
                let mut read_buf = vec![0u8; 4096];
                let mut acc: Vec<u8> = Vec::new();
                // determine TIM_AWAIT timeout (milliseconds) from env var
                let tim_await_ms: u64 = std::env::var("MELSEC_MOCK_TIM_AWAIT_MS")
                    .ok()
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(3000);
                // per policy: always send RST on close to avoid TIME_WAIT on the peer side
                // keep the socket in an Option so we can take ownership to set linger if needed
                let mut socket = Some(socket);

                // helper to set SO_LINGER=0 on the underlying socket and close it.
                let close_with_rst = |sock_opt: &mut Option<tokio::net::TcpStream>| {
                    if let Some(s) = sock_opt.take() {
                        match s.into_std() {
                            Ok(std_s) => {
                                let _ = socket2::Socket::from(std_s)
                                    .set_linger(Some(Duration::from_secs(0)));
                            }
                            Err(e) => {
                                tracing::error!(%e, "failed to convert tokio TcpStream to std TcpStream for RST close")
                            }
                        }
                    }
                };
                // whether we've successfully written at least one response to the peer
                let mut _wrote_any = false;

                loop {
                    // read with timeout to implement TIM_AWAIT
                    let read_fut = socket.as_mut().unwrap().read(&mut read_buf);
                    match tokio::time::timeout(Duration::from_millis(tim_await_ms), read_fut).await
                    {
                        Ok(Ok(0)) => {
                            tracing::info!(%peer, "connection closed by peer - forcing RST per policy");
                            // peer closed the connection; force RST to avoid TIME_WAIT
                            close_with_rst(&mut socket);
                            return;
                        }
                        Ok(Ok(n)) => {
                            acc.extend_from_slice(&read_buf[..n]);
                            // try to parse frames from the accumulated buffer
                            loop {
                                match melsec_mc::mc_frame::detect_frame(&acc) {
                                    Ok(Some((frame_len, _header_len, _serial_opt))) => {
                                        if acc.len() < frame_len {
                                            break;
                                        }
                                        let frame = acc.drain(..frame_len).collect::<Vec<u8>>();
                                        tracing::debug!(len = frame.len(), frame = ?frame, "received tcp frame bytes");
                                        match melsec_mc::request::McRequest::try_from_payload(
                                            &frame,
                                        ) {
                                            Ok(mc_req) => {
                                                let resp_data = match crate::handler::handle_request_and_apply_store(&store, &mc_req).await {
                                                    Ok(d) => d,
                                                    Err(e) => { tracing::error!(%e, "request handling failed"); vec![] }
                                                };
                                                let fmt = Self::detect_format_from_frame(&frame);
                                                let out = Self::build_mc_response_from_request(
                                                    &mc_req, &resp_data, fmt,
                                                );
                                                tracing::debug!(resp_len = out.len(), resp = ?out, "sending tcp response bytes");
                                                let out_hex = out
                                                    .iter()
                                                    .map(|b| format!("{:02X}", b))
                                                    .collect::<Vec<_>>()
                                                    .join(" ");
                                                let req_hex = frame
                                                    .iter()
                                                    .map(|b| format!("{:02X}", b))
                                                    .collect::<Vec<_>>()
                                                    .join(" ");
                                                tracing::debug!(req = %req_hex, resp = %out_hex, "mockserver normal-response");
                                                let write_res = socket
                                                    .as_mut()
                                                    .unwrap()
                                                    .writable()
                                                    .await
                                                    .map_err(|e| anyhow::anyhow!(e))
                                                    .and_then(|_| {
                                                        match socket
                                                            .as_mut()
                                                            .unwrap()
                                                            .try_write(&out)
                                                        {
                                                            Ok(_) => Ok(()),
                                                            Err(e) => Err(anyhow::anyhow!(e)),
                                                        }
                                                    });
                                                if write_res.is_ok() {
                                                    _wrote_any = true;
                                                } else if let Err(e) = write_res {
                                                    tracing::error!(%e, "failed to write response to socket");
                                                    // always force RST on write failure
                                                    close_with_rst(&mut socket);
                                                    return;
                                                }
                                            }
                                            Err(e) => {
                                                tracing::error!(%e, "failed to build McRequest from incoming frame");
                                                tracing::debug!(acc_buf = ?acc, frame_len = frame.len(), "acc buffer / frame at parse-failure");
                                                let acc_hex = acc
                                                    .iter()
                                                    .map(|b| format!("{:02X}", b))
                                                    .collect::<Vec<_>>()
                                                    .join(" ");
                                                tracing::debug!(acc = %acc_hex, frame_len = frame.len(), "mockserver parse-failure");
                                                // respond with protocol-appropriate error frame using the subheader
                                                let err_code: u16 = 0x0050;
                                                let subheader = if frame.len() >= 2 {
                                                    [frame[0], frame[1]]
                                                } else {
                                                    [0x50u8, 0x00u8]
                                                };
                                                if subheader
                                                    == melsec_mc::mc_define::MC_SUBHEADER_REQUEST
                                                {
                                                    let serial = if frame.len() >= 4 {
                                                        u16::from_le_bytes([frame[2], frame[3]])
                                                    } else {
                                                        0u16
                                                    };
                                                    let mut out: Vec<u8> = Vec::new();
                                                    out.extend_from_slice(&melsec_mc::mc_define::MC_SUBHEADER_RESPONSE);
                                                    out.extend_from_slice(&serial.to_le_bytes());
                                                    out.extend_from_slice(&0u16.to_le_bytes());
                                                    out.extend_from_slice(&melsec_mc::mc_define::AccessRoute::default().to_bytes());
                                                    out.extend_from_slice(&2u16.to_le_bytes());
                                                    out.extend_from_slice(&err_code.to_le_bytes());
                                                    tracing::debug!(error_out = ?out, "sending parse-error response bytes");
                                                    let out_hex = out
                                                        .iter()
                                                        .map(|b| format!("{:02X}", b))
                                                        .collect::<Vec<_>>()
                                                        .join(" ");
                                                    tracing::debug!(out = %out_hex, "mockserver parse-error response");
                                                    let write_res = socket.as_mut().unwrap().writable().await.map_err(|e| anyhow::anyhow!(e)).and_then(|_| {
                                                        match socket.as_mut().unwrap().try_write(&out) {
                                                            Ok(n) => { tracing::debug!(written = n, "bytes_written for parse-error response"); Ok(()) },
                                                            Err(e) => Err(anyhow::anyhow!(e)),
                                                        }
                                                    });
                                                    if write_res.is_ok() {
                                                        _wrote_any = true;
                                                    }
                                                } else {
                                                    let mut out: Vec<u8> = Vec::new();
                                                    out.extend_from_slice(&[0xD0u8, 0x00u8]);
                                                    out.extend_from_slice(&melsec_mc::mc_define::AccessRoute::default().to_bytes());
                                                    out.extend_from_slice(&2u16.to_le_bytes());
                                                    out.extend_from_slice(&err_code.to_le_bytes());
                                                    tracing::debug!(error_out = ?out, "sending parse-error response bytes (no subheader)");
                                                    let out_hex = out
                                                        .iter()
                                                        .map(|b| format!("{:02X}", b))
                                                        .collect::<Vec<_>>()
                                                        .join(" ");
                                                    tracing::debug!(out = %out_hex, "mockserver parse-error (no-subheader) response");
                                                    let write_res = socket.as_mut().unwrap().writable().await.map_err(|e| anyhow::anyhow!(e)).and_then(|_| {
                                                        match socket.as_mut().unwrap().try_write(&out) {
                                                            Ok(n) => { tracing::debug!(written = n, "bytes_written for parse-error response (no subheader)"); Ok(()) },
                                                            Err(e) => Err(anyhow::anyhow!(e)),
                                                        }
                                                    });
                                                    if write_res.is_ok() {
                                                        _wrote_any = true;
                                                    }
                                                }
                                                // continue to next frame if any
                                                continue;
                                            }
                                        }
                                    }
                                    Ok(None) => break,
                                    Err(e) => {
                                        tracing::error!(%e, "detect_frame error");
                                        tracing::debug!(acc_buf = ?acc, "acc buffer at detect_frame error");
                                        let acc_hex = acc
                                            .iter()
                                            .map(|b| format!("{:02X}", b))
                                            .collect::<Vec<_>>()
                                            .join(" ");
                                        tracing::debug!(acc = %acc_hex, "mockserver detect_frame-error acc");
                                        // guess subheader and send error response
                                        let err_code: u16 = 0x0050;
                                        let mut out: Vec<u8> = Vec::new();
                                        let subheader = if acc.len() >= 2 {
                                            [acc[0], acc[1]]
                                        } else {
                                            [0x50u8, 0x00u8]
                                        };
                                        if subheader == melsec_mc::mc_define::MC_SUBHEADER_REQUEST {
                                            let serial = if acc.len() >= 4 {
                                                u16::from_le_bytes([acc[2], acc[3]])
                                            } else {
                                                0u16
                                            };
                                            out.extend_from_slice(
                                                &melsec_mc::mc_define::MC_SUBHEADER_RESPONSE,
                                            );
                                            out.extend_from_slice(&serial.to_le_bytes());
                                            out.extend_from_slice(&0u16.to_le_bytes());
                                            out.extend_from_slice(
                                                &melsec_mc::mc_define::AccessRoute::default()
                                                    .to_bytes(),
                                            );
                                            out.extend_from_slice(&2u16.to_le_bytes());
                                            out.extend_from_slice(&err_code.to_le_bytes());
                                        } else {
                                            out.extend_from_slice(&[0xD0u8, 0x00u8]);
                                            out.extend_from_slice(
                                                &melsec_mc::mc_define::AccessRoute::default()
                                                    .to_bytes(),
                                            );
                                            out.extend_from_slice(&2u16.to_le_bytes());
                                            out.extend_from_slice(&err_code.to_le_bytes());
                                        }
                                        tracing::debug!(error_out = ?out, "sending detect_frame-error response bytes");
                                        let out_hex = out
                                            .iter()
                                            .map(|b| format!("{:02X}", b))
                                            .collect::<Vec<_>>()
                                            .join(" ");
                                        tracing::debug!(out = %out_hex, "mockserver detect_frame-error out");
                                        let write_res = socket.as_mut().unwrap().writable().await.map_err(|e| anyhow::anyhow!(e)).and_then(|_| {
                                            match socket.as_mut().unwrap().try_write(&out) {
                                                Ok(n) => { tracing::debug!(written = n, "bytes_written for detect_frame-error response"); Ok(()) },
                                                Err(e) => Err(anyhow::anyhow!(e)),
                                            }
                                        });
                                        if write_res.is_ok() {
                                            _wrote_any = true;
                                        }
                                        // force RST on malformed frame handling to simplify peer state
                                        close_with_rst(&mut socket);
                                        return;
                                    }
                                }
                            }
                        }
                        Ok(Err(e)) => {
                            tracing::error!(%e, "read error");
                            // always force RST on read error
                            close_with_rst(&mut socket);
                            return;
                        }
                        Err(_) => {
                            tracing::info!(%peer, "connection idle in TIM_AWAIT for {}ms, forcing RST and closing", tim_await_ms);
                            // Per policy, force RST even on TIM_AWAIT expiry
                            close_with_rst(&mut socket);
                            return;
                        }
                    }
                }
            });
        }
    }

    /// Start a UDP listener which accepts MC frames over UDP, parses them,
    /// dispatches to the same handler as the TCP listener and replies to the
    /// sender address.
    pub async fn run_udp_listener(&self, bind: &str) -> anyhow::Result<()> {
        tracing::info!(%bind, "udp mock server binding");
        // ensure command registry loaded like the TCP listener does
        if melsec_mc::command_registry::CommandRegistry::global().is_none() {
            if let Err(e) =
                melsec_mc::command_registry::CommandRegistry::load_and_set_global_from_src()
            {
                tracing::warn!(%e, "failed to load command registry from src; proceeding without it");
            }
        }

        let socket = UdpSocket::bind(bind).await?;
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let (n, peer) = match socket.recv_from(&mut buf).await {
                Ok((n, p)) => (n, p),
                Err(e) => {
                    tracing::error!(%e, "udp recv_from failed");
                    continue;
                }
            };
            let frame = buf[..n].to_vec();
            tracing::debug!(udp_len = n, udp_frame = ?frame, peer = %peer, "received udp frame bytes");
            // Construct McRequest from incoming UDP frame and dispatch
            let mc_req = match melsec_mc::request::McRequest::try_from_payload(&frame) {
                Ok(r) => r,
                Err(e) => {
                    tracing::error!(%e, "failed to build McRequest from incoming frame (udp)");
                    continue;
                }
            };
            let resp_data =
                match crate::handler::handle_request_and_apply_store(&self.store, &mc_req).await {
                    Ok(d) => d,
                    Err(e) => {
                        tracing::error!(%e, "request handling failed (udp)");
                        vec![]
                    }
                };
            let fmt = Self::detect_format_from_frame(&frame);
            let out = Self::build_mc_response_from_request(&mc_req, &resp_data, fmt);
            tracing::debug!(resp_len = out.len(), resp = ?out, peer = %peer, "sending udp response bytes");
            if let Err(e) = socket.send_to(&out, &peer).await.map(|_| ()) {
                tracing::error!(%e, "failed to send udp response");
            }
        }
    }
}
