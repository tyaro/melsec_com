use crate::error::MelsecError;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub enum DeviceType {
    Bit,
    Word,
    DoubleWord,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub enum NumberBase {
    Decimal,     // 10進数
    Hexadecimal, // 16進数
}
use crate::plc_series::PLCSeries;
// DeviceCode implemented in `src/device_code.rs` (TOML-driven)
// include! is used so DeviceCode is defined in the `crate::device` module scope
include!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/device_code.rs"));

// デバイス定義
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Device {
    pub category: DeviceType,    // ビットデバイスかワードデバイスか
    pub base: NumberBase,        // 表記が10進数or16進数
    pub device_code: DeviceCode, // デバイスコード
    pub description: String,     // 説明
    /// Which PLC series this device is supported on (e.g. [`PLCSeries::Q`], [`PLCSeries::R`], or [`PLCSeries::Q`, `PLCSeries::R`]).
    /// Stored as an owned Vec so devices can be loaded at runtime from TOML.
    pub supported_series: Vec<PLCSeries>,
}
impl Device {
    /// Get the device symbol as a &str.
    #[must_use]
    pub fn symbol_str(&self) -> &'static str {
        self.device_code.as_str()
    }
    #[must_use]
    pub fn device_code_q(&self) -> u8 {
        u8::from(self.device_code)
    }
    #[must_use]
    pub fn device_code_r(&self) -> u16 {
        u16::from(u8::from(self.device_code))
    }
}

// COMPILED_DEVICES: devices compiled into the crate from `src/devices.toml`.
static COMPILED_DEVICES_STORE: once_cell::sync::OnceCell<Vec<Device>> =
    once_cell::sync::OnceCell::new();

fn get_compiled_devices_slice() -> &'static [Device] {
    COMPILED_DEVICES_STORE
        .get_or_init(|| {
            let s = include_str!("./devices.toml");
            // Attempt to parse the embedded devices TOML. In the rare case this
            // fails (corrupted embed), avoid an immediate panic in library code
            // and fallback to an empty device list while logging the error so
            // integrators can notice and fix the embedded resource. This change
            // reduces runtime panics while still surfacing the problem via stderr.
            match parse_devices_toml(s) {
                Ok(v) => v,
                Err(e) => {
                    log::error!("failed parse embedded devices.toml: {}", e);
                    Vec::new()
                }
            }
        })
        .as_slice()
}

// Runtime-preferred devices list: if an executable-side `define/device.toml` exists,
// parse it and use that list at runtime; otherwise fall back to the compiled-in
// `DEVICES` array produced by build.rs.
static RUNTIME_DEVICES_STORE: once_cell::sync::OnceCell<Vec<Device>> =
    once_cell::sync::OnceCell::new();

fn get_runtime_devices_slice() -> &'static [Device] {
    // prefer `define/device.toml` next to the running executable
    if let Some(p) = find_define_device_toml() {
        if let Ok(s) = std::fs::read_to_string(&p) {
            if let Ok(vec) = parse_devices_toml(&s) {
                // try to set runtime store; ignore if already set
                let _ = RUNTIME_DEVICES_STORE.set(vec);
            } else {
                log::error!("failed parse define/device.toml at {}", p.display());
            }
        }
    }
    RUNTIME_DEVICES_STORE
        .get()
        .map_or_else(|| get_compiled_devices_slice(), |v| v.as_slice())
}

#[must_use]
pub fn get_q_devices() -> Vec<Device> {
    get_runtime_devices_slice()
        .iter()
        .filter(|d| d.supported_series.contains(&PLCSeries::Q))
        .cloned()
        .collect()
}

#[must_use]
pub fn get_r_devices() -> Vec<Device> {
    get_runtime_devices_slice()
        .iter()
        .filter(|d| d.supported_series.contains(&PLCSeries::R))
        .cloned()
        .collect()
}

/// Map from symbol string to Device reference for O(1) lookup.
static DEVICE_BY_SYMBOL: Lazy<HashMap<&'static str, &'static Device>> = Lazy::new(|| {
    let mut m = HashMap::new();
    for d in get_runtime_devices_slice() {
        m.insert(d.symbol_str(), d);
    }
    m
});

/// Map from numeric device code (u8) to Device reference for O(1) lookup.
static DEVICE_BY_CODE: Lazy<HashMap<u8, &'static Device>> = Lazy::new(|| {
    let mut m = HashMap::new();
    for d in get_runtime_devices_slice() {
        m.insert(u8::from(d.device_code), d);
    }
    m
});

/// Lookup device by its symbol (e.g. "D", "M").
pub fn device_by_symbol(sym: &str) -> Option<&'static Device> {
    DEVICE_BY_SYMBOL.get(sym).copied()
}

/// Typed lookup by `Symbol` enum. Prefer this in new code.
pub fn device_by_symbol_enum(sym: Symbol) -> Option<&'static Device> {
    DEVICE_BY_SYMBOL.get(sym.as_str()).copied()
}

