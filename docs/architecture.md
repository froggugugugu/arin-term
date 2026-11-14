# アーキテクチャ

レイヤー定義と依存方向の全体は `output/design/ARCH_multi-terminal.md` §3、最終形のディレクトリは §4、テスト方針は §10 と ADR-006。ここには M0 時点で実在するものだけを書く。

## エントリーポイント

- Rust: `src-tauri/src/main.rs` → `lib.rs`。`launch.rs` が環境変数(`ARIN_STATE_DIR`・`ARIN_SMOKE`)を読み、`commands/` が画面からのコマンド(`get_app_snapshot`・`notify_app_ready`)を受ける
- 画面: `src/main.tsx` が `createAppStore()` でストアを作り、`startApp` で写しの取得と準備完了の通知を行い、`AppShell` に props で渡す

## ディレクトリ構成

```
crates/arin-core/      # Core: 型・規則。入出力なし(paths/、workspace/)
crates/arin-platform/  # Platform: OS への入出力(storage/ready_marker)
src-tauri/             # App: Tauri の組み立てとコマンド(クレート名 arin、lib は arin_lib)
src/
├── ipc/               # Rust との通信の包み。@tauri-apps/* はここだけ
│   └── generated/     # ts-rs の生成物(手で編集しない)
├── state/             # zustand ストアと起動手順
├── ui/shell/          # 画面の部品(AppShell)
├── logic/             # 純粋ロジック(DOM・他の src/* に依存しない。別 tsconfig)
└── test/              # テスト補助(fakeIpc)
e2e/                   # Playwright(fixtures/tauriMock.ts)
scripts/               # 境界検査・カバレッジ・スモーク(fixtures/ は違反の見本)
```

`src/terminal/` など設計書 §4 のそれ以外のディレクトリは未実装(M1 以降)。

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
| Rust 結合 | `crates/*/tests/`、`src-tauri/tests/`(`harness = false`、補助は `support/`) | cargo test |
| 型の生成物の一致 | `crates/arin-core/tests/generated_types.rs` | cargo test |
| スクリプト | `scripts/*.test.mjs` | node --test |
| 画面 E2E | `e2e/*.spec.ts` | playwright(WebKit) |
| 実アプリのスモーク | `scripts/smoke.mjs` | `npm run smoke` |

## ドキュメント責務

| ファイル                      | 責務                                                   |
| ----------------------------- | ------------------------------------------------------ |
| `docs/project.md`             | ストア一覧、コマンド、技術スタック                     |
| `docs/architecture.md`        | ディレクトリ構成、境界、テスト構成                     |
| `docs/data-model.md`          | スキーマ定義、フィールド仕様、バリデーションルール     |
| `docs/development-patterns.md`| コード規約、落とし穴                                   |
