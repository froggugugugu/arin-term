# データモデル

## スキーマ配置

- Rust の型が正本(`crates/arin-core/src/`)。画面とやり取りする型は ts-rs で `src/ipc/generated/` に生成し、コミットする
- 永続化するのは `state.json`(版 1。読み書きは `crates/arin-core/src/persistence/codec.rs`、ファイルの入出力は `arin-platform/src/storage/state_file.rs`、保存のスレッドは `src-tauri/src/persistence.rs`)。設定ファイルは未実装(M6)

## スキーマ詳細

### state.json(版 1)

状態フォルダ直下。Rust だけが書く。9 枚の検証画面(`ARIN_VERIFY_GRID=1`)では読まず書かない。

| キー | 型 | 備考 |
| ---- | -- | ---- |
| `schemaVersion` | 整数 | `STATE_SCHEMA_VERSION`(1) |
| `projects[]` | 配列 | 配列の順が並び順 |
| `projects[].id` | 文字列 | UUID の形(`8-4-4-4-12` の 16 進。読むときに小文字にそろえる。版のビットは見ない) |
| `projects[].bookmark` | base64 の文字列 / `null` | `arin-core` は中身を見ない。`null` なら起動時に `lastKnownPath` から作り直す |
| `projects[].lastKnownPath` | 文字列 | 最後に分かっていた絶対パス |
| `projects[].customName` | 文字列 / `null` | `null` はフォルダ名に追従 |
| `projects[].colorId` | 文字列 | `red`・`orange`・`yellow`・`green`・`teal`・`blue`・`purple`・`pink`(パレット順。色の値は `src/ui/design/palette.ts`) |
| `ui.selectedProjectId` | 文字列 / `null` | |
| `ui.sidebarWidth` / `ui.cardAreaHeight` | 数値(CSS ピクセル) | 無ければ書かない |
| `ui.sidebarCollapsed` | 真偽 | |

「見つからない」と表示用のパスは保存しない。

### AppSnapshot(`crates/arin-core/src/workspace/snapshot.rs`)

Rust から画面へ渡す状態の写し。画面は届いたもので丸ごと置き換える。JSON は camelCase。絶対パスとブックマークは載せない。

| フィールド | 型 | 備考 |
| ---------- | -- | ---- |
| `schemaVersion` | `u32` / `number` | `SNAPSHOT_SCHEMA_VERSION`(現在 1)。項目の意味を変える・消すときに上げる(追加では上げない) |
| `appVersion` | `String` / `string` | アプリの版(`src-tauri` が package_info から渡す) |
| `projects` | `ProjectCardSnapshot[]` | 並び順。検証画面では空 |
| `selectedProjectId` | `ProjectId` / `null` | |
| `terminals` | `TerminalTabSnapshot[]` | プロジェクトの並び順、その中はタブの順。持ち主の無いもの(検証画面)は最後 |
| `layout` | `PaneSizes` | 保存値(`sidebarWidth`・`sidebarCollapsed`・`cardAreaHeight`)。表示値への調整は画面 |
| `notice` | 文字列 / `null` | 状態ファイルを退避したときの 1 行の知らせ |

- `ProjectCardSnapshot`: `id`、`name`(表示名)、`color`、`displayPath`(ホームの中は `~` で始める)、`missing`、`terminalIds`(タブの順)、`activeTerminalId`(0 枚なら `null`)
- `TerminalTabSnapshot`: `id`、`projectId`(検証画面では `null`)、`label`(「ターミナル N」)、`status`
- `TerminalStatus`(`crates/arin-core/src/terminal/state.rs`): `kind` で区別する: `notStarted` → `running` → `exited`(`code`・`signal`)/ `failed`(`message`。次の接続で再起動を試す)

### LaunchOptions(`crates/arin-core/src/launch_options.rs`)

起動時の指定(M1 の検証用)。写しには混ぜず、画面は `get_launch_options` で起動時に 1 回だけ受け取る。

