use serde_json::Value as JsonValue;

use crate::command_registry::{
    create_read_bits_params, create_read_words_params, create_write_bits_params,
    create_write_words_params, CommandRegistry, GLOBAL_COMMAND_REGISTRY,
};
use crate::commands::Command;
use crate::config::config as global_config;
use crate::endpoint::ConnectionTarget;
use crate::error::MelsecError;
use crate::mc_define::Protocol;
use crate::plc_series::PLCSeries;
use crate::request::McRequest;
use crate::response::McResponse;

use std::time::Duration;
use tokio::time::sleep;

/// Trait for types that can be decoded from a slice of u16 words.
///
/// The implementation returns the decoded value and how many words were
/// consumed from the input slice.
pub trait FromWords: Sized {
    /// Number of u16 words consumed by one value of this type.
    const WORDS: usize;

    fn from_words_slice(words: &[u16]) -> Result<(Self, usize), MelsecError>;
}

/// Trait for types that can be encoded into u16 words for writing.
pub trait ToWords {
    /// Number of u16 words consumed/produced by one value of this type.
    const WORDS: usize;

    /// Encode this value into the provided vector as u16 words (low-first).
    fn to_words(&self, out: &mut Vec<u16>);
}

impl FromWords for u16 {
    const WORDS: usize = 1;
    fn from_words_slice(words: &[u16]) -> Result<(Self, usize), MelsecError> {
        if words.is_empty() {
            return Err(MelsecError::Protocol("not enough words for u16".into()));
        }
        Ok((words[0], 1))
    }
}

impl FromWords for i16 {
    const WORDS: usize = 1;
    fn from_words_slice(words: &[u16]) -> Result<(Self, usize), MelsecError> {
        if words.is_empty() {
            return Err(MelsecError::Protocol("not enough words for i16".into()));
        }
        let w = words[0];
        let i = i16::from_le_bytes(w.to_le_bytes());
        Ok((i, 1))
    }
}

impl FromWords for [bool; 16] {
    const WORDS: usize = 1;
    fn from_words_slice(words: &[u16]) -> Result<(Self, usize), MelsecError> {
        if words.is_empty() {
            return Err(MelsecError::Protocol("not enough words for bits16".into()));
        }
        let w = words[0];
        let mut bits = [false; 16];
        for (i, b) in bits.iter_mut().enumerate() {
            *b = ((w >> i) & 0x01) != 0;
        }
        Ok((bits, 1))
    }
}

impl FromWords for u32 {
    const WORDS: usize = 2;
    fn from_words_slice(words: &[u16]) -> Result<(Self, usize), MelsecError> {
        if words.len() < 2 {
            return Err(MelsecError::Protocol("not enough words for u32".into()));
        }
        let low = words[0] as u32;
        let high = words[1] as u32;
        let v = (high << 16) | low;
        Ok((v, 2))
    }
}

impl FromWords for i32 {
    const WORDS: usize = 2;
    fn from_words_slice(words: &[u16]) -> Result<(Self, usize), MelsecError> {
        if words.len() < 2 {
            return Err(MelsecError::Protocol("not enough words for i32".into()));
        }
        let low = words[0] as u32;
        let high = words[1] as u32;
        let v = (high << 16) | low;
        let iv = i32::from_le_bytes(v.to_le_bytes());
        Ok((iv, 2))
    }
}

impl FromWords for f32 {
    const WORDS: usize = 2;
    fn from_words_slice(words: &[u16]) -> Result<(Self, usize), MelsecError> {
        if words.len() < 2 {
            return Err(MelsecError::Protocol("not enough words for f32".into()));
        }
        let low = words[0] as u32;
        let high = words[1] as u32;
        let v = (high << 16) | low;
        let fv = f32::from_bits(v);
        Ok((fv, 2))
    }
}

impl ToWords for u16 {
    const WORDS: usize = 1;
    fn to_words(&self, out: &mut Vec<u16>) {
        out.push(*self);
    }
}

impl ToWords for i16 {
    const WORDS: usize = 1;
    fn to_words(&self, out: &mut Vec<u16>) {
        let bytes = self.to_le_bytes();
        let w = u16::from_le_bytes(bytes);
        out.push(w);
    }
}

impl ToWords for [bool; 16] {
    const WORDS: usize = 1;
    fn to_words(&self, out: &mut Vec<u16>) {
        let mut w: u16 = 0;
        for (i, v) in self.iter().enumerate() {
            if *v {
                w |= 1u16 << i;
            }
        }
        out.push(w);
    }
}

