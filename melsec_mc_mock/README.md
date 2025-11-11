# melsec_mc_mock

## 概要

`melsec_mc_mock` は `melsec_mc` コアを利用するモックサーバーです。開発やテスト用途に使用します。

## 主な機能

- 仮想 PLC ハンドラの提供
- テスト／デバッグ用の応答シミュレーション
- フレームレベルでの差分検証ツール

## 起動例（開発）

```powershell
cd melsec_mc_mock
cargo run --release --bin mock-server -- --listen 127.0.0.1:5000
```

## 開発について

実装・開発はこのモノレポ `melsec_com` で行っています。配布用リポジトリは [tyaro/melsec_mc_mock](https://github.com/tyaro/melsec_mc_mock) を参照してください。
