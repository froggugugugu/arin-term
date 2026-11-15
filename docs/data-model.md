# データモデル

## スキーマ配置

- Rust の型が正本(`crates/arin-core/src/`)。画面とやり取りする型は ts-rs で `src/ipc/generated/` に生成し、コミットする
- 永続化するのは `state.json`(版 1。読み書きは `crates/arin-core/src/persistence/codec.rs`、ファイルの入出力は `arin-platform/src/storage/state_file.rs`、保存のスレッドは `src-tauri/src/persistence.rs`)。設定ファイルは未実装(M6)

## スキーマ詳細

### state.json(版 1)

状態フォルダ直下。Rust だけが書く。

| キー | 型 | 備考 |
| ---- | -- | ---- |
| `schemaVersion` | 整数 | `STATE_SCHEMA_VERSION`(1) |
| `projects[]` | 配列 | 配列の順が並び順 |
| `projects[].id` | 文字列 | UUID の形(`8-4-4-4-12` の 16 進。読むときに小文字にそろえる。版のビットは見ない) |
| `projects[].bookmark` | base64 の文字列 / `null` | `arin-core` は中身を見ない。`null` なら起動時に `lastKnownPath` から作り直す |
| `projects[].lastKnownPath` | 文字列 | 最後に分かっていた絶対パス |
| `projects[].customName` | 文字列 / `null` | `null` はフォルダ名に追従 |
| `projects[].tree.showHidden` | 真偽 | 隠しファイルの表示。ON のときだけ書く。無ければ OFF |
| `projects[].colorId` | 文字列 | `red`・`orange`・`yellow`・`green`・`teal`・`blue`・`purple`・`pink`(パレット順。色の値は `src/ui/design/palette.ts`) |
| `ui.selectedProjectId` | 文字列 / `null` | |
| `ui.sidebarWidth` / `ui.cardAreaHeight` | 数値(CSS ピクセル) | 無ければ書かない |
| `ui.sidebarCollapsed` | 真偽 | |
| `ui.displayMode` | `"tiles"` | タイル表示のときだけ書く。無い・知らない値・型が違うときは退避せずタブ表示(版は 1 のまま) |

「見つからない」と表示用のパスは保存しない。

### AppSnapshot(`crates/arin-core/src/workspace/snapshot.rs`)

Rust から画面へ渡す状態の写し。画面は届いたもので丸ごと置き換える。JSON は camelCase。絶対パスとブックマークは載せない。

| フィールド | 型 | 備考 |
| ---------- | -- | ---- |
| `schemaVersion` | `u32` / `number` | `SNAPSHOT_SCHEMA_VERSION`(現在 1)。項目の意味を変える・消すときに上げる(追加では上げない) |
| `appVersion` | `String` / `string` | アプリの版(`src-tauri` が package_info から渡す) |
| `projects` | `ProjectCardSnapshot[]` | 並び順 |
| `selectedProjectId` | `ProjectId` / `null` | |
| `terminals` | `TerminalTabSnapshot[]` | プロジェクトの並び順、その中はタブの順 |
| `layout` | `PaneSizes` | 保存値(`sidebarWidth`・`sidebarCollapsed`・`cardAreaHeight`)。表示値への調整は画面 |
| `notice` | 文字列 / `null` | 状態ファイルを退避したときの 1 行の知らせ |
| `display` | `DisplaySnapshot` | 右ペインの表示の状態(下)。グリッドとページは Rust が計算し、画面は映すだけ |

- `ProjectCardSnapshot`: `id`、`name`(表示名)、`color`、`displayPath`(ホームの中は `~` で始める)、`missing`、`showHidden`、`terminalIds`(タブの順)、`activeTerminalId`(0 枚なら `null`)
- `TerminalTabSnapshot`: `id`、`projectId`(いまは必ず値がある。型は `null` を取りうるまま)、`label`(「ターミナル N」)、`status`、`attention`(フォーカス中でないあいだに BEL・OSC 9・OSC 777 があった。フォーカス中になると消える。保存しない)
- `DisplaySnapshot`(`crates/arin-core/src/workspace/snapshot.rs`。規則は `workspace/display.rs`・`layout/tiles.rs`): `mode`(`"tabs"`/`"tiles"`)、`maximized`(タイル表示のときだけ真。拡大しているのは `focusedTerminalId`)、`page`(0 から。0 枚なら 0)、`pageCount`、`grid`(`cols`×`rows`。0 枚なら 0×0)、`tileIds`(表示中のページのタイル)、`focusedTerminalId`(=選択中のプロジェクトの表示中のタブ)、`attentionPages`(印のあるタイルのページ。昇順)。ページ・グリッド・並びはタブ表示のときも載せる。保存するのは `mode` だけ
- `TerminalStatus`(`crates/arin-core/src/terminal/state.rs`): `kind` で区別する: `notStarted` → `running` → `exited`(`code`・`signal`)/ `failed`(`message`。次の接続で再起動を試す)

## コマンドとイベント

正本は `src-tauri/src/lib.rs` の `generate_handler!`、画面側の型は `src/ipc/client.ts` の `CommandMap`・`EventMap`。

