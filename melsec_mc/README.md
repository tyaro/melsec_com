# melsec_mc

## 概要

`melsec_mc` は Mitsubishi MELSEC 系 PLC と通信するための Rust ライブラリ（コア実装）です。

## 主な機能

- PLC フレームの生成/解析
- 読み書きリクエストの構築とレスポンス処理
- 異なる MC プロトコルバリエーションのサポート

## サンプル（依存の例）

Cargo.toml に以下のように記述して利用できます（git 依存の例）:

```toml
[dependencies]
melsec_mc = { git = "https://github.com/tyaro/melsec_mc.git", branch = "main" }
```

## 使用例（概念）

```rust
use melsec_mc::client::McClient;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut c = McClient::new("192.0.2.1:5000")?;
    let words = c.read_words("D0", 10)?;
    println!("read: {:?}", words);
    Ok(())
}
```

## 開発について

実装・開発はこのモノレポ `melsec_com` 上で行っています。配布用（公開・クライアント向け）は [tyaro/melsec_mc](https://github.com/tyaro/melsec_mc) を参照してください。
