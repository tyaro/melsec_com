//! MC 終了コード（エラーコード）一覧とカテゴリ判定ユーティリティ
//!
//! 変更点: 終了コードの説明やカテゴリ判定を外部 TOML から読み込めるようにし、
//! 実行時に `ErrorRegistry::from_str(...).register_or_merge()` 等で登録できるようにしました。
//! グローバル登録がない場合は既存のハードコードされたフォールバックを
//! 使用します。

use once_cell::sync::OnceCell;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::RwLock;

use crate::error::MelsecError;

/// 終了コードのカテゴリ（簡易分類）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum ErrorCategory {
    Success,
    Addressing,
    DataFormat,
    ExecutionMode,
    BufferRange,
    Network,
    Transport,
    Icmp,
    Unknown,
}

#[derive(Debug, Deserialize)]
struct ErrorCodeEntry {
    // TOML では16進表記を文字列で書くことが多いので文字列/数値の両方を受け取れるようにする
    #[serde(deserialize_with = "parse_hex_or_int")]
    code: u16,
    name: Option<String>,
    description: Option<String>,
    category: Option<ErrorCategory>,
}

// serde 用ヘルパ: 整数または "0x...." 形式の文字列を u16 に変換する
fn parse_hex_or_int<'de, D>(deserializer: D) -> Result<u16, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct V;
    impl serde::de::Visitor<'_> for V {
        type Value = u16;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "hex string like 0xNNNN or integer")
        }
        fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            u16::try_from(v).map_err(|_| E::custom(format!("value out of range: {v}")))
        }
        fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            if v < 0 {
                return Err(E::custom(format!("negative value: {v}")));
            }
            let uv = u64::try_from(v).map_err(|_| E::custom(format!("value out of range: {v}")))?;
            u16::try_from(uv).map_err(|_| E::custom(format!("value out of range: {v}")))
        }
        fn visit_str<E>(self, s: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            let s = s.trim();
            s.strip_prefix("0x").map_or_else(
                || {
                    s.parse::<u16>()
                        .map_err(|e| E::custom(format!("parse int: {e}")))
                },
                |s| u16::from_str_radix(s, 16).map_err(|e| E::custom(format!("parse hex: {e}"))),
            )
        }
    }
    deserializer.deserialize_any(V)
}

#[derive(Debug, Deserialize)]
struct ErrorCodesToml {
    #[serde(default)]
    codes: Vec<ErrorCodeEntry>,
}

#[derive(Clone, Debug)]
pub struct ErrorEntryOwned {
    pub code: u16,
    pub name: Option<String>,
    pub description: Option<String>,
    pub category: Option<ErrorCategory>,
}

/// In-memory handle for a parsed error-codes TOML. Mirrors DeviceRegistry/CommandRegistry style.
pub struct ErrorRegistry {
    codes: Vec<ErrorCodeEntry>,
}

impl ErrorRegistry {
    /// Parse a TOML string into an `ErrorRegistry`
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Result<Self, MelsecError> {
        let parsed: ErrorCodesToml = toml::from_str(s).map_err(|e| {
            let s = e.to_string();
            MelsecError::Protocol(format!("error_codes.toml parse error: {s}"))
        })?;
        Ok(Self {
            codes: parsed.codes,
        })
    }

    pub fn from_path(path: &Path) -> Result<Self, MelsecError> {
        let s = fs::read_to_string(path)
            .map_err(|e| MelsecError::Protocol(format!("read error_codes.toml: {e}")))?;
        Self::from_str(&s)
    }

    pub fn from_exe_define() -> Result<Self, MelsecError> {
        let exe = std::env::current_exe()
            .map_err(|_| MelsecError::Protocol("current exe not found".into()))?;
        let p = exe
            .parent()
            .map(|p| p.join("define").join("error_codes.toml"));
        if let Some(p) = p {
            if p.exists() {
                return Self::from_path(&p);
            }
        }
        Err(MelsecError::Protocol(
            "define/error_codes.toml not found next to exe".into(),
        ))
    }

