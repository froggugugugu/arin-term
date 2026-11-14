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

### AppPaths(`crates/arin-core/src/paths/app_paths.rs`)

状態フォルダの置き場所。画面には渡さない(TS の型なし)。

| フィールド | 型 | 備考 |
| ---------- | -- | ---- |
| `state_dir` | `PathBuf` | 既定 `<home>/Library/Application Support/Arin` |
| `overridden` | `bool` | `ARIN_STATE_DIR` による差し替えかどうか |

## 重要なバリデーションルール

- `AppPaths::resolve`: 差し替え指定が相対パスなら `RelativeOverride`。空文字列は指定なし扱い。差し替えが無くホームが無い・空なら `HomeUnknown`、相対なら `RelativeHome`
- スモークの印(状態フォルダ直下の `smoke-ready`)は、`ARIN_SMOKE` が厳密に `1` かつ状態フォルダが差し替えられているときだけ書く
