# アーキテクチャ

レイヤー定義と依存方向の全体は `output/design/ARCH_multi-terminal.md` §3、最終形のディレクトリは §4、テスト方針は §10 と ADR-006。ここには M4.8 時点で実在するものだけを書く。

## エントリーポイント

- Rust: `src-tauri/src/main.rs` → `lib.rs`。`launch.rs` が環境変数を読み、`lib.rs` が設定ファイルを読んで(`settings.rs`。ショートカットの表と設定の知らせ)から状態ファイルを読み(`persistence.rs`)、`Workspace`・`SessionRegistry`・保存のスレッド・`Projects`・見回り(`patrol.rs`)・設定ファイルの監視(`settings_watch.rs`)・メニューを組み立て、`commands/`(`app.rs`・`terminal.rs`・`project.rs`・`layout.rs`・`tree.rs`・`fileops.rs`・`desktop.rs`・`editor.rs`)が画面からのコマンドを受ける
- 画面: `src/main.tsx` が `createAppStore()`・`createUiStore()` でストアを作り、`startApp` で写しの取得と準備完了の通知を行い、`AppShell` に props で渡す

## ディレクトリ構成

```
crates/arin-core/      # Core: 型・規則。入出力なし
│                      #   paths/(相対パス・表示と挿入の文字列)、tree/(除外・変更の集約)、fileops/(名前・範囲・操作)、shortcuts/、project/(ID・色・パス・ProjectStore)、layout/(PaneSizes・tiles=グリッドとページ・font_sizes=文字の大きさの範囲と既定と刻み)、persistence/(state.json の codec・保存の予定)、settings/(settings.toml を読んだ結果の型・SettingsStore)、editor/(EditorId・EditorTab・EditorStore・版 FileStamp と要約・応答の型。M4.8)、
│                      #   workspace/(Workspace・写し・イベントの中身・display=表示モードとページと拡大・font_sizes=文字の大きさの口・editors=エディタのタブ)、terminal/(起動の仕様・状態・流量制御・OSC 走査・attention=アテンションの走査器)、
│                      #   ports/(PtyPort・状態ファイル・設定ファイル・ルート・乱数と時計・ペーストボード・fs・watch・desktop・document=文書の読み書き・release=最新版の問い合わせ)、update/(版の比較と知らせ。ADR-018)
crates/arin-platform/  # Platform: OS への入出力(storage/ = ready_marker・state_file・settings_file(TOML の解析)、pty/、bookmark/、fs/(fd 相対のファイル操作・ゴミ箱・document=文書の読み込みと一時ファイルからの改名による保存)、watch/(F_GETPATH・fsevents)、
│                      #   command.rs(コマンドを上限時間付きで起動)、release/(`/usr/bin/curl` で最新版を問い合わせる。通信はここだけ。ADR-018)、system/(乱数・時計)、desktop/(ペーストボード・file_pasteboard・フォルダ選択のダイアログ・workspace(Finder・既定のアプリ)))
src-tauri/src/         # App: Tauri の組み立て(クレート名 arin、lib は arin_lib)
│                      #   sessions.rs(PTY ごとのスレッドと流量制御)、projects.rs、persistence.rs([arin:state])、patrol.rs(ルートの見回り)、tree.rs・tree_watch.rs([arin:watch])・fileops.rs、settings.rs([arin:settings]。設定の読み込みと「設定ファイルを開く…」)・settings_watch.rs(保存の検知)、update.rs(新しい版の確認。ADR-018)、commands/editor.rs([arin:editor]。文書のコマンド)、
│                      #   events.rs(イベント名)、commands/、menu.rs、lifecycle.rs(終了の確認・ドロップ)、
│                      #   diagnostics.rs([arin:frontend-error])、verify_log.rs([arin:verify]。開発ビルドだけ)
src/
├── ipc/               # Rust との通信の包み。@tauri-apps/* はここだけ
│   └── generated/     # ts-rs の生成物(手で編集しない)
├── state/             # appStore・uiStore・treeStore、actions(コマンドの呼び出し)、selectors、terminalSync、起動手順・エラー報告
├── terminal/          # エンジンの抽象(engine.ts)、TerminalManager(表示中の並び以外は DOM の外で取り込み続ける)、入力の列
│   └── xterm/         # xterm.js の実装(XtermEngine。DOM 描画と unicode11 に固定)。@xterm/* はここだけ
├── editor/            # エディタの口(engine.ts)、EditorManager(部品を React の外で持ち、置き場所へ付け外し。プレビューの解析の予定)、markdownTypes(プレビューの木の型)、diagram(図の描画の口。ADR-017)
│   ├── codemirror/    # CodeMirrorEngine(初めて開くときに動的 import)、markdownAst(lezer の木 → プレビューの木)、slashCompletion(「/」コマンド)。codemirror・@codemirror/*・@lezer/*・@replit/* はここだけ
│   └── mermaid/       # MermaidRenderer(初めて図を描くときに動的 import。SVG の文字列を返す)。mermaid・@mermaid-js/* はここだけ
├── ui/shell/          # AppShell、PaneSplit・Divider(ペインの分割)、NoticeBar、TreePane
├── ui/projects/       # カード一覧
├── ui/terminals/      # TerminalArea・TabBar・TerminalHost、TileGrid・TileTitleBar・PageControl(タイル表示)、コピー/ペーストの振り分け
├── ui/dialogs/        # 確認ダイアログ(削除・終了・ConfirmDialog・ConflictDialog・MoveConfirmDialog・UnsavedCloseDialog・SaveConflictDialog)
├── ui/menu/           # 右クリックのメニュー(ContextMenu)
├── ui/tree/           # TreeHeader・FileTreeView・TreeRow・InlineNameInput・TreeDragGhost・icons
├── ui/design/         # tokens.css、palette.ts(プロジェクト色の値)、applyInterfaceMetrics(画面の文字の大きさを :root に当てる・InterfaceMetricsContext)
├── ui/settings/       # 設定画面(SettingsDialog・ValueStepper)
├── ui/editor/         # EditorPanel(上端の帯)・EditorHost(置き場所)・ViewSwitch(編集・並べて・プレビュー)・MarkdownPreview(木を React の要素に)・MermaidBlock(図を blob: の <img> で出す)
├── logic/             # 純粋ロジック(DOM・他の src/* に依存しない。別 tsconfig。keys/ = キーとメニュー・クリップボードの振り分けとショートカットの表、menu/(treeMenu を含む)、panes/、tree/ = ツリーのモデル・仮想スクロール、dnd/ = ドラッグ、appearance/ = 画面の文字の大きさから追従させるトークンの値、editor/ = 文書のパスの判定・プレビューの解析の予定・「/」コマンドの表(slashCommands)。keys/editorKeyOwner = エディタの上のキーの受け手)
└── test/              # テスト補助(fakeIpc・fakeEditorEngine)
e2e/                   # Playwright(fixtures/tauriMock.ts)
scripts/               # 境界検査・カバレッジ・スモーク・perf/(fixtures/ は違反の見本)
```

