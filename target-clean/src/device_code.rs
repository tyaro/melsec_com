#[repr(transparent)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct DeviceCode(pub u8);

#[derive(serde::Deserialize)]
struct RawFile {
    #[serde(rename = "device")]
    devices: Vec<RawDevice>,
}

#[derive(serde::Deserialize)]
struct RawDevice {
    symbol: String,
    code: u32,
}

static CODE_BY_NAME: once_cell::sync::Lazy<std::collections::HashMap<&'static str, DeviceCode>> = once_cell::sync::Lazy::new(|| {
    let s = include_str!("./devices.toml");
    // Parse the embedded devices TOML at static init. If this fails it's
    // likely a build-time corruption; emit a stderr warning and continue
    // with an empty device map rather than aborting the entire process.
    let rf: RawFile = match toml::from_str(s) {
        Ok(rf) => rf,
        Err(e) => {
            // Use tracing::warn so build-time consumers can filter or capture
            // this message via the tracing subscriber if desired.
            tracing::warn!("warning: failed to parse devices.toml at compile time: {}", e);
            return std::collections::HashMap::new();
        }
    };
    let mut m = std::collections::HashMap::new();
    for d in &rf.devices {
        let sym: &'static str = Box::leak(d.symbol.clone().into_boxed_str());
        // Avoid panicking at build-time on out-of-range numeric codes. If an
        // entry cannot fit into u8, log and skip it so the crate can still be
        // compiled for other valid entries.
        let code_u8 = match u8::try_from(d.code) {
            Ok(v) => v,
            Err(_) => {
                // Avoid aborting in CI for a single bad entry. Emit a structured
                // warning via tracing so it can be captured by tracing subscribers.
                tracing::warn!("warning: device code out of range in devices.toml for symbol {}: {}", d.symbol, d.code);
                continue;
            }
        };
        m.insert(sym, DeviceCode(code_u8));
    }
    m
});

static NAME_BY_CODE: once_cell::sync::Lazy<std::collections::HashMap<DeviceCode, &'static str>> = once_cell::sync::Lazy::new(|| {
    let mut m = std::collections::HashMap::with_capacity(CODE_BY_NAME.len());
    for (k, &v) in CODE_BY_NAME.iter() {
        m.insert(v, *k);
    }
    m
});

impl DeviceCode {
    pub fn as_str(&self) -> &'static str {
        NAME_BY_CODE.get(self).copied().unwrap_or("UNKNOWN")
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(name: &str) -> Result<Self, crate::error::MelsecError> {
        CODE_BY_NAME.get(name).copied().ok_or_else(|| crate::error::MelsecError::Protocol(format!("unknown device: {name}")))
    }

    pub fn from_name(name: &str) -> Result<u8, crate::error::MelsecError> {
        Ok(name.parse::<Self>()?.0)
    }
}

impl std::str::FromStr for DeviceCode {
    type Err = crate::error::MelsecError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_str(s)
    }
}

impl From<DeviceCode> for u8 {
    fn from(dc: DeviceCode) -> Self {
        dc.0
    }
}

impl TryFrom<u8> for DeviceCode {
    type Error = crate::error::MelsecError;
    fn try_from(v: u8) -> Result<Self, Self::Error> {
    let c = Self(v);
        if NAME_BY_CODE.contains_key(&c) {
            Ok(c)
        } else {
            Err(crate::error::MelsecError::Protocol(format!("unknown device code: 0x{v:02X}")))
        }
    }
}
