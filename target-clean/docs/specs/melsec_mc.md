# melsec_mc — コアライブラリ仕様

このドキュメントはリポジトリの現行実装に基づき、`melsec_mc` クレートの公開 API と主要型の契約（入力/出力/エラー）をまとめたものです。

概要
- `melsec_mc` は Mitsubishi PLC の MC プロトコル（MC3E/MC4E 相当）の送受信・リクエスト構築・応答解析を提供する Rust ライブラリです。
- 非同期ランタイム（tokio）環境で動作し、TCP と UDP の transport をサポートします。

設計方針（要点）
- 高レベル API は利便性重視で JSON (`serde_json::Value`) を返すものが多い。
- 型安全な読み書きを望む場合は `FromWords` / `ToWords` トレイトを用いる補助 API (`read_words_as` / `write_words_as`) を使う。
- 低レベルの MC フレーム組立／解析は `src/request.rs` / `src/response.rs` / `src/mc_frame.rs` に分離されている。

主要ファイル
- `src/lib.rs` — クレート再エクスポートと初期化ヘルパ（`init_defaults`）
- `src/mc_client.rs` — 高レベルクライアント `McClient`（公開 API の大部分）
- `src/request.rs` — `McRequest`（MC フレームの組立と復元）
- `src/response.rs` — `McResponse` と `parse_mc_payload`
- `src/transport.rs` — TCP/UDP の送受信ロジック
- `src/mc_define.rs` — 定数類（サブヘッダ、AccessRoute、Protocol 等）

公開 API（代表シグネチャと契約）

注: 下記は現行ソースの代表的なシグネチャを要約したものです。正確な詳細は各 `src/*.rs` を参照してください。

- McClient の生成
  - `pub fn new() -> McClient` — デフォルト設定でインスタンスを作る。`with_target`, `with_plc_series`, `with_protocol`, `with_monitoring_timer`, `with_client_name` のチェイン式設定が可能。

- ワード読み取り（高レベル）
  - `pub async fn read_words(&self, device: &str, count: u16) -> Result<serde_json::Value, MelsecError>`
  - 説明: 指定デバイスから `count` ワード分を読み取り、JSON（`data_blocks` 等）で返す。エラーハンドリングは `MelsecError`。

- ワード読み取り（型安全）
  - `pub async fn read_words_as<T: FromWords>(&self, device: &str, count: u16) -> Result<Vec<T>, MelsecError>`
  - 説明: `T::from_words_slice` を用いて受信ワード列を `T` の要素列にデコードする。部分失敗はログ出力して可能な限り要素を返す設計。

- ビット読み取り
  - `pub async fn read_bits(&self, device: &str, count: u16) -> Result<serde_json::Value, MelsecError>`

- ワード書込
  - `pub async fn write_words(&self, device: &str, values: &[u16]) -> Result<serde_json::Value, MelsecError>`

- ワード書込（型から展開）
  - `pub async fn write_words_as<T: ToWords>(&self, device: &str, values: &[T]) -> Result<serde_json::Value, MelsecError>`

- ビット書込
  - `pub async fn write_bits(&self, device: &str, values: &[bool]) -> Result<serde_json::Value, MelsecError>`
  - 実装注意: クライアントはブール配列をニブル単位（2点で1バイト、先頭が上位ニブル）でパックして送信する。受信側（PLC）もペイロード長として `ceil(count/2)` バイトを期待する。

- エコー
  - `pub async fn echo(&self, payload: &str) -> Result<String, MelsecError>` — ASCII hex 文字列を送信し、PLC からのエコーを文字列で返す。

内部／補助 API
- `send_and_recv_with_retry(&self, mc_payload: &[u8], timeout: Option<Duration>) -> Result<Vec<u8>, MelsecError>` — 再試行付き送受信（非公開）

主要型と補助トレイト

- `MelsecError` (`src/error.rs`)
  - 列挙子: `Io(std::io::Error)`, `Timeout`, `Protocol(String)`, `AlreadyRegistered`, `NoTarget`

- `ConnectionTarget` (`src/endpoint.rs`)
  - フィールド: `ip: String`, `port: u16`, `addr: String`, `access_route: AccessRoute`
  - 生成補助: `ConnectionTarget::new()`, `ConnectionTarget::direct(ip, port)` — 実装上は `(ip, port)` の 2 引数。

- `McRequest` (`src/request.rs`)
  - 役割: MC フレーム（要求）を組み立てる。主要メソッド: `McRequest::new()`, `try_with_request_data(...) -> Result<McRequest, MelsecError>`, `with_access_route`, `with_serial_number`, `with_monitoring_timer`, `build() -> Vec<u8>`, `try_from_payload(payload: &[u8]) -> Result<McRequest, MelsecError>`（受信フレームから復元）。

