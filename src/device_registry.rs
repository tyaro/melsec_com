use crate::error::MelsecError;
use crate::toml_helpers::extract_line_col_from_msg;
use once_cell::sync::OnceCell;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::RwLock;

#[derive(Debug, Deserialize)]
struct DeviceFile {
    #[serde(rename = "device")]
    pub devices: Vec<DeviceRaw>,
}

#[derive(Debug, Deserialize, Clone)]
struct DeviceRaw {
    pub symbol: String,
    pub code: u32,
    pub category: String,
    pub base: Option<String>,
    pub description: Option<String>,
    // optional GUI display name
    pub gui_name: Option<String>,
    // optional compatible series list
    pub compatible_series: Option<Vec<String>>,
    // optional bit mapping entries for devices that require custom packing
    pub bit_mapping: Option<Vec<BitMapEntry>>,
}

#[derive(Debug, Clone)]
pub struct DeviceOverride {
    pub symbol: String,
    pub code: u32,
    pub category: String,
    pub base: Option<String>,
    pub description: Option<String>,
    pub gui_name: Option<String>,
    pub compatible_series: Option<Vec<String>>,
    pub bit_mapping: Option<Vec<BitMapEntryOwned>>,
}

static DEVICE_OVERRIDES: OnceCell<RwLock<HashMap<String, DeviceOverride>>> = OnceCell::new();

/// In-memory handle for a parsed device TOML file. Provides parsing, validation
/// and registration helpers. This mirrors the style of `CommandRegistry`.
pub struct DeviceRegistry {
    devices: Vec<DeviceRaw>,
}