/// Lookup device by numeric code (u8).
pub fn device_by_code(code: u8) -> Option<&'static Device> {
    DEVICE_BY_CODE.get(&code).copied()
}

/// Parse a combined device string like "D100" or "W1FFF" into a `&Device` and a
/// numeric address (u32).
///
/// The parsing uses the `Device::base` (Decimal/Hexadecimal) defined for the
/// device to decide the numeric base for the address portion.
///
/// Rules:
/// - Leading ASCII letters form the device symbol (e.g. "D", "TS", "LTN").
/// - The remainder is interpreted as the numeric address using the device's base.
/// - Whitespace around the input is ignored. Letters are case-insensitive.
/// - Validates resulting address fits within 3 bytes (0..=0xFFFFFF) as MC3E start-address.
/// # Errors
///
/// Returns `MelsecError` when the input is invalid or the device/symbol is unknown.
pub fn parse_device_and_address(s: &str) -> Result<(&'static Device, u32), MelsecError> {
    let s = s.trim();
    if s.is_empty() {
        return Err(MelsecError::Protocol("empty device string".to_string()));
    }
    // Uppercase for symbol matching, but keep numeric part as-is for radix parsing
    let up = s.to_uppercase();
    let mut chars = up.chars();
    let mut sym_len = 0usize;
    for c in chars.by_ref() {
        if c.is_ascii_alphabetic() {
            sym_len += c.len_utf8();
        } else {
            break;
        }
    }
    // If no symbol letters found, error
    if sym_len == 0 {
        return Err(MelsecError::Protocol(format!("invalid device string: {s}")));
    }
    let symbol = &up[..sym_len];
    let num_part = s[s
        .char_indices()
        .nth(sym_len)
        .map_or(s.len(), |(byte_idx, _)| byte_idx)..]
        .trim();
    if num_part.is_empty() {
        return Err(MelsecError::Protocol(format!(
            "missing numeric address in device string: {s}"
        )));
    }

    let device = device_by_symbol(symbol)
        .ok_or_else(|| MelsecError::Protocol(format!("unknown device symbol: {symbol}")))?;

    let addr_res = match device.base {
        NumberBase::Decimal => num_part.parse::<u32>(),
        NumberBase::Hexadecimal => u32::from_str_radix(num_part, 16),
    };
    let addr = addr_res.map_err(|_| {
        MelsecError::Protocol(format!(
            "invalid numeric address '{num_part}' for device {symbol}"
        ))
    })?;
    // MC3E start address is 3 bytes little-endian (max 0xFF_FFFF)
    if addr > 0x00FF_FFFF {
        return Err(MelsecError::Protocol(format!(
            "address out of range (max 0xFFFFFF): {addr}"
        )));
    }
    Ok((device, addr))
}

/// Return path to `define/device.toml` next to the running executable, if present.
#[must_use]
pub fn find_define_device_toml() -> Option<std::path::PathBuf> {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let p = dir.join("define").join("device.toml");
            if p.exists() {
                return Some(p);
            }
        }
    }
    None
}