- `McResponse` (`src/response.rs`)
  - フィールド: `subheader: [u8;2]`, `access_route: [u8;5]`, `request_data_len: u16`, `data: Vec<u8>`, `end_code: Option<u16>`, `has_end_code: bool`, `serial_number: Option<u16>`
  - コンストラクタ: `McResponse::try_new(payload: &[u8]) -> Result<McResponse, MelsecError>`（パーサ経由）

- `FromWords` / `ToWords` (`src/mc_client.rs`)
  - `FromWords`: 固定個数の u16 ワードから型を復元するためのトレイト。`const WORDS: usize` と `fn from_words_slice(words: &[u16]) -> Result<(Self, usize), MelsecError>` を提供。
  - `ToWords`: 型を u16 ワード列に変換するトレイト。`fn to_words(&self, out: &mut Vec<u16>)`。
  - 既定実装: `u16`, `i16`, `[bool;16]`, `u32`, `i32`, `f32` などが実装されている。

運用・テストに関する注意点
- ログは `tracing`（もしくは `log` 経由）で出力される。詳細は `RUST_LOG` 環境変数で制御。
- 実機比較テストはオプトイン（環境変数 `REAL_PLC_ADDR` や `REAL_PLC_STRICT` で制御）。CI では mock ベーステストを実行することが推奨される。

使用例（実装に忠実な最小例）

```rust
use melsec_com::McClient;
use melsec_com::endpoint::ConnectionTarget;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ConnectionTarget::direct(ip, port)
    let target = ConnectionTarget::direct("192.168.0.10", 502);
    let client = McClient::new().with_target(target);

    // JSON で結果を受け取る高レベル呼び出し
    let words_json = client.read_words("D", 10).await?;
    println!("D0.. = {:?}", words_json);

    // 型安全な読み取り（FromWords を利用）
    let words_typed: Vec<u16> = client.read_words_as::<u16>("D", 10).await?;
    println!("typed: {:?}", words_typed);

    // ビット書き込み（device は文字列で表現、例: "M5" や "M" + offset）
    let bits = vec![true, false, true, true];
    let _resp = client.write_bits("M5", &bits).await?;

    Ok(())
}
```

## 公開関数: 完全シグネチャ一覧

下記は `src/mc_client.rs` に実装されている公開（public）関数・メソッドと、`FromWords`/`ToWords` トレイトの完全シグネチャ一覧です。コピー＆ペーストして利用できる形で記載しています。

```rust
// McClient: コンストラクタ / ビルダ風メソッド
pub fn new() -> McClient
pub fn with_target(self, target: ConnectionTarget) -> McClient
pub const fn with_plc_series(self, series: PLCSeries) -> McClient
pub const fn with_protocol(self, protocol: Protocol) -> McClient
pub const fn with_monitoring_timer(self, timer: u16) -> McClient
pub fn with_client_name(self, name: impl Into<String>) -> McClient

// McClient: 非同期の公開 API
pub async fn read_words(&self, device: &str, count: u16) -> Result<serde_json::Value, MelsecError>
pub async fn read_words_as<T: FromWords>(&self, device: &str, count: u16) -> Result<Vec<T>, MelsecError>
pub async fn read_bits(&self, device: &str, count: u16) -> Result<serde_json::Value, MelsecError>
pub async fn write_words(&self, device: &str, values: &[u16]) -> Result<serde_json::Value, MelsecError>
pub async fn write_words_as<T: ToWords>(&self, device: &str, values: &[T]) -> Result<serde_json::Value, MelsecError>
pub async fn write_bits(&self, device: &str, values: &[bool]) -> Result<serde_json::Value, MelsecError>
pub async fn echo(&self, payload: &str) -> Result<String, MelsecError>

// Default impl
impl Default for McClient {
  fn default() -> Self
}

// FromWords / ToWords トレイト
pub trait FromWords: Sized {
  const WORDS: usize;
  fn from_words_slice(words: &[u16]) -> Result<(Self, usize), MelsecError>;
}

pub trait ToWords {
  const WORDS: usize;
  fn to_words(&self, out: &mut Vec<u16>);
}
```

注: 上記には内部的に使われる非公開メソッド（例: `send_and_recv_with_retry` や `check_response_end_code`）は含めていません。必要ならそれらも追加できます。

追加情報・参考
- 低レイヤーのフレームパース（`src/mc_frame.rs`）やコマンド仕様（`src/command_registry.rs` / `commands.toml`）は、特定コマンドの request/response のビルド/パース挙動を確認する際に参照してください。

このドキュメントは現行コードに合わせて随時更新してください。必要なら、各公開関数の完全シグネチャ一覧を自動で抽出して追記することもできます。
