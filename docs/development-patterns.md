# 開発パターン集

プロジェクト固有のコード規約・落とし穴だけをまとめる。構成・スキーマは `docs/architecture.md`、`docs/data-model.md` を参照。

---

## 1. 境界の規則(`npm run lint` が検査)

- `@tauri-apps/*` を import できるのは `src/ipc/` だけ。`@xterm/*` を import できるのは `src/terminal/xterm/` だけ。`codemirror`・`@codemirror/*`・`@lezer/*`・`@replit/*` を import できるのは `src/editor/codemirror/` だけ(M4.8)。`src/ui/` は `src/ipc/`・`src/terminal/xterm/`・`src/editor/codemirror/` を import しない(操作は `src/state`・`src/terminal`・`src/editor` 経由)。`src/terminal/xterm/`・`src/editor/codemirror/` は `src/ipc/` を import しない
- `arin-core` は `std::fs`・`std::process`・`std::net` を使わない。依存は serde・serde_json・thiserror・ts-rs だけ。入出力は `arin-platform` に置く
- 環境変数を読むのは `src-tauri/src/launch.rs` だけ。`arin-core` は値を引数で受け取る
- 画面のソースに通信の API(`fetch(` など)と HTML の差し込み(`innerHTML`・`dangerouslySetInnerHTML` など)を書かない
- CSP(`tauri.conf.json` の `csp`・`devCsp`): `connect-src` は IPC と自アプリだけ、スクリプトは `'self'`/`'none'` だけ、スタイルは `'self'`/`'none'`/`'unsafe-inline'` だけ
- xterm.js と CodeMirror の要素は React の管理の外に置く(ADR-003。エディタは `EditorManager` が持ち、タブを替えても取り消しの履歴とカーソルが残る)。CodeMirror は初めてエディタを開くときに動的 import で読む(静的に import しない)
- WebKit の標準の右クリックのメニューは出さない(「再読み込み」で画面が読み込み直され、ターミナルの表示が消える)。`src/ui/shell/contextMenuGuard.ts` が文書全体で止め、文字の入力欄(input・textarea・contenteditable。ターミナルの中は除く)だけ残す。右クリックのメニューが要る場所は、部品が `ContextMenu` を開いて `preventDefault` する
- プレビューは HTML の文字列を作らない。`markdownAst.ts` が自前の木(`markdownTypes.ts`)にし、`MarkdownPreview.tsx` が React の要素にする。`a[href]`・`img`・`input` を作らない。`style` 属性はプレビューの外側の要素 1 つ(文字の大きさ)だけ
- 画面から絶対パスを受け取らない・返さない(引数はプロジェクトの ID と相対パス)。ターミナルへ書く文字列は Rust が作る(`insertion_text`)
- ファイルシステムの書き込みには実パスを渡す(途中にシンボリックリンクがあると失敗する)。削除と上書きの経路を作らない(置き換えはゴミ箱へ入れてから)。**例外は文書の保存 `save_document` の 1 つだけ**(ADR-015 の 5): 開いた `.md`・`.markdown` のふつうのファイルを、本人の保存の操作のときだけ、同じフォルダの新しい一時ファイル(`.arin-save-…`)に書き終えてから改名で置き換える。外で変わっていれば本人が「上書き保存」を選ばない限り置き換えない。ハードリンク・書けない・持ち主が違うファイルは読み取り専用で開く。「作る」は `RENAME_EXCL`(同じ名前ができていたら置き換えない)。消すのは、その保存が作って使わなかった一時ファイルだけ(`TempFile::discard`。押さえたままの fd と同じ項目がその名前にあるときだけ。本人が 2026-11-17 に了承)。ほかの項目を消す経路は作らない

## 1.1 標準エラーの頭の文字列

