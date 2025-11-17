# 三菱電機 PLC 通信ライブラリ プロジェクト (melsec_com)

このリポジトリは下記の構成となります

- `melsec_mc` : 三菱電機の MC プロトコル（MC プロトコル3E/MC プロトコル4E）を扱う Rust ライブラリのコア部分
- `melsec_mc_mock` : `melsec_mc` を使った MC プロトコルのモックサーバー実装(PLC実機の代わり)
- `melsec_mc_mock_gui` : `melsec_mc_mock` の状態を GUI で操作するためのアプリケーション
  
各クレートの配布用リポジトリは以下の通りです:

- [melsec_mc](https://github.com/yourusername/melsec_mc)
- [melsec_mc_mock](https://github.com/yourusername/melsec_mc_mock)
- [melsec_mc_mock_gui](https://github.com/yourusername/melsec_mc_mock_gui)

## 開発について

このリポジトリはモノレポ構成となっており、各クレートの実装・開発はここで行っています。 配布用リポジトリは上記リンク先をご参照ください。

## ライセンス

このプロジェクトは MIT ライセンスの下でライセンスされています。詳細については、各クレートの `LICENSE` ファイルを参照してください。

## コントリビューション

バグ報告、機能リクエスト、プルリクエストは大歓迎です。貢献ガイドラインについては、各クレートの `CONTRIBUTING.md` ファイルを参照してください。

## コミュニティ

質問やディスカッションは、各クレートの GitHub リポジトリの Issues セクションを利用してください。