依存方向: `src/ui` → `src/state`・`src/terminal`・`src/editor`(`src/ipc`・`src/terminal/xterm`・`src/editor/codemirror`・`src/editor/mermaid` は直接 import しない)。`src/terminal/xterm`・`src/editor/codemirror`・`src/editor/mermaid` は `src/ipc` を import しない。`arin-platform` → `arin-core`、`src-tauri` → 両方。

## ターミナルの入出力の流れ

PTY → 読み取りスレッド(未処理の量が上限を超えたら読まない)→ まとめのスレッド(短い時間ぶんまとめる)→ Tauri のチャネル(生のバイト列)→ xterm.js → `ack_terminal_output`(処理済みの通知で読み取りを再開)。ターミナル 1 枚につきスレッド 2 本。状態が変わると `app-snapshot` イベントで写し全体を送る。規則(上限・下限・まとめの量)は `arin-core::terminal`、スレッドとロックは `src-tauri/src/sessions.rs`。

## レイヤーと境界の強制

| 手段 | 強制する内容 |
| ---- | ------------ |
| Cargo の依存宣言 | arin-platform の依存は arin-core・portable-pty・notify・objc2 系・libc・base64・thiserror・toml。arin-core は serde・serde_json・thiserror・ts-rs だけ |
| TypeScript のプロジェクト参照(`tsconfig.json` → `src/logic`・`src`・`e2e`) | `src/logic` から他を import すると `tsc -b` が失敗 |
| `scripts/check-boundaries.mjs`(`npm run lint`) | arin-core の依存と `std::fs`/`process`/`net` 禁止、`@tauri-apps/*` は `src/ipc` だけ、`@xterm/*` は `src/terminal/xterm` だけ、`codemirror`・`@codemirror/*`・`@lezer/*`・`@replit/*` は `src/editor/codemirror` だけ、`mermaid`・`@mermaid-js/*` は `src/editor/mermaid` だけ、`src/ui` → `src/ipc`・`src/editor/codemirror`・`src/editor/mermaid` 禁止、`src/editor/codemirror`・`src/editor/mermaid` → `src/ipc` 禁止、通信クレート・通信 API・HTML 差し込みの禁止、Rust の通信(curl・NSURLSession・std::net)は `crates/arin-platform/src/release/` だけ(ADR-018)、CSP の `connect-src` |

## テスト構成

