use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::Path;

/// Normalize possible legacy combined keys like "D100" into ("D", 100).
/// If addr != 0 and key contains digits, the explicit addr is preferred and
/// a warning is emitted.
pub fn normalize_key_addr(key: &str, addr: usize) -> (String, usize) {
    // Try override symbol first (sled/db overrides)
    if let Some(ov) = melsec_mc::device_registry::DeviceRegistry::get_override_by_symbol(key) {
        if let Ok(code) = u8::try_from(ov.code) {
            return (format!("0x{:02X}", code), addr);
        }
    }

    // If key looks like a combined key like "D100", prefer parsing it.
    if !key.is_empty() {
        // use upstream parser which handles device symbols and combined forms
        if let Ok((dev, parsed_addr)) = melsec_mc::device::parse_device_and_address(key) {
            let code = dev.device_code_q();
            let final_addr = if addr == 0 {
                parsed_addr as usize
            } else {
                addr
            };
            if addr != 0 {
                tracing::warn!(key = %key, addr, "ambiguous combined key with explicit addr: preferring explicit addr parameter");
            }
            return (format!("0x{:02X}", code), final_addr);
        }
    }

    // If key is a plain symbol like "D"
    if let Some(dev) = melsec_mc::device::device_by_symbol(key) {
        return (format!("0x{:02X}", dev.device_code_q()), addr);
    }

    // Hex or decimal literal
    if key.starts_with("0x") || key.starts_with("0X") {
        if let Ok(n) = u8::from_str_radix(&key[2..], 16) {
            return (format!("0x{:02X}", n), addr);
        }
    } else if let Ok(n) = key.parse::<u8>() {
        return (format!("0x{:02X}", n), addr);
    }

    // Fallback to code 0
    (format!("0x{:02X}", 0u8), addr)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DeviceKey {
    pub code: u8,
    /// Unit indicates whether this device is word-addressable or bit-addressable.
    /// Currently unused by storage logic but helpful for future extensions.
    pub unit: DeviceUnit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeviceUnit {
    Word,
    Bit,
}

impl DeviceKey {
    pub const fn from_code(code: u8) -> Self {
        Self {
            code,
            unit: DeviceUnit::Word,
        }
    }
    pub const fn code(&self) -> u8 {
        self.code
    }
}

// Keep on-disk/JSON compatibility with previous implementation which used
// bare u8 device codes as map keys. Implement Serialize/Deserialize to
// represent DeviceKey as the numeric u8 code.
impl serde::Serialize for DeviceKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_u8(self.code)
    }
}

impl<'de> serde::Deserialize<'de> for DeviceKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let code = u8::deserialize(deserializer)?;
        Ok(DeviceKey::from_code(code))
    }
}
pub type Word = u16;

#[derive(Debug, Default, Serialize, Deserialize)]
/// In-memory storage for mock PLC device areas.
///
/// `DeviceMap` は mock サーバが扱うメモリマップを提供します。キーはデバイスコード
///（例: D, X を数値コード化）で、値は u16 ワードのベクタです。主にテスト/モック用で、
/// 実機の永続スナップショット読み書きや TOML による初期化をサポートします。
pub struct DeviceMap {
    inner: HashMap<DeviceKey, Vec<Word>>,
}

impl DeviceMap {
    pub fn new() -> Self {
        Self {
            inner: HashMap::new(),
        }
    }