- アプリ: `get_app_snapshot`、`notify_app_ready`、`report_frontend_error`、`confirm_quit`
- ターミナル: `connect_terminal`(応答 `ConnectOutcome` の `sizeApplied`)、`write_terminal`、`write_terminal_bytes`、`ack_terminal_output`、`resize_terminal`、`new_terminal`(選択中のプロジェクトに足し、ID を返す)、`close_terminal`、`select_terminal`
- プロジェクト: `add_project_by_dialog`(応答 `added` / `alreadyRegistered` / `cancelled`。パスは画面を経由しない)、`select_project`、`remove_project`(`confirmed` が偽で動作中があれば `needsConfirmation` と動作中の一覧、ほかは `removed`)、`relink_project`(真偽)、`set_project_color`、`copy_project_path`(Rust がペーストボードに書く)
- ペイン: `set_pane_sizes`、`set_sidebar_collapsed`
- 表示(M4): `set_display_mode`、`focus_terminal`(閉じたタイルの ID なら何もしない)、`next_tile_page`・`previous_tile_page`(タブ表示・拡大中・端のページでは何もしない)、`toggle_tile_maximize`(`terminalId` は任意。渡せばそのタイルにフォーカスしてから拡大、無ければフォーカス中のタイル。タブ表示では何もしない)
- ツリー: `list_directory`(応答 `DirListing`)、`set_show_hidden`
- ファイル操作(応答 `FileOpOutcome`): `create_entry`、`rename_entry`、`move_entry`、`copy_entry`、`trash_entry`、`copy_entries`、`cut_entries`、`paste_entries`
- デスクトップ: `reveal_entry`、`open_entry`(ふつうのファイルだけ。実行の権限があるもの・`UNOPENABLE_EXTENSIONS` の拡張子は開かない。`src-tauri/src/commands/desktop.rs`)、`copy_entry_path`、`insert_entry_path`(ターミナルへ書く文字列は Rust の `insertion_text` が作る)
- ショートカット: `get_shortcuts`(応答 `ShortcutTable`)
- 引数のパスはプロジェクトの ID と `RelPath`(ルートからの相対パス)。絶対パスは画面に渡さない
- イベント: `app-snapshot`(状態が変わるたびに `AppSnapshot` 全体)、`tree-changed`(`TreeChange { projectId, dirs, fullReload }`)、`menu-command`(`MenuCommand`。いまは `"select-all"` だけ)、`quit-requested`(`QuitRequest`。動作中のターミナルの一覧。画面が確認したら `confirm_quit`)。出力はイベントではなくチャネルで生のバイト列を送る
- Finder からのドロップはコマンドではなく `src-tauri/src/lifecycle.rs` が受ける

### AppPaths(`crates/arin-core/src/paths/app_paths.rs`)

状態フォルダの置き場所。画面には渡さない(TS の型なし)。

| フィールド | 型 | 備考 |
| ---------- | -- | ---- |
| `state_dir` | `PathBuf` | 既定 `<home>/Library/Application Support/Arin` |
| `overridden` | `bool` | `ARIN_STATE_DIR` による差し替えかどうか |

## 重要なバリデーションルール

- `AppPaths::resolve`: 差し替え指定が相対パスなら `RelativeOverride`。空文字列は指定なし扱い。差し替えが無くホームが無い・空なら `HomeUnknown`、相対なら `RelativeHome`
- ターミナルの寸法(`validate_size`): 行は 1〜`MAX_ROWS`(1,000)、桁は 1〜`MAX_COLS`(2,000)。外れたら `InvalidSize`
- ペインの寸法(`PaneSizes::sanitized`、`crates/arin-core/src/layout/panes.rs`): 数でない・無限大・負は初期値(幅 260、カード領域 200)、最小(幅 180、カード領域 72)未満は最小。上限は持たない(画面が表示値で留める)。最小値は `src/logic/panes/paneLayout.ts` と揃える
- `state.json` の読み込み: 欠けたキーは既定値、未知のキーは無視。JSON でない・最上位がオブジェクトでない・`schemaVersion` が無い/1 以上の整数でない/新しすぎる・形が違うときは `state.json.broken-<UTC の日時>`(`20261115T093000Z`。同名があれば `-2`、`-3`)に改名して空で始める。同じ ID は先のものだけ残し、一覧に無い選択は外す
- プロジェクトのパス(`normalize_root`): 絶対パス・UTF-8・`..` を含まない・ゴミ箱の中でない。同じフォルダの二重登録は `AlreadyRegistered`
- 相対パス(`RelPath`): 規則は `crates/arin-core/src/paths/relative.rs`。名前の規則は `fileops/name.rs`、ツリーの除外は `tree/exclusion.rs`
- 監視の変更: 変わったフォルダが `MAX_CHANGED_DIRS`(1,000)を超えたら `fullReload`(`tree/changes.rs`)
- スモークの印(状態フォルダ直下の `smoke-ready`、最初の処理済みの通知で書く `smoke-terminal`)は、`ARIN_SMOKE` が厳密に `1` かつ状態フォルダが差し替えられているときだけ書く
