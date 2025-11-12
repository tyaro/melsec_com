//! MC4E プロトコル定義モジュール（mc3e からリネーム）
//!
//! このモジュールは MC4E / MC3E フレームの定数と簡易ユーティリティを提供します。主に
//! MC4E 形式（応答にシリアルを含む拡張形式）を想定した名前で公開します。
//! - サブヘッダ（要求/応答）の定義
//! - ヘッダサイズ、アクセス経路のデフォルト値
//! - コマンド／サブコマンド定数
//! - 簡易な end-code 名前解決
//!
//! NOTE: 低レイヤーのフレーム構築やパーサは別モジュールで実装されています。

/// MC4E ヘッダサイズ（シリアル番号付きフレームを含む）
pub const MC_HEADER_SIZE: usize = 15;

/// 汎用 MC 要求サブヘッダ（デフォルトは MC4E 系列: 0x54 0x00）
pub const MC_SUBHEADER_REQUEST: [u8; 2] = [0x54, 0x00];

/// 汎用 MC 応答サブヘッダ（デフォルトは MC4E 系列: 0xD4 0x00）
pub const MC_SUBHEADER_RESPONSE: [u8; 2] = [0xD4, 0x00];

/// アクセス経路（デフォルト）: ネットワーク0x00、PC番号0xFF、IO番号0x03FF、局番号0x00
pub const MC_ACCESS_PATH_DEFAULT: [u8; 5] = [0x00, 0xFF, 0xFF, 0x03, 0x00];

// MC end-code 定義（共通）
// Placeholder / canonical end-code constants. Add or replace with full table later.
pub const MC_END_OK: u16 = 0x0000;

/// Return a short static name for a known end-code, or None if unknown.
#[must_use]
pub const fn end_code_name(code: u16) -> Option<&'static str> {
    match code {
        MC_END_OK => Some("OK"),
        _ => None,
    }
}
// 再エクスポート: 詳細なエラーコード表を別ファイルで提供
// `src/error_codes.rs` に定義した説明付きコードを公開します。
pub use crate::error_codes::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
/// Frame format selection for MC framing. Default is MC4E (extended).
pub enum McFrameFormat {
    #[default]
    MC4E,
    MC3E,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Protocol {
    Tcp,
    Udp,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AccessRoute {
    pub network_number: u8,
    pub pc_number: u8,
    pub io_number: u16,
    pub station_number: u8,
}
impl AccessRoute {
    #[must_use]
    pub fn to_bytes(&self) -> [u8; 5] {
        let mut bytes = [0u8; 5];
        bytes[0] = self.network_number;
        bytes[1] = self.pc_number;
        bytes[2..4].copy_from_slice(&self.io_number.to_le_bytes());
        bytes[4] = self.station_number;
        bytes
    }

    #[must_use]
    pub const fn with_network_number(mut self, network_number: u8) -> Self {
        self.network_number = network_number;
        self
    }
    #[must_use]
    pub const fn with_pc_number(mut self, pc_number: u8) -> Self {
        self.pc_number = pc_number;
        self
    }
    #[must_use]
    pub const fn with_io_number(mut self, io_number: u16) -> Self {
        self.io_number = io_number;
        self
    }
    #[must_use]
    pub const fn with_station_number(mut self, station_number: u8) -> Self {
        self.station_number = station_number;
        self
    }
}
impl Default for AccessRoute {
    fn default() -> Self {
        Self {
            network_number: 0x00,
            pc_number: 0xFF,
            io_number: 0x03FF,
            station_number: 0x00,
        }
    }
}
