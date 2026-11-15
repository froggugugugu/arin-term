# アーキテクチャ

レイヤー定義と依存方向の全体は `output/design/ARCH_multi-terminal.md` §3、最終形のディレクトリは §4、テスト方針は §10 と ADR-006。ここには M4 時点で実在するものだけを書く。

## エントリーポイント

- Rust: `src-tauri/src/main.rs` → `lib.rs`。`launch.rs` が環境変数を読み、`lib.rs` が状態ファイルを読んで(`persistence.rs`)`Workspace`・`SessionRegistry`・保存のスレッド・`Projects`・見回り(`patrol.rs`)・メニューを組み立て、`commands/`(`app.rs`・`terminal.rs`・`project.rs`・`layout.rs`・`tree.rs`・`fileops.rs`・`desktop.rs`)が画面からのコマンドを受ける
- 画面: `src/main.tsx` が `createAppStore()`・`createUiStore()` でストアを作り、`startApp` で写しの取得と準備完了の通知を行い、`AppShell` に props で渡す

## ディレクトリ構成

```
crates/arin-core/      # Core: 型・規則。入出力なし
│                      #   paths/(相対パス・表示と挿入の文字列)、tree/(除外・変更の集約)、fileops/(名前・範囲・操作)、shortcuts/、project/(ID・色・パス・ProjectStore)、layout/(PaneSizes・tiles=グリッドとページ)、persistence/(state.json の codec・保存の予定)、
│                      #   workspace/(Workspace・写し・イベントの中身・display=表示モードとページと拡大)、terminal/(起動の仕様・状態・流量制御・OSC 走査・attention=アテンションの走査器)、
│                      #   ports/(PtyPort・状態ファイル・ルート・乱数と時計・ペーストボード・fs・watch・desktop)
crates/arin-platform/  # Platform: OS への入出力(storage/ = ready_marker・state_file、pty/、bookmark/、fs/(fd 相対のファイル操作・ゴミ箱)、watch/(F_GETPATH・fsevents)、
│                      #   system/(乱数・時計)、desktop/(ペーストボード・file_pasteboard・フォルダ選択のダイアログ・workspace(Finder・既定のアプリ)))
src-tauri/src/         # App: Tauri の組み立て(クレート名 arin、lib は arin_lib)
│                      #   sessions.rs(PTY ごとのスレッドと流量制御)、projects.rs、persistence.rs([arin:state])、patrol.rs(ルートの見回り)、tree.rs・tree_watch.rs([arin:watch])・fileops.rs、
│                      #   events.rs(イベント名)、commands/、menu.rs、lifecycle.rs(終了の確認・ドロップ)、
│                      #   diagnostics.rs([arin:frontend-error])、verify_log.rs([arin:verify]。開発ビルドだけ)
src/
├── ipc/               # Rust との通信の包み。@tauri-apps/* はここだけ
│   └── generated/     # ts-rs の生成物(手で編集しない)
├── state/             # appStore・uiStore・treeStore、actions(コマンドの呼び出し)、selectors、terminalSync、起動手順・エラー報告
├── terminal/          # エンジンの抽象(engine.ts)、TerminalManager(表示中の並び以外は DOM の外で取り込み続ける)、入力の列
│   └── xterm/         # xterm.js の実装(XtermEngine。DOM 描画と unicode11 に固定)。@xterm/* はここだけ
├── ui/shell/          # AppShell、PaneSplit・Divider(ペインの分割)、NoticeBar、TreePane
├── ui/projects/       # カード一覧
├── ui/terminals/      # TerminalArea・TabBar・TerminalHost、TileGrid・TileTitleBar・PageControl(タイル表示)、コピー/ペーストの振り分け
├── ui/dialogs/        # 確認ダイアログ(削除・終了・ConfirmDialog・ConflictDialog・MoveConfirmDialog)
├── ui/menu/           # 右クリックのメニュー(ContextMenu)
├── ui/tree/           # TreeHeader・FileTreeView・TreeRow・InlineNameInput・TreeDragGhost・icons
├── ui/design/         # tokens.css、palette.ts(プロジェクト色の値)
├── logic/             # 純粋ロジック(DOM・他の src/* に依存しない。別 tsconfig。keys/ = キーとメニュー・クリップボードの振り分けとショートカットの表、menu/(treeMenu を含む)、panes/、tree/ = ツリーのモデル・仮想スクロール、dnd/ = ドラッグ)
└── test/              # テスト補助(fakeIpc)
e2e/                   # Playwright(fixtures/tauriMock.ts)
scripts/               # 境界検査・カバレッジ・スモーク・perf/(fixtures/ は違反の見本)
```