| フィールド | 型 | 備考 |
| ---------- | -- | ---- |
| `renderer` | `"dom"` / `"webgl"` | `ARIN_RENDERER`。既定 `dom` |
| `unicode` | `"unicode11"` / `"graphemes"` | `ARIN_UNICODE`。既定 `unicode11` |
| `verifyGrid` | `bool` | `ARIN_VERIFY_GRID`(`0`/`1`)。既定 `false` |

## コマンドとイベント

正本は `src-tauri/src/lib.rs` の `generate_handler!`、画面側の型は `src/ipc/client.ts` の `CommandMap`・`EventMap`。

- アプリ: `get_app_snapshot`、`get_launch_options`、`notify_app_ready`、`report_frontend_error`、`confirm_quit`
- ターミナル: `connect_terminal`(応答 `ConnectOutcome` の `sizeApplied`)、`write_terminal`、`write_terminal_bytes`、`ack_terminal_output`、`resize_terminal`、`new_terminal`(選択中のプロジェクトに足し、ID を返す)、`close_terminal`、`select_terminal`
- プロジェクト: `add_project_by_dialog`(応答 `added` / `alreadyRegistered` / `cancelled`。パスは画面を経由しない)、`select_project`、`remove_project`(`confirmed` が偽で動作中があれば `needsConfirmation` と動作中の一覧、ほかは `removed`)、`relink_project`(真偽)、`set_project_color`、`copy_project_path`(Rust がペーストボードに書く)
- ペイン: `set_pane_sizes`、`set_sidebar_collapsed`
- イベント: `app-snapshot`(状態が変わるたびに `AppSnapshot` 全体)、`menu-command`(`MenuCommand`。いまは `"select-all"` だけ)、`quit-requested`(`QuitRequest`。動作中のターミナルの一覧。画面が確認したら `confirm_quit`)。出力はイベントではなくチャネルで生のバイト列を送る
- Finder からのドロップはコマンドではなく `src-tauri/src/lifecycle.rs` が受ける

### AppPaths(`crates/arin-core/src/paths/app_paths.rs`)

状態フォルダの置き場所。画面には渡さない(TS の型なし)。

| フィールド | 型 | 備考 |
| ---------- | -- | ---- |
| `state_dir` | `PathBuf` | 既定 `<home>/Library/Application Support/Arin` |
| `overridden` | `bool` | `ARIN_STATE_DIR` による差し替えかどうか |

## 重要なバリデーションルール

- `AppPaths::resolve`: 差し替え指定が相対パスなら `RelativeOverride`。空文字列は指定なし扱い。差し替えが無くホームが無い・空なら `HomeUnknown`、相対なら `RelativeHome`
- `LaunchOptions::resolve`: 空・未指定は既定。大文字小文字は区別しない。前後の空白や不正な値は既定に戻し、標準エラーに `[arin:launch]` で 1 行
- ターミナルの寸法(`validate_size`): 行は 1〜`MAX_ROWS`(1,000)、桁は 1〜`MAX_COLS`(2,000)。外れたら `InvalidSize`
- ペインの寸法(`PaneSizes::sanitized`、`crates/arin-core/src/layout/panes.rs`): 数でない・無限大・負は初期値(幅 260、カード領域 200)、最小(幅 180、カード領域 72)未満は最小。上限は持たない(画面が表示値で留める)。最小値は `src/logic/panes/paneLayout.ts` と揃える
- `state.json` の読み込み: 欠けたキーは既定値、未知のキーは無視。JSON でない・最上位がオブジェクトでない・`schemaVersion` が無い/1 以上の整数でない/新しすぎる・形が違うときは `state.json.broken-<UTC の日時>`(`20261115T093000Z`。同名があれば `-2`、`-3`)に改名して空で始める。同じ ID は先のものだけ残し、一覧に無い選択は外す
- プロジェクトのパス(`normalize_root`): 絶対パス・UTF-8・`..` を含まない・ゴミ箱の中でない。同じフォルダの二重登録は `AlreadyRegistered`
- スモークの印(状態フォルダ直下の `smoke-ready`、最初の処理済みの通知で書く `smoke-terminal`)は、`ARIN_SMOKE` が厳密に `1` かつ状態フォルダが差し替えられているときだけ書く
