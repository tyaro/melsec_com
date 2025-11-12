# Release artifacts

このドキュメントでは、GitHub Release に添付されるビルド成果物（ZIP）とそれらの命名規約、チェックサム検証手順を説明します。

## アセットの入手
- GitHub の Releases ページ（https://github.com/tyaro/melsec_com/releases）から目的のタグ（例: `v0.2.4`）を選び、添付アセットをダウンロードしてください。

## 命名規約
- アセットは次の形式で命名されます（例）:
  - `melsec_com-v{version}-linux-x86_64.zip`
  - `melsec_com-v{version}-linux-aarch64.zip`
  - `melsec_com-v{version}-macos-x86_64.zip`
  - `melsec_com-v{version}-macos-aarch64.zip`

- 各 ZIP には同名の SHA256 チェックサムファイルが付属します（例: `melsec_com-v0.2.4-linux-x86_64.zip.sha256`）。

## SHA256 チェックサム検証
- Linux / macOS:

```sh
# ファイルの SHA256 を表示
shasum -a 256 melsec_com-v0.2.4-linux-x86_64.zip
# または
sha256sum melsec_com-v0.2.4-linux-x86_64.zip

# 付属の .sha256 と照合する
sha256sum -c melsec_com-v0.2.4-linux-x86_64.zip.sha256
```

- Windows (PowerShell):

```powershell
Get-FileHash .\melsec_com-v0.2.4-linux-x86_64.zip -Algorithm SHA256 | Format-List
```

- チェックサムが一致すればアセットの整合性が確認できます。

## 備考
- リポジトリには生成物（`release_artifacts/`、`ci_artifacts_*` など）をコミットしない方針です。
- 追加で GPG 署名を行いたい場合は、署名の生成と公開鍵の配布方法を別途用意できます。