impl ToWords for u32 {
    const WORDS: usize = 2;
    fn to_words(&self, out: &mut Vec<u16>) {
        let v = *self;
        let low = (v & 0xFFFF) as u16;
        let high = ((v >> 16) & 0xFFFF) as u16;
        out.push(low);
        out.push(high);
    }
}

impl ToWords for i32 {
    const WORDS: usize = 2;
    fn to_words(&self, out: &mut Vec<u16>) {
        let v = *self as u32;
        let low = (v & 0xFFFF) as u16;
        let high = ((v >> 16) & 0xFFFF) as u16;
        out.push(low);
        out.push(high);
    }
}

impl ToWords for f32 {
    const WORDS: usize = 2;
    fn to_words(&self, out: &mut Vec<u16>) {
        let v = self.to_bits();
        let low = (v & 0xFFFF) as u16;
        let high = ((v >> 16) & 0xFFFF) as u16;
        out.push(low);
        out.push(high);
    }
}

fn maybe_log_payload(label: &str, payload: &[u8]) {
    if global_config().log_mc_payloads {
        let hex = payload
            .iter()
            .map(|b| format!("{b:02X}"))
            .collect::<Vec<_>>()
            .join(" ");
        log::debug!("[MC PAYLOAD {}] {}", label, hex);
    }
}

const DEFAULT_MONITORING_TIMER: u16 = 5; // 0 means "no monitoring"
const DEFAULT_CLIENT_NAME: &str = "client";

/// High-level client for interacting with Mitsubishi PLCs using MC3E/MC4E frames.
///
/// McClient は接続先情報 (`ConnectionTarget`) とプロトコル設定を保持し、
/// 高レベルの read/write 操作を提供します。内部では request ビルダーと
/// 低レイヤ送受信 (`transport`) を使ってフレームを組み立て/解析します。
///
/// エラーは `MelsecError` を返します。ネットワーク上の実機テストは環境依存のため
/// CI ではオプトアウト（環境変数で制御）することを推奨します。
pub struct McClient {
    pub target: ConnectionTarget,
    pub plc_series: PLCSeries,
    pub protocol: Protocol,
    pub monitoring_timer: u16,
    pub client_name: String,
    pub mc_format: crate::mc_define::McFrameFormat,
}
impl McClient {
    /// Create a new `McClient` with full options.
    #[must_use]
    pub fn new() -> Self {
        // On first client creation, attempt to set embedded definitions as global defaults.
        // Ignore errors (registry may already be set by the application or tests).
        let _ = CommandRegistry::load_and_set_global_from_src();
        let _ = crate::error_codes::ErrorRegistry::from_str(include_str!("error_codes.toml"))
            .and_then(|r| r.register_or_merge());
        Self {
            target: ConnectionTarget::new(),
            plc_series: PLCSeries::R,
            protocol: Protocol::Tcp,
            monitoring_timer: DEFAULT_MONITORING_TIMER,
            client_name: DEFAULT_CLIENT_NAME.to_string(),
            mc_format: crate::mc_define::McFrameFormat::default(),
        }
    }
    /// Create a client that will use the specified MC frame format (MC4E/MC3E).
    ///
    #[must_use]
    pub fn with_mc_format(mut self, fmt: crate::mc_define::McFrameFormat) -> Self {
        self.mc_format = fmt;
        self
    }
    #[must_use]
    pub fn with_target(mut self, target: ConnectionTarget) -> Self {
        self.target = target;
        self
    }
    #[must_use]
    pub const fn with_plc_series(mut self, series: PLCSeries) -> Self {
        self.plc_series = series;
        self
    }
    #[must_use]
    pub const fn with_protocol(mut self, protocol: Protocol) -> Self {
        self.protocol = protocol;
        self
    }
    #[must_use]
    pub const fn with_monitoring_timer(mut self, timer: u16) -> Self {
        self.monitoring_timer = timer;
        self
    }
    #[must_use]
    pub fn with_client_name(mut self, name: impl Into<String>) -> Self {
        self.client_name = name.into();
        self
    }

    // Common helper to check MC response end-code and convert to MelsecError when non-zero.
    // This is an associated function because it does not need `self`.
    fn check_response_end_code(response: &McResponse) -> Result<(), MelsecError> {
        if response.has_end_code {
            if let Some(code) = response.end_code {
                if code != 0 {
                    let name = crate::error_codes::code_name(code).unwrap_or_default();
                    let desc = crate::error_codes::code_description(code).unwrap_or_default();
                    let mut extra = String::new();
                    if !name.is_empty() && !desc.is_empty() {
                        extra = format!(" ({name}: {desc})");
                    } else if !name.is_empty() {
                        extra = format!(" ({name})");
                    } else if !desc.is_empty() {
                        extra = format!(" ({desc})");
                    }
                    return Err(MelsecError::Protocol(format!(
                        "device end code: 0x{code:04X}{extra}"
                    )));
                }
            }
        }
        Ok(())
    }

