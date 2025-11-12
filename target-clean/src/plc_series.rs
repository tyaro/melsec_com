use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub enum PLCSeries {
    Q,
    R,
}
impl PLCSeries {
    /// Parse PLC series from string like "Q" or "R".
    #[allow(clippy::should_implement_trait)]
    #[must_use]
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "Q" | "q" => Some(Self::Q),
            "R" | "r" => Some(Self::R),
            _ => None,
        }
    }

    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Q => "Q",
            Self::R => "R",
        }
    }
}

impl std::str::FromStr for PLCSeries {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_str(s).ok_or(())
    }
}

// 将来用
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MelsecQSeriesModelName {
    Q100UDEHCPU,
    Q50UDEHCPU,
    Q26UDEHCPU,
    Q26UDHCPU,
    Q20UDEHCPU,
    Q20UDHCPU,
    Q13UDEHCPU,
    Q13UDHCPU,
    Q10UDEHCPU,
    Q10UDHCPU,
    Q06UDEHCPU,
    Q06UDHCPU,
    Q04UDEHCPU,
    Q04UDHCPU,
    Q03UDECPU,
    Q03UDCPU,
    Q02UCPU,
    Q01UCPU,
    Q00UCPU,
    Q00UJCPU,
}

// 将来用
pub enum MelsecRSeriesModelName {
    R00CPU,
    R01CPU,
    R02CPU,
    R04CPU,
    R08CPU,
    R16CPU,
    R32CPU,
    R120CPU,
    R04ENCPU,
    R08ENCPU,
    R16ENCPU,
    R32ENCPU,
    R120ENCPU,
    R08PCPU,
    R16PCPU,
    R32PCPU,
    R120PCPU,
}