    /// Validate TOML string for basic correctness (called without registering)
    pub fn validate_str(s: &str) -> Result<(), MelsecError> {
        let _ = toml::from_str::<ErrorCodesToml>(s).map_err(|e| {
            let s = e.to_string();
            MelsecError::Protocol(format!("error_codes.toml parse error: {s}"))
        })?;
        Ok(())
    }

    /// Register parsed codes into the global registry (in-memory map). Fails if already set.
    pub fn register_codes(&self) -> Result<(), MelsecError> {
        // Attempt to initialize the global registry exactly once. If another thread
        // already initialized it, return AlreadyRegistered so callers can decide how
        // to handle the situation. This avoids silently overwriting an existing
        // registry which could hide races.
        let mut map = HashMap::new();
        for e in &self.codes {
            map.insert(
                e.code,
                ErrorEntryOwned {
                    code: e.code,
                    name: e.name.clone(),
                    description: e.description.clone(),
                    category: e.category,
                },
            );
        }
        let rw = RwLock::new(map);
        match ERROR_REGISTRY.set(rw) {
            Ok(()) => Ok(()),
            Err(_existing) => Err(MelsecError::AlreadyRegistered),
        }
    }

    /// Register parsed codes into the global registry, but if the registry is
    /// already set, merge the new entries into the existing map. This is
    /// convenient when callers want to add additional codes without overwriting
    /// the whole registry.
    pub fn register_or_merge(&self) -> Result<(), MelsecError> {
        let mut map = HashMap::new();
        for e in &self.codes {
            map.insert(
                e.code,
                ErrorEntryOwned {
                    code: e.code,
                    name: e.name.clone(),
                    description: e.description.clone(),
                    category: e.category,
                },
            );
        }
        let rw = RwLock::new(map);
        match ERROR_REGISTRY.set(rw) {
            Ok(()) => Ok(()),
            Err(_existing) => {
                // Merge into existing registry
                if let Some(cell) = ERROR_REGISTRY.get() {
                    let mut w = cell
                        .write()
                        .map_err(|_| MelsecError::Protocol("error registry poisoned".into()))?;
                    for e in &self.codes {
                        w.insert(
                            e.code,
                            ErrorEntryOwned {
                                code: e.code,
                                name: e.name.clone(),
                                description: e.description.clone(),
                                category: e.category,
                            },
                        );
                    }
                    Ok(())
                } else {
                    Err(MelsecError::Protocol(
                        "error registry inconsistent state".into(),
                    ))
                }
            }
        }
    }

    /// Convenience: parse from path and register
    pub fn register_codes_from_path(path: &Path) -> Result<(), MelsecError> {
        let reg = Self::from_path(path)?;
        reg.register_codes()
    }

    /// Convenience: parse exe define and register
    pub fn register_codes_from_exe_define() -> Result<(), MelsecError> {
        let reg = Self::from_exe_define()?;
        reg.register_codes()
    }

    /// Convenience: validate exe define file
    pub fn validate_exe_define() -> Result<(), MelsecError> {
        let exe = std::env::current_exe()
            .map_err(|_| MelsecError::Protocol("current exe not found".into()))?;
        let p = exe
            .parent()
            .map(|p| p.join("define").join("error_codes.toml"));
        if let Some(p) = p {
            if p.exists() {
                let s = fs::read_to_string(&p)
                    .map_err(|e| MelsecError::Protocol(format!("read error_codes.toml: {e}")))?;
                return Self::validate_str(&s);
            }
        }
        Err(MelsecError::Protocol(
            "define/error_codes.toml not found next to exe".into(),
        ))
    }
}

impl std::str::FromStr for ErrorRegistry {
    type Err = MelsecError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Explicitly dispatch to the inherent implementation to avoid
        // ambiguity with the trait method and possible recursion.
        ErrorRegistry::from_str(s)
    }
}

static ERROR_REGISTRY: OnceCell<RwLock<HashMap<u16, ErrorEntryOwned>>> = OnceCell::new();

fn get_global_registry(
) -> Option<std::sync::RwLockReadGuard<'static, HashMap<u16, ErrorEntryOwned>>> {
    ERROR_REGISTRY.get().and_then(|rw| rw.read().ok())
}

