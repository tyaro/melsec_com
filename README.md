# melsec_com

配布用リポジトリ（各クレート）:

- [melsec_mc](https://github.com/tyaro/melsec_mc) (core library, 通信ライブラリ)
- [melsec_mc_mock](https://github.com/tyaro/melsec_mc_mock) (モック/サーバー)
- [melsec_mc_mock_gui](https://github.com/tyaro/melsec_mc_mock_gui) (GUI / Tauri ベース)

---

# build 時の注意

MELSEC MC Rust クレートのビルド成果物（バイナリやライブラリ）は `target` ディレクトリに出力されます。
この `target` ディレクトリはプロジェクトごとに生成され、多くのファイルが含まれるため、Git リポジトリに含めるべきではありません。
そのため、`.gitignore` ファイルに `target/` を追加して、Git がこのディレクトリを無視するように設定しています。
もし、既にリポジトリに `target` ディレクトリが含まれている場合は、以下の手順で対応してください。

1. **Git のキャッシュから `target` ディレクトリを削除**:

    ```bash
    git rm -r --cached --ignore-unmatch melsec_mc_mock/target
    ```

2. **`.gitignore` ファイルを更新**:

    ```plaintext
    target/
    ```

3. **変更をコミット**:
  
    ```bash
    git add .gitignore
    git commit -m "Stop tracking melsec_mc_mock/target and update .gitignore"
    ```

これにより、今後 `target` ディレクトリが Git によって追跡されなくなります。

## ワークスペースルートでのビルド方法

ワークスペースルートでは以下のコマンドを実行して、MELSEC MC クレートをビルドします。

```powershell
cargo build -p melsec_mc --target-dir melsec_mc/target
cargo build -p melsec_mc_mock --target-dir melsec_mc_mock/target
cargo build -p melsec_mc_examples --target-dir melsec_mc_examples/target
```