    /// Example:
    /// ```no_run
    /// let mut dm = melsec_mc_mock::device_map::DeviceMap::new();
    /// dm.set_words("D", 0, &[0x1234u16]);
    /// let w = dm.get_words("D", 0, 1);
    /// ```
    pub fn set_words(&mut self, key: &str, addr: usize, words: &[Word]) {
        // Centralize key/address normalization using melsec_mc helpers.
        let (dkey, resolved_addr) = normalize_key_addr(key, addr);
        tracing::debug!(key = %key, resolved_key = ?dkey, addr = resolved_addr, words = ?words, "device_map.set_words called");
        // convert canonical key ("0x.." or decimal) to DeviceKey for map lookup
        let code_val = if dkey.starts_with("0x") || dkey.starts_with("0X") {
            u8::from_str_radix(&dkey[2..], 16).unwrap_or(0u8)
        } else {
            dkey.parse::<u8>().unwrap_or(0u8)
        };
        let dk = DeviceKey::from_code(code_val);
        tracing::debug!(orig_key = %key, code = code_val, resolved_addr = resolved_addr, words = ?words, "device_map.set_words resolved");
        let vec = self.inner.entry(dk).or_default();
        if vec.len() < resolved_addr + words.len() {
            vec.resize(resolved_addr + words.len(), 0);
        }
        vec[resolved_addr..resolved_addr + words.len()].copy_from_slice(words);
    }

    pub fn get_words(&self, key: &str, addr: usize, count: usize) -> Vec<Word> {
        let (dkey, resolved_addr) = normalize_key_addr(key, addr);
        // convert canonical key to DeviceKey for lookup
        let code_val = if dkey.starts_with("0x") || dkey.starts_with("0X") {
            u8::from_str_radix(&dkey[2..], 16).unwrap_or(0u8)
        } else {
            dkey.parse::<u8>().unwrap_or(0u8)
        };
        let dk = DeviceKey::from_code(code_val);
        match self.inner.get(&dk) {
            Some(vec) => {
                let mut out = Vec::with_capacity(count);
                for i in 0..count {
                    out.push(*vec.get(resolved_addr + i).unwrap_or(&0));
                }
                out
            }
            None => vec![0; count],
        }
    }

    /// Clear all stored device words (management helper)
    pub fn clear(&mut self) {
        self.inner.clear();
    }

    /// Return true when the internal map is empty
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Return whether the given key is present in the internal map (used for tests).
    pub fn has_key(&self, key: &str) -> bool {
        let (dkey, _addr) = normalize_key_addr(key, 0);
        let code_val = if dkey.starts_with("0x") || dkey.starts_with("0X") {
            u8::from_str_radix(&dkey[2..], 16).unwrap_or(0u8)
        } else {
            dkey.parse::<u8>().unwrap_or(0u8)
        };
        let dk = DeviceKey::from_code(code_val);
        self.inner.contains_key(&dk)
    }

    /// Save the current device map to a JSON file at the given path.
    /// This is synchronous and intended for simple persistence on shutdown.
    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> anyhow::Result<()> {
        let f = File::create(path.as_ref())?;
        let mut w = BufWriter::new(f);
        serde_json::to_writer(&mut w, &self)?;
        Ok(())
    }

    /// Load device map from a JSON file if present. Returns Ok(Some(dm)) when loaded,
    /// Ok(None) when file not present, Err on parse/open errors.
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> anyhow::Result<Option<DeviceMap>> {
        let p = path.as_ref();
        if !p.exists() {
            return Ok(None);
        }
        let f = File::open(p)?;
        let r = BufReader::new(f);
        let dm: DeviceMap = serde_json::from_reader(r)?;
        Ok(Some(dm))
    }