| 頭 | 出すところ | 備考 |
| -- | ---------- | ---- |
| `[arin:frontend-error]` | `diagnostics.rs`(`report_frontend_error`) | CSP 違反・例外・読み込み失敗。常に有効。スモークは 1 行でもあれば失敗 |
| `[arin:verify]` | `verify_log.rs` | OSC 52 の回数と流量制御の集計。開発ビルド(`cfg(debug_assertions)`)だけ |
| `[arin:launch]` | `lib.rs`(`apply_window_effects`) | ウィンドウの半透過を付けられなかった(起動は続ける) |
| `[arin:state]` | `persistence.rs` | 状態ファイルを読めずに退避した・退避できなかった・読み取りや保存に失敗した |
| `[arin:fileops]` | `fileops.rs`(`report_staging_leftover`) | コピーの作業用フォルダ `.arin-copy-…` を片付けられずに残した(場所を出す。消す処理は無い) |
| `[arin:watch]` | `tree_watch.rs` | ツリーの監視を張れなかった |
| `[arin:editor]` | `commands/editor.rs`(`report_save_leftover`) | 保存の一時ファイル `.arin-save-…` を片付けられずに残した(場所を出す。ふつうは保存が自分で片付けるので出ない) |
| `[arin:settings]` | `settings.rs`(`report` と「設定ファイルを開く…」)、`settings_watch.rs` | 設定ファイルの問題(1 つにつき 1 行)、開く・見本を作る処理の失敗、監視を張れなかった・メニューを作り直せなかった。ID・キーの文字列・行と列・理由の種類だけで、行の全文とパスは出さない |

## 2. 状態管理