impl DeviceRegistry {
    /// Parse a device TOML from a string and return a registry value.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Result<Self, MelsecError> {
        let df: DeviceFile = toml::from_str(s).map_err(|e| {
            let s = e.to_string();
            if let Some((line, col)) = extract_line_col_from_msg(&s) {
                MelsecError::Protocol(format!("device.toml parse error at {line}:{col}: {s}"))
            } else {
                MelsecError::Protocol(format!("device.toml parse error: {s}"))
            }
        })?;
        Ok(Self {
            devices: df.devices,
        })
    }

    /// Load and parse a device TOML from a filesystem path.
    pub fn from_path(path: &Path) -> Result<Self, MelsecError> {
        let s = fs::read_to_string(path)
            .map_err(|e| MelsecError::Protocol(format!("read device toml: {e}")))?;
        s.parse::<Self>()
    }

    /// Find `define/device.toml` next to the executable and parse it.
    pub fn from_exe_define() -> Result<Self, MelsecError> {
        if let Some(p) = crate::device::find_define_device_toml() {
            return Self::from_path(&p);
        }
        Err(MelsecError::Protocol(
            "define/device.toml not found next to exe".into(),
        ))
    }

    /// Validate TOML syntax and some semantic constraints from a string.
    pub fn validate_str(s: &str) -> Result<(), MelsecError> {
        let df: DeviceFile = toml::from_str(s).map_err(|e| {
            let s = e.to_string();
            if let Some((line, col)) = extract_line_col_from_msg(&s) {
                MelsecError::Protocol(format!("device.toml parse error at {line}:{col}: {s}"))
            } else {
                MelsecError::Protocol(format!("device.toml parse error: {s}"))
            }
        })?;
        let mut syms = std::collections::HashSet::new();
        for d in &df.devices {
            if d.symbol.trim().is_empty() {
                return Err(MelsecError::Protocol("device symbol empty".to_string()));
            }
            if !syms.insert(d.symbol.clone()) {
                return Err(MelsecError::Protocol(format!(
                    "duplicate device symbol: {s}",
                    s = d.symbol
                )));
            }
            if d.code > 0xFFFF {
                return Err(MelsecError::Protocol(format!(
                    "device code out of range: {code}",
                    code = d.code
                )));
            }
            match d.category.as_str() {
                "Bit" | "Word" | "DoubleWord" => {}
                _ => {
                    return Err(MelsecError::Protocol(format!(
                        "invalid device category: {cat}",
                        cat = d.category
                    )));
                }
            }
            if let Some(g) = d.gui_name.as_ref() {
                if g.trim().is_empty() {
                    return Err(MelsecError::Protocol(format!(
                        "device {sym}: gui_name is empty",
                        sym = d.symbol
                    )));
                }
            }
            if let Some(cps) = d.compatible_series.as_ref() {
                for s in cps {
                    match s.as_str() {
                        "Q" | "R" => {}
                        other => {
                            return Err(MelsecError::Protocol(format!(
                                "device {sym}: invalid compatible_series entry: {other}",
                                sym = d.symbol
                            )))
                        }
                    }
                }
            }
            if let Some(bmap) = d.bit_mapping.as_ref() {
                for e in bmap {
                    if e.bit_pos >= 8 {
                        return Err(MelsecError::Protocol(format!(
                            "device {sym}: bit_mapping bit_pos out of range: {pos}",
                            sym = d.symbol,
                            pos = e.bit_pos
                        )));
                    }
                }
            }
        }
        Ok(())
    }

    /// Register parsed overrides into the global in-memory map.
    pub fn register_overrides(&self) -> Result<(), MelsecError> {
        let mut map: HashMap<String, DeviceOverride> = HashMap::new();
        for d in &self.devices {
            let ov = DeviceOverride {
                symbol: d.symbol.clone(),
                code: d.code,
                category: d.category.clone(),
                base: d.base.clone(),
                description: d.description.clone(),
                gui_name: d.gui_name.clone(),
                compatible_series: d.compatible_series.clone(),
                bit_mapping: d.bit_mapping.clone().map(|v| {
                    v.into_iter()
                        .map(|e| BitMapEntryOwned {
                            bit_index: e.bit_index,
                            byte_offset: e.byte_offset,
                            bit_pos: e.bit_pos,
                        })
                        .collect()
                }),
            };
            map.insert(d.symbol.clone(), ov);
        }
        DEVICE_OVERRIDES
            .set(RwLock::new(map))
            .map_err(|_| MelsecError::Protocol("device overrides already registered".into()))?;
        Ok(())
    }

    /// Convenience: parse from `path` and register overrides
    pub fn register_overrides_from_path(path: &Path) -> Result<(), MelsecError> {
        let reg = Self::from_path(path)?;
        reg.register_overrides()
    }

    /// Convenience: locate exe define and register overrides
    pub fn register_overrides_from_exe_define() -> Result<(), MelsecError> {
        let reg = Self::from_exe_define()?;
        reg.register_overrides()
    }

    /// Convenience: locate exe define and parse only (no registration)
    pub fn validate_exe_define() -> Result<(), MelsecError> {
        if let Some(p) = crate::device::find_define_device_toml() {
            let s = fs::read_to_string(&p)
                .map_err(|e| MelsecError::Protocol(format!("read device toml: {e}")))?;
            return Self::validate_str(&s);
        }
        Err(MelsecError::Protocol(
            "define/device.toml not found next to exe".into(),
        ))
    }

    /// Get a registered override by symbol (cloned). Returns None if not registered.
    pub fn get_override_by_symbol(sym: &str) -> Option<DeviceOverride> {
        DEVICE_OVERRIDES
            .get()
            .and_then(|rw| rw.read().ok())
            .and_then(|m| m.get(sym).cloned())
    }
}

impl std::str::FromStr for DeviceRegistry {
    type Err = MelsecError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_str(s)
    }
}

#[derive(Debug, Deserialize, Clone)]
struct BitMapEntry {
    /// logical bit index within a block (0..)
    pub bit_index: u32,
    /// byte offset in the packed payload where this bit is located (optional)
    pub byte_offset: Option<u32>,
    /// bit position within byte (0..7)
    pub bit_pos: u8,
}

#[derive(Clone, Debug, Deserialize)]
pub struct BitMapEntryOwned {
    pub bit_index: u32,
    pub byte_offset: Option<u32>,
    pub bit_pos: u8,
}

// extract_line_col_from_msg moved to `src/toml_helpers.rs` and is imported above.
