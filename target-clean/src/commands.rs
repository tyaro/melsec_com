use serde::Deserialize;
use std::str::FromStr;

/// Centralized command id enum. These variant names are chosen to match the ids
/// used in `commands.toml` so serde can deserialize unit-variant strings like
/// `"read_blocks"` directly into `Command::read_blocks`.
///
///
#[derive(Debug, Deserialize, PartialEq, Eq, Hash, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum Command {
    ReadWords,
    WriteWords,
    ReadBits,
    WriteBits,
    Echo,
    ReadRandomWords,
    WriteRandomWords,
    WriteRandomBits,
    ReadBlocks,
    WriteBlocks,
}
impl Command {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::ReadWords => "read_words",
            Self::WriteWords => "write_words",
            Self::ReadBits => "read_bits",
            Self::WriteBits => "write_bits",
            Self::Echo => "echo",
            Self::ReadRandomWords => "read_random_words",
            Self::WriteRandomWords => "write_random_words",
            Self::WriteRandomBits => "write_random_bits",
            Self::ReadBlocks => "read_blocks",
            Self::WriteBlocks => "write_blocks",
        }
    }
    #[must_use]
    pub const fn is_read(&self) -> bool {
        matches!(self, Self::ReadWords | Self::ReadBits | Self::ReadBlocks)
    }

    #[must_use]
    pub const fn is_write(&self) -> bool {
        matches!(self, Self::WriteWords | Self::WriteBits | Self::WriteBlocks)
    }

    #[must_use]
    pub const fn is_block_command(&self) -> bool {
        matches!(self, Self::ReadBlocks | Self::WriteBlocks)
    }

    #[must_use]
    pub const fn is_word_command(&self) -> bool {
        matches!(self, Self::ReadWords | Self::WriteWords)
    }
}
impl FromStr for Command {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "read_words" => Ok(Self::ReadWords),
            "write_words" => Ok(Self::WriteWords),
            "read_bits" => Ok(Self::ReadBits),
            "write_bits" => Ok(Self::WriteBits),
            "echo" => Ok(Self::Echo),
            "read_blocks" => Ok(Self::ReadBlocks),
            "write_blocks" => Ok(Self::WriteBlocks),
            "read_random_words" => Ok(Self::ReadRandomWords),
            "write_random_words" => Ok(Self::WriteRandomWords),
            "write_random_bits" => Ok(Self::WriteRandomBits),
            other => Err(format!("unknown command id: {other}")),
        }
    }
}