- zustand は `zustand/vanilla` の生成関数で作り、`main.tsx` から props で渡す。モジュールの読み込み時にストアを作らない
- `AppSnapshot` は画面で書き換えず、届いたもので丸ごと置き換える
- ショートカットのキーは表(`arin-core/shortcuts`、`src/logic/keys/shortcutTable.ts`)から引き、部品やメニューにキーの文字列を直書きしない。見本の表 `defaultShortcuts.ts` は生成物と同じ中身であることを `scripts/default-shortcuts.test.mjs` が確かめる。コマンドの ID の一覧は生成物 `src/ipc/generated/defaultShortcuts.json` を正とし、文書に写さない
- ショートカットの表は `menu::Shortcuts`(`RwLock`。`table()` は写しを返す)が 1 つだけ持ち、メニューと `get_shortcuts` が見る。差し替えたら `menu::rebuild`(`build_with_table` で作り直して `set_menu`)を**メインスレッドで**呼ぶ(ほかのスレッドからは `run_on_main_thread`)。項目ごとの `set_accelerator` は使わない
- 画面の文字と行の高さのトークンは `src/logic/appearance/interfaceMetrics.ts` が正本で、`src/ui/design/applyInterfaceMetrics.ts` が `:root` に当てる(M4.6。ADR-013 の 4)。ツリーの行の高さは CSS から読まず `InterfaceMetricsContext` から受け取る。追従させるトークンを足すときは `INTERFACE_TOKEN_DEFAULTS` と `tokens.css` の両方に書く(既定の値の一致はテストが確かめる)
- エディタの未保存の印は画面だけが送る(`set_document_dirty`。変わり目だけ)。Rust は保存でも確かめでも印を替えない
- 外の変更の確かめ `check_document` は Rust の状態を変えない。画面が本文を実際に当てたあとにだけ `adopt_document` で読んだ版の印(`token`)を送り、Rust が版を持ち直す。保存中のエディタは確かめを飛ばし、保存が終わってから 1 回確かめる(自分の保存の改名を外の変更と取り違えない)
- エディタの本文は中では `\n` で持ち、保存のときに文書の改行(`lineEnding`)へ戻す。CodeMirror の `lineSeparator` は使わない
- エディタの上のキーは `editorKeyOwner` で振り分ける: 表の受け口が `menu-item`・`edit-item` のキーは CodeMirror に渡さずメニューへ、`editor.save` は保存、ほかは CodeMirror。⌘W・前後のタブは Rust がメニューで受け、表示中のエディタが未保存なら `editor-close-requested` を送る
- エディタの本文とプレビューはターミナルの文字の大きさに追従し、枠は画面の文字の大きさ(ADR-015 の 7)
- ⌘,(`app.open-settings`)は画面に `menu-command` の `open-settings` を送って設定画面を開く。「設定ファイルを開く…」は表の外のメニューの項目(`menu::OPEN_SETTINGS_FILE_ID`。キーなし)で、表には載らない(表の行数は生成物 `defaultShortcuts.json` が正)
- 透過(M4.7。ADR-014)は `src/logic/appearance/translucency.ts` が値から不透明度を作り、`src/ui/design/applyTranslucency.ts` が `:root` にライト用・ダーク用の両方を当てる(どちらを使うかは `tokens.css` の media query)。ターミナルの地を塗るのは囲みの 1 段(`.terminal-area__panel`・`.tile__body`)だけで、`.terminal-host` と xterm.js の要素は透明にする(重ねて塗ると濃くなる)
- ツリーの矢印キー(`src/logic/tree/treeKeys.ts`)と Enter(開く。ダブルクリックと同じ)は表に載せない。Enter は表に割り当てが無いときだけ効く(`FileTreeView.tsx` の `onKeyDown`。表が先)。名称変更の既定は F2(キーの書式は `F1`〜`F12` まで。ADR-012 の末尾の追記)
- テーマ(システム・ライト・ダーク。ADR-016)は Rust がアプリの外観を替えて効かせる(`ThemeApplier`。`tokens.css` の `prefers-color-scheme` がそのまま追従する)。画面の CSS・JS に切替を書かない(`data-theme`・クラス・`matchMedia` で分けない)。模擬ランタイムは外観の切替を実装していない(呼ぶと panic する)ので、`AppHandle::set_theme` を直接呼ばず `ThemeApplier` を通す
- タイルの番号キー(`view.focus-tile-1`〜`-9`)は Rust がメニューで受けて `focus_tile_at` を呼ぶ。番号は表示中のページの並び順で、画面の札「#n」は写しの `tileIds` の位置から出す(キーの表示ではないので表から引かない)
- 並べ替え(ADR-019)の並びは Rust が持つ(`move_project`・`move_terminal_tab`・`move_editor_tab`。`toIndex` は動かしたあとの位置)。タブは同じプロジェクトの同じ種類の中だけで動き、種類をまたぐ口は作らない。画面のドラッグは `src/ui/shell/useReorderDrag.ts`(pointer のイベント。項目に `data-reorder-group`・`data-reorder-id`、始めない場所に `data-reorder-ignore`)、落とす位置の規則は `src/logic/dnd/reorder.ts`
- タブをまとめて閉じるのは `close_tabs` の 1 回(画面から `close_terminal`・`close_editor` を並べて呼ばない)。確認は真偽でなく、確認に出した ID の一覧(`confirmedTerminalIds`・`confirmedEditorIds`)を付けて呼び直す(確認のあいだに増えたら Rust がもう一度確認を求める)。画面が未保存と知っているエディタは `dirtyEditorIds` で送る(`set_document_dirty` は遅れて届く)。メニューの項目と閉じるタブは `src/logic/menu/tabMenu.ts`
- 並べ替えのコマンドは対象が無いとき失敗にしない(`ignore_closed` と同じ)。ドラッグのあいだに写しで組の並びが変わったら、画面は離しても送らない
- ターミナルの Shift+Enter は `ESC`+`CR` を送る(`src/logic/keys/newline.ts`。表より先に判定する。表には載せない)
- 設定の知らせは写しの `notice` と別に持つ(Rust は `AppSettings`、画面は `uiStore` の `settingsNotice`)。中身は設定ファイルの問題と「設定ファイルを開く…」の結果(開けなかった・見本を作れなかった)の 2 つを 1 行にまとめたもの。開く結果の知らせは、次に開けたときと、読み直しで表か設定ファイルの問題の知らせが変わったときに消える。画面で閉じても、違う文の知らせが届けば出し直す

## 3. 型の生成

