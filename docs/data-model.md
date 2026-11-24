# データモデル

## スキーマ配置

- Rust の型が正本(`crates/arin-core/src/`)。画面とやり取りする型は ts-rs で `src/ipc/generated/` に生成し、コミットする
- 永続化するのは `state.json`(版 1。読み書きは `crates/arin-core/src/persistence/codec.rs`、ファイルの入出力は `arin-platform/src/storage/state_file.rs`、保存のスレッドは `src-tauri/src/persistence.rs`)。設定ファイル `settings.toml` は人だけが書き、アプリは読むだけ(M4.5 は `[shortcuts]` だけ。下)。文字の大きさ(M4.6。ADR-013)は設定画面で変え、`state.json` に置く

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
| `ui.terminalFontSize` / `ui.interfaceFontSize` | 整数(CSS ピクセル) | ターミナルの文字(既定 15、9〜28)と画面の文字の基準値(既定 13、11〜18)。既定のあいだはキーごとに書かない。無い・数でないときは既定、整数でない数は四捨五入、範囲の外は端に留める。どれも退避しない(版は 1 のまま。規則は `layout/font_sizes.rs`) |
| `ui.windowTranslucency` / `ui.terminalTranslucency` | 整数(%) | ウィンドウの地(既定 45、0〜100)とターミナルの背景(既定 20、0〜80)の透過の度合い。5 刻み。既定のあいだはキーごとに書かない。無い・数でないときは既定、5 の倍数でない値は近い 5 の倍数に丸め、範囲の外は端に留める。どれも退避しない(M4.7。ADR-014。規則は `layout/translucency.rs`) |
| `ui.editorVim` | 真偽 | エディタの Vim のキー操作(M4.8。ADR-015 の 6)。真のときだけ書く。無い・真偽値でないときは偽(退避しない。版は 1 のまま)。「既定に戻す」(`reset_appearance`)では戻さない |
| `ui.theme` | 文字列 | テーマの選び方(ADR-016)。`"light"`・`"dark"` のときだけ書く。無い・知らない値はシステム(OS に追従。退避しない。版は 1 のまま)。「既定に戻す」(`reset_appearance`)では戻さない |
| `ui.checkUpdates` | 真偽 | 起動時に新しい版を GitHub の Releases に問い合わせるか(ADR-018)。既定は偽。真のときだけ書く。無い・真偽値でないときは偽(退避しない。版は 1 のまま) |

「見つからない」と表示用のパスは保存しない。エディタのタブも保存しない(再起動で開き直さない。ADR-015 の【要確認】2 の A)。

### settings.toml(M4.5。ADR-012)

状態フォルダ直下(`AppPaths::settings_file()`。`ARIN_STATE_DIR` で差し替えたら差し替え先だけを読む)。人だけが書き、アプリは書き換え・退避・削除をしない。例外は「設定ファイルを開く…」が**無いときだけ**見本(`arin-core/shortcuts/sample.rs`)を 0600 で作ること(あれば上書きしない)。読むのは `FsSettingsFile`(`arin-platform/src/storage/settings_file.rs`。TOML の解析もここ)、内容と問題は `SettingsStore`(`arin-core/settings`)、行の規則は `arin-core/shortcuts/overrides.rs`。

```toml
[shortcuts]
"terminal.new-tab" = "Cmd+Shift+T"
```

