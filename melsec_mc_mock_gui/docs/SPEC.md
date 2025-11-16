## melsec_mc_mock_gui 仕様書

このドキュメントは `melsec_mc_mock_gui` フロントエンドのコード（`src/main.ts`, `src/components/monitor.ts` 等）から抽出した動作仕様をまとめたものです。開発者向けの参照仕様として利用してください。

**概要:**
- **目的:** `melsec_mc_mock_gui` は Tauri ベースの GUI クライアントで、モックサーバ（melsec_mc_mock）への操作、デバイスマップの可視化、ログ表示を提供します。
- **主要ファイル:** `src/main.ts`, `src/components/monitor.ts`, `index.html`, `styles.css`。

**バックエンド API（Tauri invocations）:**
- **`start_mock`**: パラメータ `{ ip, tcpPort, udpPort, timAwaitMs }` を受ける。UI からモックサーバを起動するために呼び出す。
- **`stop_mock`**: モックサーバ停止を要求する。引数無し。
- **`start_monitor`**: パラメータ `{ target, intervalMs }` を受ける。バックエンド側でモニター配信（イベント）を開始させる。
- **`stop_monitor`**: モニター停止を要求する。引数無し。
- **`get_words`**: パラメータ `{ key, addr, count }` を受ける。指定アドレスから `count` 個のワード配列を返す（Array<number>）。
- **`set_words`**: パラメータ `{ key, addr, words }` を受ける。`words` は 16bit 単位の数値配列で、指定アドレスに書き込む。

**イベント（Tauri event 名）:**
- **`monitor`**: ペイロード `{ key, addr, vals }` を受け取る。`vals` はアドレス `addr` からのワード配列。受信時に UI 行を更新する。
- **`server-status`**: サーバ状態文字列（例えば `起動中`）を payload に持つ。受信すると画面上のステータス表示を更新する。

**UI 要素（主な ID / クラス）:**
- **入力 / ボタン:** `tcp-port`, `udp-port`, `tim-await`, `mock-toggle`（Start/Stop Mock）、`mon-target`（監視対象入力）、`mon-toggle`（監視 Start/Stop）、`auto-start-next`（次回自動起動チェック）
- **編集パネル:** `edit-modal`, `edit-modal-box`, `edit-modal-title`, `edit-value`, `edit-write`, `edit-cancel`, `.write-type[data-typ]`（書き込み形式ボタン）
- **テーブル:** `monitor-table`, `monitor-tbody`。各行 id は `row-<KEY>-<ADDR>`（例 `row-D-0`）。
- **ログ:** `monitor-log`（表示用 `pre` 要素）。
- **状態表示:** `server-status`。

**表示フォーマット（frontend でサポートしている表示形式）:**
- `U16`（符号無し16bit）
- `I16`（符号付き16bit）
- `HEX`（16進）
- `BIN`（2進）
- `ASCII`（2文字→1ワード）
- `U32`, `I32`, `F32`（32bit 値は偶数アドレス + 次のワードを組合せて表示）

注意: `U32`/`I32`/`F32` は「ペア」表現を使うため、偶数アドレス側に意味ある表示が出力され、奇数側は `paired-empty` として扱われます。

**モニタリング挙動:**
- アプリ起動時または対象変更時に初期行数 30 を用意する（`createInitialRows(key, addr, 30)`）。
- バックエンドのイベント API (`window.__TAURI__.event.listen`) が利用可能な場合はそれを利用して `monitor` イベントを購読する。
- イベント API が利用不可の場合は、フォールバックとして `startFallbackPolling(key, addr, intervalMs)` を使い、`get_words` を定期的に呼ぶ（UI 側の既定値は 500ms）。

**ターゲット解析ルール（`parseTarget`）:**
- 先頭の英字列がデバイスコード（例 `D`, `M`, `W` 等）、その後の部分がアドレス。
- 数字部分に `A-F` が含まれていれば 16 進数として解釈、そうでなければ 10 進数。例: `D10` → key=`D`, addr=10； `WFF` → key=`W`, addr=0xFF。
- 文字列のみ（数値無し）はパース失敗（`null`）とする。ただし `monitor.ts` 内には `WFF` のようなケースを扱う補正がある。

**書き込み（Edit Modal）の挙動:**
- 単語幅（16bit）の書き込みは `set_words` を直接呼ぶ（`{ key, addr, words: [value] }`）。
- 32bit タイプ（`U32/I32/F32`）は、偶数アドレスに揃えて 2 ワードを生成し、`set_words` で baseAddr（偶数）に書き込む。F32 はリトルエンディアンで DataView を使ってビット表現に変換する。
- ASCII は 2 文字を hi/lo に分割して 1 ワードに詰める。
- 書き込み後は UI を楽観的に更新（`setWordRow` を呼ぶ）が、ポップアップは自動で閉じない（ユーザ操作を維持）。

**ローカルストレージキー:**
- `displayFormat` : 現在の表示フォーマットを保存。
- `autoStartNext`  : 自動開始チェックの状態を保存（'1' が有効）。
- `editPopupPos`   : 編集ポップアップの位置 `{ left, top }` を保存。

**DOM / CSS クラス（意味）:**
- `bit-on` / `bit-off` : 各ビットセルの ON/OFF。
- `paired-empty` : 32bit ペア表示で片側が空の行に付与。
- `selected-row` : 選択中の行。

**エラー処理 / ロギング:**
- 多くの非同期呼び出しは try/catch で囲まれ、`monitor-log` にメッセージを追加するか `console` に出力する。バックエンド呼び出し失敗時はフォールバック（空行表示や polling）を利用する。

**開発者向けメモ:**
- フロントエンドのエントリポイント: `src/main.ts`（UI ロジック, ボタンハンドラ）
- モニタリング表示ロジック: `src/components/monitor.ts`（行生成、フォーマット、イベント購読、polling）
- テスト環境では `window.__TAURI__` が存在しないため `invoke` はテスト用にスタブ化されている。

**素早く動かす手順（Windows PowerShell）:**
```powershell
cd melsec_mc_mock_gui
npm install
npm run build
# 開発モード（tauri がセットアップ済みの場合）
# npm run tauri dev
```

**次の推奨作業候補:**
- `README.md` にこの SPEC のサマリをリンクする。
- `src/components/monitor.ts` の型付けを強化して可読性を向上する。
- `set_words` / `get_words` のバックエンド仕様（引数/戻り値）を Rust 側ソースと突合する（`src-tauri` または Rust クレート内）。

---
このファイルはコードの現在（`src/main.ts`, `src/components/monitor.ts`）をベースに自動抽出・手動整理しました。追加で Rust 側実装や `index.html` の DOM を参照して追記できます。