| 種類 | 置き場所 | 実行 |
| ---- | -------- | ---- |
| TS 単体 | `src/**/*.test.ts`(コロケーション) | vitest(environment: node) |
| Rust 単体 | 各ソースの `#[cfg(test)]` | cargo test |
| Rust 結合 | `crates/*/tests/`、`src-tauri/tests/`(模擬ランタイムを使うものは `harness = false`、補助は `support/`)。タイル表示とアテンション: `crates/arin-core/tests/`(layout_tiles・workspace_display・workspace_attention・attention_scanner)、`src-tauri/tests/`(display_commands・session_attention) | cargo test |
| 実 PTY の結合 | `crates/arin-platform/tests/pty.rs`、`src-tauri/tests/session_pty.rs` | cargo test |
| 性能の計測 | `crates/arin-platform/tests/perf_pty.rs`、`src-tauri/tests/perf_sessions.rs`、`crates/arin-platform/tests/perf_tree.rs`(すべて `#[ignore]`)、`scripts/perf/` | `npm run perf` |
| ツリーとファイル操作の結合 | `crates/arin-core/tests/`(rel_path・tree_rules・fileops_name・fileops_scope・fileops_service・shortcuts・path_format・shell_escape)、`crates/arin-platform/tests/`(fs・fs_race・watch・desktop・shell_roundtrip、trash は `#[ignore]`)、`src-tauri/tests/`(tree_commands・tree_watch・fileops_commands・desktop_commands・app_menu) | cargo test |
| 設定ファイル(M4.5) | `crates/arin-core/tests/`(settings_load・shortcut_overrides・app_paths)、`crates/arin-platform/tests/settings_file.rs`、`src-tauri/tests/`(app_settings・settings_watch。偽の監視と偽の `DesktopPort`) | cargo test |
| 文字の大きさ(M4.6) | `crates/arin-core/tests/`(font_sizes・workspace_font_sizes)、`src-tauri/tests/font_commands.rs` | cargo test |
| Markdown エディタ(M4.8) | `crates/arin-core/tests/`(editor_document・workspace_editors)、`crates/arin-platform/tests/document.rs`、`src-tauri/tests/editor_commands.rs`(`harness = false`)、画面は `src/editor/*.test.ts`・`src/editor/codemirror/markdownAst.test.ts`・`src/ui/editor/MarkdownPreview.test.ts`・`src/state/editorController.test.ts`・`src/logic/editor/slashCommands.test.ts`(本物の `CodeMirrorEngine.ts` と mermaid は E2E だけ。`e2e/editor-mermaid-slash.spec.ts`) | cargo test / vitest |
| 新しい版の確認(ADR-018) | `crates/arin-core/src/update/`(単体)、`crates/arin-platform/src/release/`(単体。本物の GitHub への問い合わせは `#[ignore]`)、`src-tauri/tests/desktop_commands.rs`(偽の `ReleasePort`)、`src/state/startApp.test.ts`・`src/ui/shell/NoticeBar.test.ts`、`e2e/update-check.spec.ts` | cargo test / vitest / playwright |
| 並べ替えとタブをまとめて閉じる(ADR-019) | `crates/arin-core/tests/workspace_reorder_tabs.rs`、`src-tauri/tests/project_commands.rs`(`move_project`・`move_terminal_tab`・`close_tabs`)、画面は `src/logic/dnd/reorder.test.ts`・`src/logic/menu/tabMenu.test.ts`・`src/state/actions.test.ts` | cargo test / vitest |
| 型の生成物の一致 | `crates/arin-core/tests/generated_types.rs` | cargo test |
| スクリプト | `scripts/*.test.mjs`(default-shortcuts は見本の表と生成物の一致) | node --test |
| 画面 E2E | `e2e/*.spec.ts`(app-shell、terminal、projects、tabs、m2-scenario、tree、tree-ops、tree-dnd、m3-scenario、tiles、m4-scenario、settings、m45-scenario、font-settings、m46-scenario、m47-scenario、editor、m48-scenario、update-check、reorder-tab-menu) | playwright(WebKit) |
| 画面側の性能の計測 | `e2e/perf-tiles.spec.ts`(`npm run e2e` には入らない。`ARIN_PERF_E2E=1` のときだけ) | `npm run perf` |
| 実アプリのスモーク | `scripts/smoke.mjs`(`--tiles` で 9 プロジェクトのタイル表示、`--settings` で設定ファイルを置いて起動、`--fonts` で文字の大きさを入れた `state.json` で 1 回起動) | `npm run smoke` |

## ドキュメント責務

| ファイル                      | 責務                                                   |
| ----------------------------- | ------------------------------------------------------ |
| `docs/project.md`             | ストア一覧、コマンド、技術スタック                     |
| `docs/architecture.md`        | ディレクトリ構成、境界、テスト構成                     |
| `docs/data-model.md`          | スキーマ定義、フィールド仕様、バリデーションルール     |
| `docs/development-patterns.md`| コード規約、落とし穴                                   |
