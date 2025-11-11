# melsec_mc_mock_gui

## 概要

`melsec_mc_mock_gui` は `melsec_mc_mock` の GUI クライアント（Tauri ベース）です。テスト用 UI や操作パネルを提供します。

## 主な機能

- モックサーバーへの接続と操作
- デバイスマップの可視化
- ログの表示とエクスポート

## 開発・起動（開発環境）

```powershell
cd melsec_mc_mock_gui
npm install
npm run build
# または tauri 開発モード
npm run tauri dev
```

## build 時の注意

MELSEC MC Rust クレートのビルド成果物（バイナリやライブラリ）は `target` ディレクトリに出力されます。
この `target` ディレクトリはプロジェクトごとに生成され、多くのファイルが含まれるため、Git リポジトリに含めるべきではありません。
そのため、`.gitignore` ファイルに `target/` を追加して、Git がこのディレクトリを無視するように設定しています。

1. **Git のキャッシュから `target` ディレクトリを削除**:

    ```bash
    git rm -r --cached --ignore-unmatch melsec_mc_mock_gui/target
    ```

2. **`.gitignore` ファイルを更新**:

    ```plaintext
    target/
    ```

3. **変更をコミット**:

    ```bash
    git add .gitignore
    git commit -m "Stop tracking melsec_mc_mock_gui/target and update .gitignore"
    ```

これにより、今後 `target` ディレクトリが Git によって追跡されなくなります。

配布用リポジトリは [tyaro/melsec_mc_mock_gui](https://github.com/tyaro/melsec_mc_mock_gui) を参照してください。
