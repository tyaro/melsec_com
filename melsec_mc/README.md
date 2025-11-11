# melsec_mc

[![crates.io](https://img.shields.io/crates/v/melsec_mc.svg)](https://crates.io/crates/melsec_mc)
[![docs.rs](https://docs.rs/melsec_mc/badge.svg)](https://docs.rs/melsec_mc)
[![Crates Downloads](https://img.shields.io/crates/dt/melsec_mc.svg)](https://crates.io/crates/melsec_mc)
[![GitHub release (latest SemVer)](https://img.shields.io/github/v/release/tyaro/melsec_mc.svg)](https://github.com/tyaro/melsec_mc/releases)
[![CI](https://github.com/tyaro/melsec_mc/actions/workflows/ci.yml/badge.svg)](https://github.com/tyaro/melsec_mc/actions)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![dependency status](https://deps.rs/repo/github/tyaro/melsec_mc/status.svg)](https://deps.rs/repo/github/tyaro/melsec_mc)

三菱電機 PLC と MC プロトコル（Ethernet）用の軽量な Rust ライブラリです。

提供内容（概要）

- 非同期 TCP トランスポート（Tokio ベース）と簡易クライアント
- エラー型とレスポンス/リクエストの最低限の構造
- 生のフレーム送受信を行うサンプルとユーティリティ

注: 高レベルなバッチ読み書き API は一部実装済みですが、今後拡張予定です。

## English summary

Lightweight Rust library for Mitsubishi Electric PLCs (MC protocol over Ethernet, compatible with MC4E). Provides an async Tokio-based transport, utilities for building and parsing MC frames, and a simple reusable `McClient` for common read/write operations.

This README is primarily in Japanese; a short English summary is provided above and a more detailed English README is available in `README.en.md`.

## 目次

- クイックスタート
- インストール
- 使い方（簡単な例）
- 高度な利用（Typed API、McClient）
- リリースと公開
- 貢献方法、ライセンス

バージョン: 0.4.13

## クイックスタート

### クローンとビルド

```powershell
git clone https://github.com/tyaro/melsec_mc.git
cd melsec_mc
cargo build
```

### サンプル実行（ `examples` を参照して PLC アドレスを設定してください）

```powershell
cargo run --example simple
```

## インストール

crates.io に公開済みであれば `Cargo.toml` に追加できます（例）:

```toml
[dependencies]
melsec_mc = "0.4.13"
```

開発中に git から参照する場合:

```toml
[dependencies]
melsec_mc = { git = "https://github.com/tyaro/melsec_mc", branch = "main" }
```

## 使い方（簡単な例）

非同期 Tokio 環境での利用例:

```rust
use melsec_mc::{McClient, ConnectionTarget};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 接続先を指定してクライアントを作成します
    let target = ConnectionTarget::direct("192.168.1.40", 4020);
    // McClient::new().with_target(...) でターゲットを組み込んだクライアントを生成します
    // 必要に応じて MC4E / MC3E を明示的に選択できます。
    let client = McClient::new().with_target(target).with_mc_format(melsec_mc::mc_define::McFrameFormat::MC4E);

    // ビット読み出し: device は開始アドレスを含む文字列（例: "M100"）
    let bits = client.read_bits("M100", 10).await?; // 10 ビットを読み取る
    println!("bits: {:?}", bits);

    // ワード読み出し: device は開始アドレスを含む文字列（例: "D1000"）
    let words = client.read_words("D1000", 2).await?; // 2 ワードを読み取る
    println!("words: {:?}", words);

    Ok(())
}
```

簡単な注意:

- `McClient` の `with_mc_format(...)` で MC4E/MC3E を明示できます。Mock サーバは受信フレームを自動判定して同じ形式で応答します。

## 型付き読み書き（Typed API）

このリリースでは `FromWords` / `ToWords` トレイトにより、`f32` や `u32`、`[bool;16]` のような複数ワードにまたがる型を直接読み書きできます。例:

```rust
// 読み取り（要素数 = 2 の f32 を 2 要素取得すると内部的に 4 ワード要求します）
let floats: Vec<f32> = client.read_words_as::<f32>("D1000", 2).await?;

// 書き込み
client.write_words_as("D1010", &vec![1.23f32, 4.56f32]).await?;
```

型付き読み出しは要素単位で解析を行い、解析に失敗した要素は警告ログを出してスキップし、成功した要素は返却されます（部分的な受信に対して寛容に動作します）。

## リリースと公開

- このリポジトリは GitHub Release を利用しています（[https://github.com/tyaro/melsec_mc/releases]）。
- crates.io に公開する場合は `cargo publish` を使用してください（公開には 2FA とクレデンシャルが必要です）。

パッケージ作成の検証:

```powershell
cargo publish --dry-run
```

## 貢献と連絡

- プルリクエスト歓迎です。大きな API 変更は事前に Issue で相談してください。
- バグ報告や機能要望は GitHub Issue を利用してください。

## ライセンス

MIT

---
日本語版 README（`README.md`）を更新しました。英語版は `README.en.md` を参照してください。

### 備考

## Payload フィールドの種類（TOML 定義）

`commands.toml` の `request_format` ではペイロードを表すフィールドに下記のような種類が使えます。

- `bytes`:
  - 生のバイト列を表します。パラメータ側では数値配列（例: `[16, 32, 255]`）を与えるか、文字列を与えるとそのバイト列がそのまま送信されます（`ascii_hex` のような文字検査は行われません）。
  - 旧式のエイリアス `rest` および `..` は互換のためにサポートされていますが、新規定義では `bytes` を推奨します。

- `ascii_hex`:
  - ASCII 文字で表現された 16 進文字列（`0-9 A-F a-f`）を扱います。パラメータ側は文字列（例: `"0123AB"`）を指定し、ライブラリは各文字をその ASCII バイトとして送信します。
  - Echo のようなコマンド（受け取った ASCII 16 進文字列をそのまま返す）や、テキストベースの 16 進表現を扱う用途に便利です。

例（echo コマンド）:

```toml
[[command]]
id = "echo"
command_code = 0x0619
request_format = ["command:2be", "subcommand:2be", "payload:ascii_hex"]
response_format = ["payload:ascii_hex"]
device_family = "Any"
```
