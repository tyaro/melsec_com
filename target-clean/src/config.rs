use once_cell::sync::Lazy;

#[derive(Debug)]
pub struct Config {
    pub melsec_conn_idle_secs: u64,
    pub melsec_udp_recv_attempts: usize,
    pub melsec_dump_on_error: bool,
    pub log_mc_payloads: bool,
    pub melsec_tcp_retry_attempts: usize,
    pub melsec_tcp_retry_backoff_ms: u64,
}

impl Config {
    fn from_env() -> Self {
        let melsec_conn_idle_secs = std::env::var("MELSEC_CONN_IDLE_SECS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(300u64);
        let melsec_udp_recv_attempts = std::env::var("MELSEC_UDP_RECV_ATTEMPTS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(3usize);
        let melsec_dump_on_error = std::env::var("MELSEC_DUMP_ON_ERROR")
            .map(|v| v == "1")
            .unwrap_or(false);
        let log_mc_payloads = std::env::var("LOG_MC_PAYLOADS")
            .map(|v| v == "1")
            .unwrap_or(false);
        let melsec_tcp_retry_attempts = std::env::var("MELSEC_TCP_RETRY_ATTEMPTS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(3usize);
        let melsec_tcp_retry_backoff_ms = std::env::var("MELSEC_TCP_RETRY_BACKOFF_MS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(100u64);
        Self {
            melsec_conn_idle_secs,
            melsec_udp_recv_attempts,
            melsec_dump_on_error,
            log_mc_payloads,
            melsec_tcp_retry_attempts,
            melsec_tcp_retry_backoff_ms,
        }
    }
}

/// Global config loaded once from environment at first access.
pub static GLOBAL_CONFIG: Lazy<Config> = Lazy::new(Config::from_env);

/// Convenience accessor
pub fn config() -> &'static Config {
    &GLOBAL_CONFIG
}