- Rust の型(`AppSnapshot` など)を変えたら `npm run gen:types` で `src/ipc/generated/` を再生成する。生成物は手で編集しない。古いままだと `cargo test` が失敗する

## 4. 依存の版

- 版は正確に固定する(npm は `^` なし、Cargo は `=`)。新しく選ぶ版は公開から 2 週間未満のものを避ける(ADR-003)
- OS 連携のクレート(objc2 系・libc・base64)は Tauri が引き込む版と同じにする。Tauri を上げたら `Cargo.lock` に 2 つの版が載っていないか確かめ、ずれたら合わせる(ADR-009)
- 依存(本番の npm・Rust の通常の依存)を変えたら `npm run licenses` で `THIRD_PARTY_LICENSES.md` を作り直してコミットする(`release.yml` が `licenses:check` で止める)。MPL-2.0 のクレートの版が変わったら `THIRD_PARTY_NOTICES.md` の表も直す
- `notify` は 8.2.1 に固定。推移的な `notify-types` は `Cargo.lock` で 2.1.0 に止めている(`cargo update` で上がりうる)
- Biome は React のフックの 2 規則だけを見る。わざと依存を外すときは `biome-ignore` に理由を書く。後片付けで `ref.current` を読む誤りは検出されないのでレビューで見る(ADR-008)

## 5. テスト

- 状態フォルダは `ARIN_STATE_DIR`(絶対パス)で差し替える。テストとスモークは本物の `~/Library/Application Support/Arin/` を読み書きしない。`ARIN_SMOKE=1` は差し替え時だけ有効
- Tauri のアプリ(模擬ランタイム)を作る `src-tauri/tests/` は `harness = false`(macOS ではメニューの部品をメインスレッドでしか作れず、標準ハーネスの別スレッドでは panic する)。テストを足すときは `src-tauri/Cargo.toml` に `[[test]]` を足す
- `src-tauri/tests/` の 1 つのテストのバイナリに `tauri::generate_context!()` は 1 か所だけ書ける。組み立てを 2 通り使うときは共通の仕上げの関数(`app_settings.rs` の `finish()`)を通す
- 設定ファイルの監視は `builder_with_settings_watcher` で偽の `WatcherPort` に差し替える(`src-tauri/tests/settings_watch.rs`)。テストで設定ファイルを書き換えるときは一時ファイルに書いて改名する(その場で上書きすると、読み直しが書きかけの空や途中までの中身を読むことがある)
- 結合テストで起動直後にターミナルが要るときは `src-tauri/tests/support/` の `launch_with_projects_in`(空の状態フォルダの起動は 0 枚)、`Workspace` を直接作るときは `workspace_with_projects`
- 実 PTY のテスト(`crates/arin-platform/tests/pty.rs` にならう): シェルは `/bin/zsh` 固定、`HOME` を一時フォルダに向ける、出力は目印の文字列で囲ませて取り出す、共通のロックで直列にする、どの待ちにも上限時間を付ける
- ペーストボードのテストは一般のペーストボード(利用者のクリップボード)に触れず、名前付きのペーストボードに書いて終わったら解放する(ADR-009)
- 性能の計測テストは `#[ignore]` にし、`npm run perf` が `--release -- --ignored` で動かす
- `npm run e2e` は実アプリではなく Vite の画面を WebKit で開き、Rust との通信は `e2e/fixtures/tauriMock.ts` が差し替える(出力は `sendChannelBytes`、イベントは `emitEvent` で流す)。製品のソースに差し込み口は置かない。1420 番を使うので `npm run dev` を動かしたままだと失敗する
- `tauriMock.ts` の `withTileDisplay` はタイル表示の Rust の規則(グリッド・ページ・フォーカス)の写し。Rust の規則を変えたら合わせる(合っていないと製品の修正で E2E が落ちる)
- 計測のテスト(`#[ignore]`・`e2e/perf-tiles.spec.ts`)は通常のテストでは走らない。足場(`support/`・`tauriMock.ts`・状態ファイルの置き方)を変えたら `npm run perf` を回して確かめる
- 本物のゴミ箱を使うテストは `#[ignore]`(`cargo test -p arin-platform --test trash -- --ignored`。入れた項目は自分で片付ける)。Finder・既定のアプリには偽の `DesktopPort` を使う
- ファイル操作のテストは一時フォルダを `canonicalize` した実パスで使う(macOS の一時領域は `/var` → `/private/var` のリンクを含む)
- E2E でスクロールや描き直しの直後の状態を測るときは、落ち着いた状態を `expect.poll` で待つ
- E2E で設定の変更を流すときは、`setMockReply` で `get_shortcuts`・`get_settings_notice` の応答を替えてから `emitEvent(page, "settings-changed", null)` を流す(`e2e/settings.spec.ts`)。`e2e/app-shell.spec.ts` は起動時の呼び出しの並びを完全一致で見るので、起動時の呼び出しを足したら合わせる
- `TerminalManager` のテストでは `scheduleFrame` に手で流す偽物を渡す(文字の大きさの変更は 1 フレームにまとめて当てる)
- `src/logic` のテストで `node:fs` を使うときは型の無い動的 import で読み込む(`interfaceMetrics.test.ts`)
- E2E で文字の大きさを流すときは `tauriMock.ts` の `withFontSizes`、透過は `withTranslucency` で作った写しを流す(偽の Rust 側は写しを送り返さない)。`@tauri-apps/api` の `invoke` は空の引数を `{}` で送るので、引数なしのコマンドの記録は `{}` になる
- エディタの部品のテストは `src/test/fakeEditorEngine.ts`(CodeMirror も DOM も使わない)を渡す。本物の `CodeMirrorEngine.ts` は Vitest では動かない(DOM が無い)ので E2E で確かめる
- E2E でエディタを出すときは `tauriMock.ts` の `withEditors`・`sampleEditorTab` で写しを作る。文書のコマンドの応答は `setMockReply` で替える
- 文書の読み書きのテスト(`crates/arin-platform/tests/document.rs`)は一時フォルダの実パスで、拡張属性は `libc::setxattr`・`getxattr` で確かめる。`src-tauri/tests/editor_commands.rs` は `harness = false`
- スモークは「設定ファイルを開く…」を選ばない(本物の既定のアプリや Finder が開く)。テストは偽の `DesktopPort` を使う