依存方向: `src/ui` → `src/state`・`src/terminal`(`src/ipc`・`src/terminal/xterm` は直接 import しない)。`src/terminal/xterm` は `src/ipc` を import しない。`arin-platform` → `arin-core`、`src-tauri` → 両方。

## ターミナルの入出力の流れ

PTY → 読み取りスレッド(未処理の量が上限を超えたら読まない)→ まとめのスレッド(短い時間ぶんまとめる)→ Tauri のチャネル(生のバイト列)→ xterm.js → `ack_terminal_output`(処理済みの通知で読み取りを再開)。ターミナル 1 枚につきスレッド 2 本。状態が変わると `app-snapshot` イベントで写し全体を送る。規則(上限・下限・まとめの量)は `arin-core::terminal`、スレッドとロックは `src-tauri/src/sessions.rs`。

## レイヤーと境界の強制

| 手段 | 強制する内容 |
| ---- | ------------ |
| Cargo の依存宣言 | arin-platform の依存は arin-core・portable-pty・notify・objc2 系・libc・base64・thiserror。arin-core は serde・serde_json・thiserror・ts-rs だけ |
| TypeScript のプロジェクト参照(`tsconfig.json` → `src/logic`・`src`・`e2e`) | `src/logic` から他を import すると `tsc -b` が失敗 |
| `scripts/check-boundaries.mjs`(`npm run lint`) | arin-core の依存と `std::fs`/`process`/`net` 禁止、`@tauri-apps/*` は `src/ipc` だけ、`@xterm/*` は `src/terminal/xterm` だけ、`src/ui` → `src/ipc` 禁止、通信クレート・通信 API・HTML 差し込みの禁止、CSP の `connect-src` |

## テスト構成

| 種類 | 置き場所 | 実行 |
| ---- | -------- | ---- |
| TS 単体 | `src/**/*.test.ts`(コロケーション) | vitest(environment: node) |
| Rust 単体 | 各ソースの `#[cfg(test)]` | cargo test |
| Rust 結合 | `crates/*/tests/`、`src-tauri/tests/`(模擬ランタイムを使うものは `harness = false`、補助は `support/`)。タイル表示とアテンション: `crates/arin-core/tests/`(layout_tiles・workspace_display・workspace_attention・attention_scanner)、`src-tauri/tests/`(display_commands・session_attention) | cargo test |
| 実 PTY の結合 | `crates/arin-platform/tests/pty.rs`、`src-tauri/tests/session_pty.rs` | cargo test |
| 性能の計測 | `crates/arin-platform/tests/perf_pty.rs`、`src-tauri/tests/perf_sessions.rs`、`crates/arin-platform/tests/perf_tree.rs`(すべて `#[ignore]`)、`scripts/perf/` | `npm run perf` |
| ツリーとファイル操作の結合 | `crates/arin-core/tests/`(rel_path・tree_rules・fileops_name・fileops_scope・fileops_service・shortcuts・path_format・shell_escape)、`crates/arin-platform/tests/`(fs・fs_race・watch・desktop・shell_roundtrip、trash は `#[ignore]`)、`src-tauri/tests/`(tree_commands・tree_watch・fileops_commands・desktop_commands・app_menu) | cargo test |
| 型の生成物の一致 | `crates/arin-core/tests/generated_types.rs` | cargo test |
| スクリプト | `scripts/*.test.mjs`(default-shortcuts は見本の表と生成物の一致) | node --test |
| 画面 E2E | `e2e/*.spec.ts`(app-shell、terminal、projects、tabs、m2-scenario、tree、tree-ops、tree-dnd、m3-scenario、tiles、m4-scenario) | playwright(WebKit) |
| 画面側の性能の計測 | `e2e/perf-tiles.spec.ts`(`npm run e2e` には入らない。`ARIN_PERF_E2E=1` のときだけ) | `npm run perf` |
| 実アプリのスモーク | `scripts/smoke.mjs`(`--tiles` で 9 プロジェクトのタイル表示) | `npm run smoke` |

## ドキュメント責務

| ファイル                      | 責務                                                   |
| ----------------------------- | ------------------------------------------------------ |
| `docs/project.md`             | ストア一覧、コマンド、技術スタック                     |
| `docs/architecture.md`        | ディレクトリ構成、境界、テスト構成                     |
| `docs/data-model.md`          | スキーマ定義、フィールド仕様、バリデーションルール     |
| `docs/development-patterns.md`| コード規約、落とし穴                                   |