// Parse devices TOML into Vec<Device>. This is a small, local parser that mirrors
// the structure generated by build.rs. It is intentionally tolerant and returns
// `MelsecError` on failure.
fn parse_devices_toml(s: &str) -> Result<Vec<Device>, crate::error::MelsecError> {
    #[derive(serde::Deserialize)]
    struct RawFile {
        #[serde(rename = "device")]
        devices: Vec<RawDevice>,
    }

    #[derive(serde::Deserialize)]
    struct RawDevice {
        symbol: String,
        code: u32,
        category: String,
        base: String,
        description: String,
        #[serde(default)]
        series: Vec<String>,
    }

    let rf: RawFile = toml::from_str(s)
        .map_err(|e| crate::error::MelsecError::Protocol(format!("parse devices.toml: {e}")))?;
    let mut out: Vec<Device> = Vec::with_capacity(rf.devices.len());
    for d in rf.devices {
        let category = match d.category.as_str() {
            "Bit" => DeviceType::Bit,
            "Word" => DeviceType::Word,
            "DoubleWord" => DeviceType::DoubleWord,
            other => {
                return Err(crate::error::MelsecError::Protocol(format!(
                    "invalid category for {symbol}: {other}",
                    symbol = d.symbol,
                    other = other
                )))
            }
        };
        let base = match d.base.as_str() {
            "Decimal" => NumberBase::Decimal,
            "Hexadecimal" => NumberBase::Hexadecimal,
            other => {
                return Err(crate::error::MelsecError::Protocol(format!(
                    "invalid base for {symbol}: {other}",
                    symbol = d.symbol,
                    other = other
                )))
            }
        };
        // Map series strings to PLCSeries
        let mut series_tokens: Vec<PLCSeries> = Vec::new();
        for s in &d.series {
            match s.as_str() {
                "Q" => series_tokens.push(PLCSeries::Q),
                "R" => series_tokens.push(PLCSeries::R),
                other => {
                    return Err(crate::error::MelsecError::Protocol(format!(
                        "invalid series for {symbol}: {other}",
                        symbol = d.symbol,
                        other = other
                    )))
                }
            }
        }

        // Map numeric code to DeviceCode via TryFrom<u8>
        let code_u8 = u8::try_from(d.code).map_err(|_| {
            crate::error::MelsecError::Protocol(format!(
                "device code out of range for {symbol}: {code}",
                symbol = d.symbol,
                code = d.code
            ))
        })?;
        let dev_code = DeviceCode::try_from(code_u8)?;

        out.push(Device {
            category,
            base,
            device_code: dev_code,
            description: d.description,
            supported_series: series_tokens,
        });
    }
    Ok(out)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Symbol {
    D,
    W,
    M,
    X,
    Y,
    L,
    F,
    V,
    B,
    TS,
    TC,
    TN,
    SN,
    CN,
    CS,
    HS,
    HR,
    AR,
    DM,
    EM,
    ZR,
    LTN,
    SW,
    CC,
    CI,
    DI,
    PI,
    SI,
    BI,
    FI,
    AI,
    AO,
}
impl Symbol {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::D => "D",
            Self::W => "W",
            Self::M => "M",
            Self::X => "X",
            Self::Y => "Y",
            Self::L => "L",
            Self::F => "F",
            Self::V => "V",
            Self::B => "B",
            Self::TS => "TS",
            Self::TC => "TC",
            Self::TN => "TN",
            Self::SN => "SN",
            Self::CN => "CN",
            Self::CS => "CS",
            Self::HS => "HS",
            Self::HR => "HR",
            Self::AR => "AR",
            Self::DM => "DM",
            Self::EM => "EM",
            Self::ZR => "ZR",
            Self::LTN => "LTN",
            Self::SW => "SW",
            Self::CC => "CC",
            Self::CI => "CI",
            Self::DI => "DI",
            Self::PI => "PI",
            Self::SI => "SI",
            Self::BI => "BI",
            Self::FI => "FI",
            Self::AI => "AI",
            Self::AO => "AO",
        }
    }
    /// Convert this symbol to a `Device` reference.
    ///
    /// Prefer using `try_to_device` which returns `Option<&Device>` when the
    /// symbol may be missing. This method returns a `Result` with a
    /// `MelsecError::Protocol` when the symbol is unknown. It does not panic.
    pub fn to_device(&self) -> Result<&'static Device, crate::error::MelsecError> {
        self.try_to_device().ok_or_else(|| {
            crate::error::MelsecError::Protocol("known device symbol not found".into())
        })
    }

    /// Non-panicking form of `to_device`.
    ///
    /// Returns `Some(&Device)` when the symbol exists in the runtime device
    /// registry, or `None` when the symbol is not present. Prefer this in
    /// library code that wants to handle missing device definitions gracefully.
    #[must_use]
    pub fn try_to_device(&self) -> Option<&'static Device> {
        device_by_symbol_enum(*self)
    }
}

impl std::str::FromStr for Symbol {
    type Err = crate::error::MelsecError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "D" => Ok(Self::D),
            "W" => Ok(Self::W),
            "M" => Ok(Self::M),
            "X" => Ok(Self::X),
            "Y" => Ok(Self::Y),
            "L" => Ok(Self::L),
            "F" => Ok(Self::F),
            "V" => Ok(Self::V),
            "B" => Ok(Self::B),
            "TS" => Ok(Self::TS),
            "TC" => Ok(Self::TC),
            "TN" => Ok(Self::TN),
            "SN" => Ok(Self::SN),
            "CN" => Ok(Self::CN),
            "CS" => Ok(Self::CS),
            "HS" => Ok(Self::HS),
            "HR" => Ok(Self::HR),
            "AR" => Ok(Self::AR),
            "DM" => Ok(Self::DM),
            "EM" => Ok(Self::EM),
            "ZR" => Ok(Self::ZR),
            "LTN" => Ok(Self::LTN),
            "SW" => Ok(Self::SW),
            "CC" => Ok(Self::CC),
            "CI" => Ok(Self::CI),
            "DI" => Ok(Self::DI),
            "PI" => Ok(Self::PI),
            "SI" => Ok(Self::SI),
            "BI" => Ok(Self::BI),
            "FI" => Ok(Self::FI),
            "AI" => Ok(Self::AI),
            "AO" => Ok(Self::AO),
            other => Err(crate::error::MelsecError::Protocol(format!(
                "unknown symbol: {other}"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_decimal_device() {
        let (dev, addr) = parse_device_and_address("D100").expect("parse D100");
        assert_eq!(dev.symbol_str(), "D");
        assert_eq!(addr, 100u32);
    }

    #[test]
    fn test_parse_hex_device() {
        let (dev, addr) = parse_device_and_address("W1FFF").expect("parse W1FFF");
        assert_eq!(dev.symbol_str(), "W");
        assert_eq!(addr, 0x1FFFu32);
    }

    #[test]
    fn test_parse_invalid_symbol() {
        assert!(parse_device_and_address("QZZZ").is_err());
    }
}