    /// Populate the device map from a TOML file with the following simple format:
    ///
    /// ```toml
    /// [devices]
    /// X = 8192
    /// Y = 8192
    /// D = 12288
    /// ```
    ///
    /// Values may be integers or strings like `8K`/`2K` (K meaning *1024).
    pub fn populate_from_toml<P: AsRef<Path>>(&mut self, path: P) -> anyhow::Result<()> {
        // Simple TOML-lite parser: only understands a [devices] section and
        // key = value lines. This avoids adding a hard dependency on a
        // particular toml crate version and is sufficient for our usage.
        let p = path.as_ref();
        let s = std::fs::read_to_string(p)
            .with_context(|| format!("failed to read device assignment file: {}", p.display()))?;
        let mut in_devices = false;
        for line in s.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if line.starts_with('[') {
                // section header
                in_devices = line.trim() == "[devices]";
                continue;
            }
            if !in_devices {
                continue;
            }
            // parse key = value
            if let Some(eq_pos) = line.find('=') {
                let key = line[..eq_pos].trim().trim_matches('"');
                let mut val = line[eq_pos + 1..].trim();
                // strip optional quotes
                if val.starts_with('"') && val.ends_with('"') && val.len() >= 2 {
                    val = &val[1..val.len() - 1];
                }
                let count_opt = if val.is_empty() {
                    None
                } else {
                    // accept formats like 8K / 8k or plain integer
                    let last = val.chars().last().unwrap();
                    if last == 'K' || last == 'k' {
                        val[..val.len() - 1]
                            .trim()
                            .parse::<usize>()
                            .ok()
                            .map(|n| n.saturating_mul(1024))
                    } else {
                        val.parse::<usize>().ok()
                    }
                };
                if let Some(count) = count_opt {
                    if count == 0 {
                        continue;
                    }
                    // Special handling for timers/counters: expand to sub-units
                    // T -> TS/TN/TC, LT -> LTS/LTN/LTC, ST -> STS/STN/STC, LST -> LSTS/LSTN/LSTC
                    // C -> CTS/CTN/CTC, LC -> LCTS/LCTN/LCTC
                    // For ZR (file registers) we skip eager preallocation by default
                    let key_upper = key.to_uppercase();
                    if key_upper == "ZR" {
                        tracing::info!(symbol=%key, points=count, "skipping eager allocation for ZR (lazy allocation enabled)");
                        continue;
                    }
                    let targets: Vec<&str> = match key_upper.as_str() {
                        "T" => vec!["TS", "TN", "TC"],
                        "LT" => vec!["LTS", "LTN", "LTC"],
                        "ST" => vec!["STS", "STN", "STC"],
                        "LST" => vec!["LSTS", "LSTN", "LSTC"],
                        "C" => vec!["CTS", "CTN", "CTC"],
                        "LC" => vec!["LCTS", "LCTN", "LCTC"],
                        _ => vec![key],
                    };
                    // initialize each target with zeroed words
                    let zeros = vec![0u16; count];
                    for t in targets {
                        self.set_words(t, 0, &zeros);
                        tracing::info!(symbol=%t, points=count, "populated device points from toml (expanded)");
                    }
                } else {
                    tracing::warn!(symbol=%key, value=%val, "skipping invalid device assignment entry");
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn set_get_words_roundtrip() {
        let mut dm = DeviceMap::new();
        dm.set_words("D", 0, &[0x1234u16, 0x5678u16]);
        let got = dm.get_words("D", 0, 2);
        assert_eq!(got, vec![0x1234u16, 0x5678u16]);
    }

    #[test]
    fn populate_from_toml_loads_defaults() {
        let mut dm = DeviceMap::new();
        // default_device_assignment.toml is placed in the crate directory
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("default_device_assignment.toml");
        dm.populate_from_toml(&path)
            .expect("populate_from_toml failed");
        assert!(
            !dm.is_empty(),
            "device map should not be empty after populate"
        );
        // spot-check a few devices
        assert_eq!(dm.get_words("X", 0, 1), vec![0u16]);
        assert_eq!(dm.get_words("Z", 19, 1), vec![0u16]);
        // D has 12288 points in the default file; ensure the last index is present
        assert_eq!(dm.get_words("D", 12287, 1), vec![0u16]);
    }

    #[test]
    fn zr_is_lazy_allocated_and_allocates_on_write() {
        let mut dm = DeviceMap::new();
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("default_device_assignment.toml");
        dm.populate_from_toml(&path)
            .expect("populate_from_toml failed");
        // ZR should NOT be present as a preallocated key
        assert!(
            !dm.has_key("ZR"),
            "ZR should not be preallocated by default"
        );
        // reading returns zeros without creating the key
        assert_eq!(dm.get_words("ZR", 0, 1), vec![0u16]);
        assert!(!dm.has_key("ZR"), "get_words should not create the key");
        // writing should create the key
        dm.set_words("ZR", 0, &[0x1234u16]);
        assert!(dm.has_key("ZR"), "set_words should create ZR entry");
        assert_eq!(dm.get_words("ZR", 0, 1), vec![0x1234u16]);
    }
}