    // read_words コマンドを実行して応答伝文から生データを取得する
    pub async fn read_words(&self, device: &str, count: u16) -> Result<JsonValue, MelsecError> {
        let params = create_read_words_params(device, count);

        // コマンド仕様を取得してリクエスト伝文を構築
        let reg = GLOBAL_COMMAND_REGISTRY
            .get()
            .ok_or_else(|| MelsecError::Protocol("global registry not set".into()))?;
        let spec = reg
            .get(Command::ReadWords)
            .ok_or_else(|| MelsecError::Protocol("command not found".into()))?;

        let request_data = spec.build_request(&params, Some(self.plc_series))?;

        // McRequest構築
        let mc_req = McRequest::new()
            .with_access_route(self.target.access_route)
            .try_with_request_data(request_data)?;
        let mc_payload = mc_req.build_with_format(self.mc_format);

        maybe_log_payload("read_words", &mc_payload);

        // Send/receive using selected transport (TCP/UDP)
        let timeout = if self.monitoring_timer > 0 {
            Some(Duration::from_secs(u64::from(self.monitoring_timer)))
        } else {
            None
        };
        let buf = self.send_and_recv_with_retry(&mc_payload, timeout).await?;
        // parse response
        let response: McResponse = McResponse::try_new(&buf)?;
        // centralized end-code check
        Self::check_response_end_code(&response)?;

        let response_data = spec.parse_response(&params, &response.data)?;
        Ok(response_data)
    }

    pub async fn read_words_as<T: FromWords>(
        &self,
        device: &str,
        count: u16,
    ) -> Result<Vec<T>, MelsecError> {
        // interpret `count` as number of T elements; compute required word count
        let required_words = match (T::WORDS as u32).checked_mul(u32::from(count)) {
            Some(v) => {
                if v > u32::from(u16::MAX) {
                    return Err(MelsecError::Protocol("requested count too large".into()));
                }
                v as u16
            }
            None => return Err(MelsecError::Protocol("requested count overflow".into())),
        };
        let params = create_read_words_params(device, required_words);

        let reg = GLOBAL_COMMAND_REGISTRY
            .get()
            .ok_or_else(|| MelsecError::Protocol("global registry not set".into()))?;
        let spec = reg
            .get(Command::ReadWords)
            .ok_or_else(|| MelsecError::Protocol("command not found".into()))?;

        let request_data = spec.build_request(&params, Some(self.plc_series))?;
        let mc_req = McRequest::new()
            .with_access_route(self.target.access_route)
            .try_with_request_data(request_data)?;
        let mc_payload = mc_req.build_with_format(self.mc_format);

        maybe_log_payload("read_words", &mc_payload);

        let timeout = if self.monitoring_timer > 0 {
            Some(Duration::from_secs(u64::from(self.monitoring_timer)))
        } else {
            None
        };
        let buf = self.send_and_recv_with_retry(&mc_payload, timeout).await?;
        let response: McResponse = McResponse::try_new(&buf)?;
        Self::check_response_end_code(&response)?;

        let parsed = spec.parse_response(&params, &response.data)?;

        // collect words from data_blocks (flatten blocks)
        let mut words: Vec<u16> = Vec::new();
        if let Some(db) = parsed.get("data_blocks").and_then(|v| v.as_array()) {
            for block in db {
                if let Some(arr) = block.as_array() {
                    for it in arr {
                        if let Some(n) = it.as_u64() {
                            let w = u16::try_from(n).map_err(|_| {
                                MelsecError::Protocol("word value out of range for u16".into())
                            })?;
                            words.push(w);
                        }
                    }
                }
            }
        }

        let mut out: Vec<T> = Vec::new();
        let mut idx = 0usize;
        let mut parsed = 0u32;
        // Attempt to parse up to `count` elements. On per-element parse errors,
        // record a warning and advance by one word to try to resynchronize.
        while parsed < u32::from(count) && idx + T::WORDS <= words.len() {
            match T::from_words_slice(&words[idx..]) {
                Ok((val, used)) => {
                    out.push(val);
                    idx += used;
                    parsed += 1;
                }
                Err(e) => {
                    log::warn!(
                        "parse error for {} at word index {}: {}",
                        std::any::type_name::<T>(),
                        idx,
                        e
                    );
                    // advance by one word and try to resync
                    idx += 1;
                }
            }
        }
        Ok(out)
    }

