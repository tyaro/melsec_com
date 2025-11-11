#![allow(
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::doc_markdown,
    clippy::too_long_first_doc_paragraph
)]

//! melsec_mc
//!
//! melsec_mc は Mitsubishi PLC の MC プロトコル (MC3E / MC4E 相当) の低レベル送受信および
//! 高レベル操作を提供する Rust ライブラリです。
//!
//! 主な機能:
//! - MC3E / MC4E ペイロードの組立・解析
//! - ソケット経由の送受信ラッパ (TCP/UDP)
//! - 高レベルな Read/Write 操作 (`McClient`)
//! - テスト用のモックサーバー (別 crate `melsec_mc_mock`)
//!
//! 使い方の簡単な例:
//! ```no_run
//! // fully-qualified path to avoid relying on crate re-exports in doctests
//! use melsec_mc::mc_client::McClient;
//! let client = McClient::new().with_mc_format(melsec_mc::mc_define::McFrameFormat::MC4E);
//! // client.read_words("D1000", 10).await?;
//! ```
//!
//! crates.io に公開する際は `Cargo.toml` に `repository`, `documentation`, `readme` を適切に設定
//! しておくと、crates.io 上でリポジトリやドキュメントへのリンクが表示されます。

pub mod config;
pub mod device;
pub mod endpoint;
pub mod error;
pub mod error_codes;
pub mod mc_client;
pub mod mc_define;
pub mod mc_frame;
pub mod request;
pub mod response;
pub mod toml_helpers;
pub mod transport;

pub mod command_registry;
pub mod commands;
pub mod device_registry;
pub mod plc_series;

pub use endpoint::ConnectionTarget;
pub use error::MelsecError;

/// Initialize embedded default definitions (commands and error codes) into global registries.
///
/// This is a convenience for applications that want to explicitly initialize the
/// built-in `commands.toml` and `error_codes.toml` into the global registries.
/// If the command registry is already set, that error is ignored; other errors
/// are returned.
///
/// # Errors
///
/// Returns `Err(MelsecError)` when initialization of embedded definitions
/// (commands or error codes) fails for reasons other than the command registry
/// already being set.
pub fn init_defaults() -> Result<(), MelsecError> {
    // Try to set commands; if already set, ignore that specific error.
    match crate::command_registry::CommandRegistry::load_and_set_global_from_src() {
        Ok(()) => {}
        Err(e) => {
            // If it's the "already set" protocol error, ignore.
            let msg = format!("{e}");
            if !msg.contains("global CommandRegistry already set") {
                return Err(e);
            }
        }
    }
    // Register error codes from the embedded `error_codes.toml`. Try to set
    // or merge with any existing registry so initialization is idempotent.
    let reg = crate::error_codes::ErrorRegistry::from_str(include_str!("error_codes.toml"))?;
    reg.register_or_merge()?;
    Ok(())
}

// Note: test-only helpers (previously `test_utils::announce`) removed per cleanup.
