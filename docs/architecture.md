# アーキテクチャ

レイヤー定義と依存方向の全体は `output/design/ARCH_multi-terminal.md` §3、最終形のディレクトリは §4、テスト方針は §10 と ADR-006。ここには M1 時点で実在するものだけを書く。

## エントリーポイント

- Rust: `src-tauri/src/main.rs` → `lib.rs`。`launch.rs` が環境変数を読み、`lib.rs` が `Workspace`・`SessionRegistry`・メニューを組み立て、`commands/`(`app.rs`・`terminal.rs`)が画面からのコマンドを受ける
- 画面: `src/main.tsx` が `createAppStore()` でストアを作り、`startApp` で写しの取得と準備完了の通知を行い、`AppShell` に props で渡す

## ディレクトリ構成

```
crates/arin-core/      # Core: 型・規則。入出力なし
│                      #   paths/、workspace/、terminal/(起動の仕様・状態・流量制御・OSC 走査)、ports/(PtyPort)、launch_options.rs
crates/arin-platform/  # Platform: OS への入出力(storage/ready_marker、pty/ = portable-pty による PtyPort の実装)
src-tauri/src/         # App: Tauri の組み立て(クレート名 arin、lib は arin_lib)
│                      #   sessions.rs(PTY ごとのスレッドと流量制御)、commands/、menu.rs、lifecycle.rs、
│                      #   diagnostics.rs([arin:frontend-error])、verify_log.rs([arin:verify]。開発ビルドだけ)
src/
├── ipc/               # Rust との通信の包み。@tauri-apps/* はここだけ
│   └── generated/     # ts-rs の生成物(手で編集しない)
├── state/             # zustand ストアと起動手順・エラー報告
├── terminal/          # エンジンの抽象(engine.ts)、TerminalManager、描画方式の切替
│   └── xterm/         # xterm.js の実装(XtermEngine)。@xterm/* はここだけ
├── ui/shell/          # AppShell など
├── ui/terminals/      # TerminalHost、コピー/ペーストの振り分け、9 枚の検証画面
├── logic/             # 純粋ロジック(DOM・他の src/* に依存しない。別 tsconfig。keys/ はクリップボードの判定)
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
| Cargo の依存宣言 | arin-platform は arin-core だけに依存。arin-core は serde・serde_json・thiserror・ts-rs だけ |
| TypeScript のプロジェクト参照(`tsconfig.json` → `src/logic`・`src`・`e2e`) | `src/logic` から他を import すると `tsc -b` が失敗 |
| `scripts/check-boundaries.mjs`(`npm run lint`) | arin-core の依存と `std::fs`/`process`/`net` 禁止、`@tauri-apps/*` は `src/ipc` だけ、`@xterm/*` は `src/terminal/xterm` だけ、`src/ui` → `src/ipc` 禁止、通信クレート・通信 API・HTML 差し込みの禁止、CSP の `connect-src` |

## テスト構成

| 種類 | 置き場所 | 実行 |
| ---- | -------- | ---- |
| TS 単体 | `src/**/*.test.ts`(コロケーション) | vitest(environment: node) |
| Rust 単体 | 各ソースの `#[cfg(test)]` | cargo test |
| Rust 結合 | `crates/*/tests/`、`src-tauri/tests/`(模擬ランタイムを使うものは `harness = false`、補助は `support/`) | cargo test |
| 実 PTY の結合 | `crates/arin-platform/tests/pty.rs`、`src-tauri/tests/session_pty.rs` | cargo test |
| 性能の計測 | `crates/arin-platform/tests/perf_pty.rs`、`src-tauri/tests/perf_sessions.rs`(すべて `#[ignore]`)、`scripts/perf/` | `npm run perf` |
| 型の生成物の一致 | `crates/arin-core/tests/generated_types.rs` | cargo test |
| スクリプト | `scripts/*.test.mjs` | node --test |
| 画面 E2E | `e2e/*.spec.ts`(app-shell、terminal、terminal-webgl、verify-grid) | playwright(WebKit) |
| 実アプリのスモーク | `scripts/smoke.mjs` | `npm run smoke` |

## ドキュメント責務

| ファイル                      | 責務                                                   |
| ----------------------------- | ------------------------------------------------------ |
| `docs/project.md`             | ストア一覧、コマンド、技術スタック                     |
| `docs/architecture.md`        | ディレクトリ構成、境界、テスト構成                     |
| `docs/data-model.md`          | スキーマ定義、フィールド仕様、バリデーションルール     |
| `docs/development-patterns.md`| コード規約、落とし穴                                   |