    // Note: `read_words_as_partial` removed — its tolerant behavior is now integrated
    // into `read_words_as` above. This keeps the public API surface stable while
    // returning successfully parsed elements even when some alignments fail.

    pub async fn read_bits(&self, device: &str, count: u16) -> Result<JsonValue, MelsecError> {
        let params = create_read_bits_params(device, count);
        let reg = GLOBAL_COMMAND_REGISTRY
            .get()
            .ok_or_else(|| MelsecError::Protocol("global registry not set".into()))?;
        let spec = reg
            .get(Command::ReadBits)
            .ok_or_else(|| MelsecError::Protocol("command not found".into()))?;

        let request_data = spec.build_request(&params, Some(self.plc_series))?;
        let mc_req = McRequest::new()
            .with_access_route(self.target.access_route)
            .try_with_request_data(request_data)?;
        let mc_payload = mc_req.build_with_format(self.mc_format);

        maybe_log_payload("read_bits", &mc_payload);

        let timeout = if self.monitoring_timer > 0 {
            Some(Duration::from_secs(u64::from(self.monitoring_timer)))
        } else {
            None
        };
        let buf = self.send_and_recv_with_retry(&mc_payload, timeout).await?;
        let response: McResponse = McResponse::try_new(&buf)?;
        Self::check_response_end_code(&response)?;
        let response_data = spec.parse_response(&params, &response.data)?;
        Ok(response_data)
    }

    pub async fn write_words(
        &self,
        device: &str,
        values: &[u16],
    ) -> Result<JsonValue, MelsecError> {
        let request_params = create_write_words_params(device, values);
        let reg = GLOBAL_COMMAND_REGISTRY
            .get()
            .ok_or_else(|| MelsecError::Protocol("global registry not set".into()))?;
        let spec = reg
            .get(Command::WriteWords)
            .ok_or_else(|| MelsecError::Protocol("command not found".into()))?;

        let request_data = spec.build_request(&request_params, Some(self.plc_series))?;
        let mc_req = McRequest::new()
            .with_access_route(self.target.access_route)
            .try_with_request_data(request_data)?;
        let mc_payload = mc_req.build_with_format(self.mc_format);

        maybe_log_payload("write_words", &mc_payload);

        let timeout = if self.monitoring_timer > 0 {
            Some(Duration::from_secs(u64::from(self.monitoring_timer)))
        } else {
            None
        };
        let buf = self.send_and_recv_with_retry(&mc_payload, timeout).await?;
        let response: McResponse = McResponse::try_new(&buf)?;
        Self::check_response_end_code(&response)?;
        let response_data = spec.parse_response(&request_params, &response.data)?;
        Ok(response_data)
    }

    /// Write an array of typed elements by encoding them into u16 words and
    /// issuing a write_words command. The device argument is the start device
    /// (e.g. "D1010") and values are written sequentially.
    pub async fn write_words_as<T: ToWords>(
        &self,
        device: &str,
        values: &[T],
    ) -> Result<JsonValue, MelsecError> {
        // Flatten values into words
        let mut flat: Vec<u16> = Vec::with_capacity(values.len() * T::WORDS);
        for v in values {
            v.to_words(&mut flat);
        }
        // Delegate to existing write_words which builds the request from u16 slice
        self.write_words(device, &flat).await
    }

    pub async fn write_bits(
        &self,
        device: &str,
        values: &[bool],
    ) -> Result<JsonValue, MelsecError> {
        let request_params = create_write_bits_params(device, values);
        let reg = GLOBAL_COMMAND_REGISTRY
            .get()
            .ok_or_else(|| MelsecError::Protocol("global registry not set".into()))?;
        let spec = reg
            .get(Command::WriteBits)
            .ok_or_else(|| MelsecError::Protocol("command not found".into()))?;

        let request_data = spec.build_request(&request_params, Some(self.plc_series))?;
        let mc_req = McRequest::new()
            .with_access_route(self.target.access_route)
            .try_with_request_data(request_data)?;
        let mc_payload = mc_req.build_with_format(self.mc_format);

        maybe_log_payload("write_bits", &mc_payload);

        let timeout = if self.monitoring_timer > 0 {
            Some(Duration::from_secs(u64::from(self.monitoring_timer)))
        } else {
            None
        };
        let buf = self.send_and_recv_with_retry(&mc_payload, timeout).await?;
        let response: McResponse = McResponse::try_new(&buf)?;
        Self::check_response_end_code(&response)?;
        let response_data = spec.parse_response(&request_params, &response.data)?;
        Ok(response_data)
    }

