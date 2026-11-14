# データモデル

## スキーマ配置

- Rust の型が正本(`crates/arin-core/src/`)。画面とやり取りする型は ts-rs で `src/ipc/generated/` に生成し、コミットする
- 永続化するスキーマ(`state.json`・設定ファイル)は未実装。M2・M6 で追加

## スキーマ詳細

### AppSnapshot(`crates/arin-core/src/workspace/snapshot.rs`)

Rust から画面へ渡す状態の写し。画面は届いたもので丸ごと置き換える。JSON は camelCase。

| フィールド | 型 | 備考 |
| ---------- | -- | ---- |
| `schemaVersion` | `u32` / `number` | `SNAPSHOT_SCHEMA_VERSION`(現在 1)。項目の意味を変える・消すときに上げる |
| `appVersion` | `String` / `string` | アプリの版(`src-tauri` が package_info から渡す) |
| `terminals` | `Vec<TerminalSnapshot>` | ターミナルの一覧(M1 は 1 枚、`ARIN_VERIFY_GRID=1` なら 9 枚)。追加時に版は上げていない |

### TerminalSnapshot・TerminalStatus(`crates/arin-core/src/terminal/state.rs`)

| フィールド | 型 | 備考 |
| ---------- | -- | ---- |
| `id` | `TerminalId`(文字列) | コマンドで指す識別子 |
| `status` | `TerminalStatus` | `kind` で区別する: `notStarted` → `running` → `exited`(`code`・`signal`)/ `failed`(`message`。次の接続で再起動を試す) |

### LaunchOptions(`crates/arin-core/src/launch_options.rs`)

起動時の指定(M1 の検証用)。写しには混ぜず、画面は `get_launch_options` で起動時に 1 回だけ受け取る。

| フィールド | 型 | 備考 |
| ---------- | -- | ---- |
| `renderer` | `"dom"` / `"webgl"` | `ARIN_RENDERER`。既定 `dom` |
| `unicode` | `"unicode11"` / `"graphemes"` | `ARIN_UNICODE`。既定 `unicode11` |
| `verifyGrid` | `bool` | `ARIN_VERIFY_GRID`(`0`/`1`)。既定 `false` |

## コマンドとイベント

- コマンド(`src-tauri/src/lib.rs` の `generate_handler!`): `get_app_snapshot`、`get_launch_options`、`notify_app_ready`、`report_frontend_error`、`connect_terminal`、`write_terminal`、`ack_terminal_output`、`resize_terminal`。画面側の型は `src/ipc/client.ts` の `CommandMap`
- イベント: `app-snapshot`(状態が変わるたびに `AppSnapshot` 全体)。出力はイベントではなくチャネルで生のバイト列を送る

### AppPaths(`crates/arin-core/src/paths/app_paths.rs`)

状態フォルダの置き場所。画面には渡さない(TS の型なし)。

| フィールド | 型 | 備考 |
| ---------- | -- | ---- |
| `state_dir` | `PathBuf` | 既定 `<home>/Library/Application Support/Arin` |
| `overridden` | `bool` | `ARIN_STATE_DIR` による差し替えかどうか |

## 重要なバリデーションルール

- `AppPaths::resolve`: 差し替え指定が相対パスなら `RelativeOverride`。空文字列は指定なし扱い。差し替えが無くホームが無い・空なら `HomeUnknown`、相対なら `RelativeHome`
- `LaunchOptions::resolve`: 空・未指定は既定。大文字小文字は区別しない。前後の空白や不正な値は既定に戻し、標準エラーに `[arin:launch]` で 1 行
- ターミナルの寸法(`validate_size`): 行と桁は 1 以上(0 なら `InvalidSize`)
- スモークの印(状態フォルダ直下の `smoke-ready`、最初の処理済みの通知で書く `smoke-terminal`)は、`ARIN_SMOKE` が厳密に `1` かつ状態フォルダが差し替えられているときだけ書く