## 6. 過去の問題事例

| 問題 | 原因 | 対策 |
| ---- | ---- | ---- |
| `node --test scripts/` が失敗 | Node 24 はディレクトリをモジュールとして読もうとする | `node --test "scripts/*.test.mjs"`(`npm test` に含む) |
| `rm -rf` が止められる | `.claude/settings.json` の deny に `Bash(rm -rf *)` がある | `npm ci` は自分で `node_modules` を消すので不要 |
| `Cargo.lock` に `reqwest` が載る | Lock は全 OS の依存を載せる。Tauri は Android・iOS でだけ使う | 境界検査は `tauri → reqwest` の 1 辺だけを理由付きで除外。Tauri を上げたら `cargo tree -i reqwest --target aarch64-apple-darwin` で再確認 |
| 型検査用の一時ファイルから `.d.ts` が `src/` に出てコミットされた | 型検査失敗時にも出力していた | `tsconfig` に `noEmitOnError: true`(src・src/logic・e2e) |
| 素の `cargo build` のバイナリは最低 OS が 11.0 になる | 最低 OS 15.0(`tauri.conf.json` の `minimumSystemVersion`)は `tauri build` が渡す | 配布物は必ず `npm run app` で作る(`otool -l` の `minos` で確認できる) |
| `portable-pty` でログインシェルの `argv[0]` を `-zsh` にできない・環境変数が漏れる・不正な値が黙って置き換わる | `CommandBuilder::new` は `argv[0]` を変えられない。作った時点で親の環境変数を取り込む。`SHELL` が実行できない・cwd がフォルダでないと別の値に切り替える | `new_default_prog()` + `SHELL` で起動し、`env_clear()` してから許可リストの値だけ渡す。シェルと cwd は起動前に確かめてエラーにする(`arin-platform/src/pty/native.rs`) |
| xterm.js が CSP で止められる | 実行時に `<style>` 要素を作り、truecolor の文字に style 属性を書く | `style-src-elem 'self' 'unsafe-inline'` と `style-src-attr 'unsafe-inline'` を足す(`style-src`・`script-src` は変えない) |
| Tauri の既定のメニューが ⌘W などを持つ | メニューを指定しないと macOS で既定のメニューが付く | `menu.rs` で自前のメニュー(「Arin」「ターミナル」「編集」「表示」)に置き換えた。⌘C・⌘V はメニュー経由で画面の `copy`・`paste` イベントになる |
| `footprint` が別のプロセスを測る | PID を位置引数で渡すと名前の部分一致になる | 必ず `-p <pid>` で渡す(`scripts/perf/app-memory.mjs`) |
| フォルダを削除しても「見つからない」にならない | `F_GETPATH` は削除後も消える前のパスを返す(macOS 26、APFS) | 記述子の `fstat` が `ENOENT` かと、返ったパスの装置番号・inode が一致するかで判定する(`arin-platform/src/watch/`) |
| `tauri-plugin-dialog` を足すとファイル操作のプラグインが入る | `tauri-plugin-fs` が optional でない依存 | `NSOpenPanel` を直接呼ぶ(`desktop/folder_panel.rs`)。メインスレッドでしか開けないので `run_on_main_thread` で開いて待つ |
| Finder に渡した日本語の名前が別物になる | `NSURL::fileURLWithPath` は名前を NFD に変える | パスを自前で百分率符号化して URL を作る(`desktop/workspace.rs` の `file_url_from_str`) |
| 検査のあとに途中のフォルダをリンクへ差し替えられるとルートの外に書く | パスの検査と操作の間に隙がある | 親を `O_NOFOLLOW_ANY` で開き fd 相対で操作する(`fs/local.rs`) |
| 大文字・小文字だけの名称変更が衝突にならない | APFS では `RENAME_EXCL` でも成功する(設計書の見込みと違った) | 前提にしない |
| ツリーに `.arin-copy-…` が見える | コピーは宛先のフォルダに一時的な作業用フォルダを作る | 監視はこの名前を隠す。一覧は隠しファイル表示が OFF のとき隠し、ON なら残ったものを出す(`[arin:fileops]` の行と合わせて本人が片付ける) |
| `scripts/smoke.test.mjs` がときどき落ちる | 一時フォルダの数を数える検査が、ほかのテストと同時に動かすと乱れる | 単独で再実行して確かめる |
| 画面の表示が前の状態に戻る(タイルのフォーカス・ページ) | 写しを作る→送るが直列でなく、古い写しがあとから届いた(送り手はコマンド・読み取りスレッド・見回りなど複数) | `SessionRegistry::notify` の中で写しを作って送るまでを直列にした(`sessions.rs`)。写しを送る経路を足すときも `notify` を通す |
| メニューのキーを `MenuItem::set_accelerator` で替えられない | 読めない文字列は黙って `None` になり、muda の macOS の実装は `None` でキーを消さない(Tauri 2.12.5・muda 0.20.0。ADR-012 の 5) | 表を差し替えたら `menu::rebuild` でメニューを丸ごと作り直す |
| 引用符の無い点つきの ID(`terminal.new-tab = …`)が 1 つの ID にならない | TOML では入れ子の表(`terminal` の下の `new-tab`)になる | `[shortcuts]` の下の入れ子の表は 1 段だけ `.` でつないで読む(`parse_settings_toml`)。見本は引用符で囲む形だけを示す(ADR-012 の 2) |
| シンボリックリンクの `settings.toml` の先を保存しても反映されない | 監視は状態フォルダに張るので、リンクの先の変更は届かない | 保存のあとにリンク自体を触る(`touch`)か再起動する(見本にも書いた。ADR-012 の 1・5) |
| ターミナルの文字の大きさを変えても行・桁が変わらない | 文字の大きさだけを変えても置き場所の大きさは変わらず、`ResizeObserver` が鳴らない。`display: none` の中では xterm.js が文字の寸法を 0 で測りうる | 表示中は当ててから明示して寸法を合わせ直す。隠しているものは覚えておき、表示に戻すときに当てる(`XtermEngine.ts`・`TerminalManager.ts`) |
| 文字の大きさのトークンを `calc()` にするとツリーの行の高さが読めない | CSS の変数の `getPropertyValue` は計算前の文字列を返す | 行の高さは CSS から読まず `interfaceMetrics` の値を文脈で受け取る |
| 設定画面でボタンをマウスで押したあとフォーカスが行き場を失う | WebKit はボタンをマウスで押してもボタンにフォーカスせず、`<dialog>` 自身に置く | `<dialog>` 自身にあるときも行き場が無いとみなして「完了」へ移す(`SettingsDialog.tsx`) |
| 設定画面を開いたまま ⌘W でタブが閉じうる | ⌘W はメニューの項目のキーで、確認なしでタブを閉じる | 設定画面の keydown で取り消して設定画面だけを閉じる。実機でメニューのキーより先に効くかは未確認 |
| xterm.js のテーマの地を透明にしても黒いまま | `xterm.css` が `.xterm-viewport` を `#000` で塗る | `src/terminal/xterm/XtermEngine.css` で透明に上書きする(`allowTransparency: true` と合わせて) |
| `\r\n` と `\n` が混ざった文書の保存の基準が食い違う | CodeMirror は読み込みで改行をそろえる | 中では `\n` で持ち、保存で文書の改行(`\r\n` が 1 つでもあれば `crlf`)に戻す。混ざった文書は保存で 1 種類にそろい、単独の `\r` は `\n` になる(知らせは出さない) |
| 外の変更の読み直しで未保存の印と版が失われる | 確かめの応答を待つあいだに打鍵・保存が挟まる | `check_document` は状態を変えず、本文を当てたあとにだけ `adopt_document` で版を持ち直す |
| 書きかけで外の変更を見つけると `.arin-save-…` が残った | 削除の経路を作らない規則で、止めた保存の一時ファイルも残していた | 保存が自分で作った一時ファイルに限って片付ける(本人が了承)。名前が別のものに替わっていた・消せなかったときだけ残し、`[arin:editor]` に場所を出す。アプリが落ちたときの残りは片付けない(ツリーは隠しファイル表示が ON なら出す) |
| stat が同じだと中身の変更を見落としうる | 版は stat の 4 つが同じなら中身を比べない | 大きさと mtime を保って書き換える道具の変更は拾えない(承知のうえ) |
| エディタの塊が大きい | CodeMirror・lezer・Vim で `CodeMirrorEngine-*.js` が 737 kB(gzip 250 kB) | 動的 import で別の塊にし、最初に開くときだけ読む。本体は 693 → 719 kB |
| 並べ替えのドラッグで、落とす位置の線が掴んだ項目と重なって取り消し線に見える | 掴んだ項目はポインタに付いて動くので、線のある境目の上に来る | 線を引かず、あいだの項目を滑らせて隙間を空ける。落とす位置はドラッグを始めたときの中心で決める(滑った位置で決めると行ったり来たりする。ADR-019) |
| タブを右クリックするとタブ名の文字が選ばれる | WebKit は右クリックで語を選ぶ | `.tab-bar` に `user-select: none` |
| E2E で隙間を空ける動きの途中が撮れる | 160ms の `transition` | 画像を撮るテストは `emulateMedia({ reducedMotion: "reduce" })`(動きを減らす設定では滑らせない) |
| IME の変換中の ⌘S で保存が走る | 保存のキーは変換中かを見ない | 未対応(合否一覧の残り) |
| 既定のショートカットが JIS 配列で打てない(⇧⌘\ など) | US 配列の記号のキーは JIS で別の位置・別の組み合わせになる | JIS 配列で打てない記号のキーを既定にしない(M4 は ⇧⌘G に替えた) |