    /// Echo test: send arbitrary ASCII hex string payload (0-9, A-F/a-f) and return echoed string.
    ///
    /// The request format is: command(2be)=0x0619, subcommand(2be)=0x0000, payload:ascii_hex
    /// Returns the echoed payload as a UTF-8 string on success.
    pub async fn echo(&self, payload: &str) -> Result<String, MelsecError> {
        let bytes = payload.as_bytes();
        let len = bytes.len();
        if !(1..=960).contains(&len) {
            return Err(MelsecError::Protocol(format!(
                "echo payload length out of range: {}",
                len
            )));
        }
        // validate allowed characters: 0-9, A-F, a-f
        if !bytes.iter().all(|&b| {
            b.is_ascii_digit() || (b'A'..=b'F').contains(&b) || (b'a'..=b'f').contains(&b)
        }) {
            return Err(MelsecError::Protocol(
                "echo payload contains invalid characters; allowed: 0-9 A-F".into(),
            ));
        }

        // build request payload: command(2be) + subcommand(2be) + payload
        let mut request_data: Vec<u8> = Vec::with_capacity(4 + len);
        // command/subcommand are encoded little-endian per command registry semantics
        request_data.extend_from_slice(&0x0619u16.to_le_bytes());
        request_data.extend_from_slice(&0x0000u16.to_le_bytes());
        request_data.extend_from_slice(bytes);

        let mc_req = McRequest::new()
            .with_access_route(self.target.access_route)
            .try_with_request_data(request_data)?;
        let mc_payload = mc_req.build_with_format(self.mc_format);

        maybe_log_payload("echo", &mc_payload);

        let timeout = if self.monitoring_timer > 0 {
            Some(Duration::from_secs(u64::from(self.monitoring_timer)))
        } else {
            None
        };
        let buf = self.send_and_recv_with_retry(&mc_payload, timeout).await?;
        let response: McResponse = McResponse::try_new(&buf)?;
        Self::check_response_end_code(&response)?;

        // response.data should contain the echoed bytes; convert to string
        match String::from_utf8(response.data) {
            Ok(s) => Ok(s),
            Err(_) => Err(MelsecError::Protocol(
                "echo response not valid UTF-8".into(),
            )),
        }
    }

    // write_nibbles was removed: use `write_bits` (booleans) or
    // `create_write_4bit_params_high_first` + low-level build_request/send if you need
    // to craft raw 4-bit payloads. The high-level `write_bits` API handles boolean
    // points and packs them into 4-bit nibbles as required by common PLC variants.

    /// Send payload using configured protocol with automatic retries and exponential backoff for TCP.
    async fn send_and_recv_with_retry(
        &self,
        mc_payload: &[u8],
        timeout: Option<Duration>,
    ) -> Result<Vec<u8>, MelsecError> {
        // Configurable parameters via env vars
        // MELSEC_TCP_RETRY_ATTEMPTS (default 3)
        // MELSEC_TCP_RETRY_BACKOFF_MS (base backoff in ms, default 100)
        let attempts: usize = global_config().melsec_tcp_retry_attempts;
        let base_backoff_ms: u64 = global_config().melsec_tcp_retry_backoff_ms;

        let mut last_err: Option<MelsecError> = None;
        for attempt in 1..=attempts {
            let res = match self.protocol {
                Protocol::Tcp => {
                    crate::transport::send_and_recv_tcp(&self.target.addr, mc_payload, timeout)
                        .await
                }
                Protocol::Udp => {
                    crate::transport::send_and_recv_udp(&self.target.addr, mc_payload, timeout)
                        .await
                }
            };
            match res {
                Ok(buf) => return Ok(buf),
                Err(e) => {
                    last_err = Some(e);
                    // log retryable error when debug enabled
                    let dump = global_config().melsec_dump_on_error;
                    if dump {
                        if let Some(ref err) = last_err {
                            log::warn!(
                                "[MC RETRY] attempt {}/{} failed for {} proto={:?}, error={}",
                                attempt,
                                attempts,
                                self.target.addr,
                                self.protocol,
                                err
                            );
                        }
                    }
                    // if last attempt, break and return error
                    if attempt == attempts {
                        break;
                    }
                    // exponential backoff
                    // cap the exponent to avoid unbounded shifts and avoid usize->u32 truncation
                    let exp = u32::try_from(attempt.saturating_sub(1).min(63)).unwrap_or(63);
                    let backoff = base_backoff_ms.saturating_mul(2u64.pow(exp));
                    sleep(Duration::from_millis(backoff)).await;
                }
            }
        }
        Err(last_err.unwrap_or_else(|| MelsecError::Protocol("unknown transport error".into())))
    }
}
impl Default for McClient {
    fn default() -> Self {
        Self::new()
    }
}