- 読む節は `[shortcuts]` だけ。ほかの節は黙って無視する(M6 の T56 が足す)
- 文字の大きさは設定画面で変え、設定ファイルには項目が無い(見本の先頭にその旨の 1 行)。「設定ファイルを開く…」はアプリ名のメニューのキーの無い項目と設定画面のボタン(コマンド `open_settings_file`)の 2 か所
- 行は `"コマンドの ID" = "キーの文字列"`。ID の一覧と既定のキーは生成物 `src/ipc/generated/defaultShortcuts.json` が正(大文字・小文字を区別する)。キーの書式は `KeyChord::parse`(正規の書式でなくてよい)。書かなかった行は既定、行を消せば既定に戻る。割り当てを外す書き方は無い(`""` は読めないキー)
- 引用符の無い点つきの ID(`terminal.new-tab = …`)と `[shortcuts.terminal]` の下の行は、入れ子の表を 1 段だけ `.` でつないで同じ ID として読む。2 段以上の入れ子は「文字列でない」
- 行の問題はその行だけ既定に戻し、残りは使う: 文字列でない・読めないキー・予約のキー(`reserved.rs`)・メニューの項目の行で `Cmd` を含まない・画面の行で `Cmd` を含まず修飾キーの無い `Enter`・`Space`・`Backspace`・`Delete`・`F1`〜`F12` でもない・重なり(関わる設定の行をすべて戻すのを無くなるまで繰り返す)・同じコマンドを 2 度(引用符の有無だけ違う 2 行)。知らない ID と `edit.cut`・`edit.copy`・`edit.paste`(変えられない)は無視する。どれも知らせる。受け口は変えられない
- ファイル全体の問題(構文エラー・UTF-8 でない・1 MiB を超える・ファイルでない・読む権限が無い・入出力の失敗)は、起動時は既定の表、動作中は直前の表のまま。`[shortcuts]` が表でなければその節だけ既定。ファイルが無ければ既定で知らせない。同じキーを 2 度書くのは構文エラー
- 知らせ(`get_settings_notice` の 1 行)と標準エラー(`[arin:settings]`)に出すのは、コマンドの ID・キーの文字列(制御文字を除き 64 文字で切る)・行と列・理由の種類だけ。行の全文・TOML のクレートの文言・絶対パスは出さない
- 保存は状態フォルダの監視(`src-tauri/src/settings_watch.rs`)で拾い、すぐ反映する。シンボリックリンクの先の変更は拾えない(リンクの先は読む)

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
| `fontSizes` | `FontSizesSnapshot` | `terminal`・`interface` のそれぞれに `value`・`min`・`max`・`defaultValue`(M4.6。画面に範囲と既定の数を写さない) |
| `translucency` | `TranslucencySnapshot` | `window`・`terminal` のそれぞれに `value`・`min`・`max`・`defaultValue`・`step`(M4.7) |
| `editors` | `EditorTabSnapshot[]` | エディタのタブ(M4.8。プロジェクトの並び順、その中は開いた順)。文書の本文と版は載せない(コマンドの応答で渡す) |
| `editor` | `EditorSettingsSnapshot` | `vim`(`ui.editorVim`) |
| `theme` | `ThemeMode` | `system`・`light`・`dark`(`ui.theme`。ADR-016) |
| `checkUpdates` | 真偽 | 起動時に新しい版を確かめるか(`ui.checkUpdates`。ADR-018) |

- `ProjectCardSnapshot`: `id`、`name`(表示名)、`color`、`displayPath`(ホームの中は `~` で始める)、`missing`、`showHidden`、`terminalIds`(タブの順)、`activeTerminalId`(0 枚なら `null`)、`editorIds`(開いた順)、`shownEditorId`(タブ表示でターミナルの代わりに出すエディタ。ターミナルのタブを選ぶ・作ると `null`)
- `EditorTabSnapshot`(タブと版の型は `crates/arin-core/src/editor/`): `id`(`"editor-<n>"`)、`projectId`、`path`(`RelPath`)、`label`(ファイル名)、`dirty`(未保存。画面が `set_document_dirty` で送る。Rust は保存でも消さない)、`readOnly`(`hard-link`・`not-owner`・`not-writable`・`folder-not-writable` / `null`)
- 文書の版(`FileStamp`。画面には渡さない): `{ stat: { dev, ino, size, mtime_ns }, digest }`。`digest` は本文の `DefaultHasher` の 64 ビット(変更の検出用。暗号学的ではない)。stat の 4 つが同じなら中身を比べない
- `TerminalTabSnapshot`: `id`、`projectId`(いまは必ず値がある。型は `null` を取りうるまま)、`label`(「ターミナル N」)、`status`、`attention`(フォーカス中でないあいだに BEL・OSC 9・OSC 777 があった。フォーカス中になると消える。保存しない)
- `DisplaySnapshot`(`crates/arin-core/src/workspace/snapshot.rs`。規則は `workspace/display.rs`・`layout/tiles.rs`): `mode`(`"tabs"`/`"tiles"`)、`maximized`(タイル表示のときだけ真。拡大しているのは `focusedTerminalId`)、`page`(0 から。0 枚なら 0)、`pageCount`、`grid`(`cols`×`rows`。0 枚なら 0×0)、`tileIds`(表示中のページのタイル)、`focusedTerminalId`(=選択中のプロジェクトの表示中のタブ)、`attentionPages`(印のあるタイルのページ。昇順)。ページ・グリッド・並びはタブ表示のときも載せる。保存するのは `mode` だけ
- `TerminalStatus`(`crates/arin-core/src/terminal/state.rs`): `kind` で区別する: `notStarted` → `running` → `exited`(`code`・`signal`)/ `failed`(`message`。次の接続で再起動を試す)