// Note: backward-compatible top-level convenience wrappers were removed in
// favor of the `ErrorRegistry` type which provides parsing and registration
// helpers. Use `ErrorRegistry::from_str(...)?; reg.register_codes()?` or the
// inherent convenience methods `register_codes_from_path` /
// `register_codes_from_exe_define` when appropriate.

/// コード説明を返す。登録があればそちらを優先し、なければ既存のハードコード版へフォールバックする。
#[must_use]
pub fn code_description(code: u16) -> Option<String> {
    get_global_registry().and_then(|map| map.get(&code).and_then(|e| e.description.clone()))
}

/// Return registered error code name (e.g. "MC_ERR_C061") if available
#[must_use]
pub fn code_name(code: u16) -> Option<String> {
    get_global_registry().and_then(|map| map.get(&code).and_then(|e| e.name.clone()))
}

/// カテゴリ判定: 登録があればそれを優先、無ければ従来の範囲判定へフォールバック
#[must_use]
pub fn code_category(code: u16) -> ErrorCategory {
    get_global_registry()
        .and_then(|map| map.get(&code).and_then(|e| e.category))
        .unwrap_or(ErrorCategory::Unknown)
}

/// 簡易判定ヘルパー
#[must_use]
pub const fn is_network_error(code: u16) -> bool {
    matches!(code, 0xC000..=0xC0FF)
}

#[must_use]
pub fn is_buffer_error(code: u16) -> bool {
    (0x00A0..=0xFFFF).contains(&code)
}

#[must_use]
pub const fn is_transport_error(code: u16) -> bool {
    matches!(code, 0xC030..=0xC04F)
}

#[must_use]
pub const fn is_icmp_error(code: u16) -> bool {
    matches!(code, 0xC044..=0xC048)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_known_codes() {
        // register sample error codes
        // register a superset of codes used across module tests so repeated registration
        // from other tests will be a no-op but the codes we expect are present.
        let toml = r#"
[[codes]]
code = "0x0000"
description = "正常完了"
category = "Success"

[[codes]]
code = "0x0002"
description = "読出し/書込み対象範囲の指定に誤りがある"
category = "Addressing"

[[codes]]
code = "0x0054"
description = "Ethernet設定のデータ形式(ASCII/BINARY)不整合"
category = "DataFormat"

[[codes]]
code = "0xC00F"
description = "IPアドレスの重複が検出された"
category = "Network"

[[codes]]
code = "0xC017"
description = "TCPコネクションのオープンに失敗した"
category = "Network"

[[codes]]
code = "0xC032"
description = "TCP/UDPのタイムアウトやACK未受信等"
category = "Transport"

[[codes]]
code = "0xC044"
description = "ICMPのエラーパケットを受信した"
category = "Icmp"
"#;
        ErrorRegistry::from_str(toml)
            .expect("parse toml")
            .register_or_merge()
            .expect("register error codes");

        assert_eq!(code_description(0x0000), Some("正常完了".into()));
        let desc_54 = code_description(0x0054).expect("description for 0x0054 expected");
        assert!(desc_54.contains("Ethernet"));
        assert_eq!(
            code_description(0xC00F),
            Some("IPアドレスの重複が検出された".into())
        );
    }

    #[test]
    fn categories_and_helpers() {
        let toml = r#"
[[codes]]
code = "0x0000"
category = "Success"

[[codes]]
code = "0x0002"
category = "Addressing"

[[codes]]
code = "0xC017"
category = "Network"

[[codes]]
code = "0xC032"
category = "Transport"

[[codes]]
code = "0xC044"
category = "Icmp"
"#;
        ErrorRegistry::from_str(toml)
            .expect("parse toml")
            .register_or_merge()
            .expect("register error codes");

        assert_eq!(code_category(0x0000), ErrorCategory::Success);
        assert_eq!(code_category(0x0002), ErrorCategory::Addressing);
        assert!(is_network_error(0xC017));
        assert!(is_transport_error(0xC032));
        assert!(is_icmp_error(0xC044));
        assert!(is_buffer_error(0x00A1));
    }

    #[test]
    fn unknown_code() {
        assert_eq!(code_description(0x9999), None);
    }
}