## コマンドとイベント

正本は `src-tauri/src/lib.rs` の `generate_handler!`、画面側の型は `src/ipc/client.ts` の `CommandMap`・`EventMap`。

- アプリ: `get_app_snapshot`、`notify_app_ready`、`report_frontend_error`、`confirm_quit`、`open_settings_file`(引数なし。無いときだけ見本を作って既定のアプリで開く。メニューの「設定ファイルを開く…」と同じ処理)
- ターミナル: `connect_terminal`(応答 `ConnectOutcome` の `sizeApplied`)、`write_terminal`、`write_terminal_bytes`、`ack_terminal_output`、`resize_terminal`、`new_terminal`(選択中のプロジェクトに足し、ID を返す。`projectId` を渡すと、そのプロジェクトを選んでから足す)、`close_terminal`、`select_terminal`
- プロジェクト: `add_project_by_dialog`(応答 `added` / `alreadyRegistered` / `cancelled`。パスは画面を経由しない)、`select_project`、`remove_project`(`confirmed` が偽で動作中があれば `needsConfirmation` と動作中の一覧、ほかは `removed`)、`relink_project`(真偽)、`set_project_color`、`copy_project_path`(Rust がペーストボードに書く)
- ペイン: `set_pane_sizes`、`set_sidebar_collapsed`
- 表示(M4): `set_display_mode`、`focus_terminal`(閉じたタイルの ID なら何もしない)、`next_tile_page`・`previous_tile_page`(タブ表示・拡大中・端のページでは何もしない)、`toggle_tile_maximize`(`terminalId` は任意。渡せばそのタイルにフォーカスしてから拡大、無ければフォーカス中のタイル。タブ表示では何もしない)
- 文字の大きさ(M4.6): `step_font_size`(`target` は `terminal`/`interface`、`delta` だけ動かす。端で留め、変わったときだけ写しを送る)、`reset_appearance`(文字の大きさ 2 つと透過 2 つを既定に。M4.7 で `reset_font_sizes` から替えた)
- 透過(M4.7): `step_translucency`(`target` は `window`/`terminal`、`delta` は % の差分。5 の倍数に丸めて端で留める)。タイルの番号キーにコマンドは無い(メニューの項目が `focus_tile_at` を呼ぶ)
- テーマ(ADR-016): `set_theme({theme})`(`system`・`light`・`dark`。変わったら写しを送り、Rust がアプリの外観に当てる。知らない文字列は引数の読み取りで失敗する)
- ツリー: `list_directory`(応答 `DirListing`)、`set_show_hidden`
- ファイル操作(応答 `FileOpOutcome`): `create_entry`、`rename_entry`、`move_entry`、`copy_entry`、`trash_entry`、`copy_entries`、`cut_entries`、`paste_entries`
- デスクトップ: `reveal_entry`、`open_entry`(ふつうのファイルだけ。実行の権限があるもの・`UNOPENABLE_EXTENSIONS` の拡張子は開かない。`src-tauri/src/commands/desktop.rs`)、`copy_entry_path`、`insert_entry_path`(ターミナルへ書く文字列は Rust の `insertion_text` が作る)
- ショートカット: `get_shortcuts`(応答 `ShortcutTable`。設定ファイルの値を当てた表)、`get_settings_notice`(設定の知らせの 1 行か `null`。写しの `notice` とは別)
- エディタ(M4.8。ADR-015 の 4 と末尾の追記。`src-tauri/src/commands/editor.rs`): `open_document({projectId, path})`(応答 `OpenDocumentOutcome`: `opened`(本文は BOM を外し、`lineEnding` は `lf`/`crlf`)/ `already-open` / `not-allowed`(`not-markdown`・`link`・`directory`・`too-large`・`not-text`・`outside-root`)/ `not-found` / `failed`)、`save_document({editorId, text, mode})`(`mode` は `normal`・`overwrite`・`create`。`normal` は外で変わっていれば `changed-outside`、`overwrite` は変わっていても置き換えるがファイルが無ければ `missing`、`create` は同じ名前があれば置き換えず `changed-outside`。ほかに `saved`・`read-only`・`too-large`・`failed`)、`check_document({editorId, withText})`(`withText` は `never`・`if-changed`・`always`。**Rust の状態を変えない**。応答 `unchanged` / `changed { text, lineEnding, token }` / `missing`)、`adopt_document({editorId, token})`(画面が `changed` の本文を当てたあとにだけ呼ぶ。Rust はここで版を持ち直す)、`set_document_dirty({editorId, dirty})`(変わり目だけ)、`select_editor`、`close_editor({editorId, discard})`(`closed` / `needs-confirmation`)、`set_editor_vim({enabled})`
- `remove_project` の `needsConfirmation` と `QuitRequest` に `unsaved: UnsavedEditor[]`(`editorId`・`projectName`・`path`)。動作中のターミナルが無くても未保存のエディタがあれば確認する
- ショートカットの表は 33 行(M4.8 で `editor.save` = `Cmd+S`、受け口 `screen` を足した)
- 引数のパスはプロジェクトの ID と `RelPath`(ルートからの相対パス)。絶対パスは画面に渡さない
- イベント: `app-snapshot`(状態が変わるたびに `AppSnapshot` 全体)、`tree-changed`(`TreeChange { projectId, dirs, fullReload }`)、`menu-command`(`MenuCommand`。`"select-all"` と `"open-settings"`(「設定…」。画面が設定画面を開く))、`quit-requested`(`QuitRequest`。動作中のターミナルの一覧。画面が確認したら `confirm_quit`)、`settings-changed`(中身なし。設定の表か知らせが変わった。画面は `get_shortcuts` と `get_settings_notice` を呼び直す)、`editor-close-requested`(`EditorCloseRequest { editorId }`。⌘W で表示中のエディタが未保存のとき、Rust は閉じずにこれを送る。画面が確認して `close_editor` を呼ぶ)。出力はイベントではなくチャネルで生のバイト列を送る
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
- 文書(`crates/arin-core/src/editor/document.rs`・`arin-platform/src/fs/document.rs`): 拡張子 `.md`・`.markdown`(大文字・小文字を問わない)、ふつうのファイル(リンク自体は開かない)、1 MiB 以下、UTF-8 で NUL を含まない。改行は `\r\n` があれば `crlf`。保存の一時ファイルは同じフォルダの `.arin-save-<pid>-<連番>`(監視と一覧は `.arin-copy-` と同じに隠す。`tree/exclusion.rs` の `WORK_NAME_PREFIXES`)
- 監視の変更: 変わったフォルダが `MAX_CHANGED_DIRS`(1,000)を超えたら `fullReload`(`tree/changes.rs`)
- スモークの印(状態フォルダ直下の `smoke-ready`、最初の処理済みの通知で書く `smoke-terminal`)は、`ARIN_SMOKE` が厳密に `1` かつ状態フォルダが差し替えられているときだけ書く
