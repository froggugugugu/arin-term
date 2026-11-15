# プロジェクト情報

## 概要

macOS 向けのライト・マルチターミナル「Arin」(Tauri 2、arm64、最低 OS 15.0)。M0(骨格)と M1(ターミナル 1 枚。ログインシェル・PTY・xterm.js・コピー/ペースト)の実装が済み、M1 は本人が Go と判定した(`output/design/ADR_001_terminal-engine.md`)。
M2(プロジェクトの登録・カード一覧・タブ・ペインの分割・`state.json` の保存・ルートへの追従・終了の確認)と M3(ファイルツリー・ファイル操作・ドラッグ・ショートカット)の実装が済み、M3 は本人の完了確認待ち。タイル表示・設定ファイルは未実装。設計は `output/design/ARCH_multi-terminal.md`、進捗は `output/tasks/PROGRESS.md`。

## コマンド

正本は `package.json` の scripts。

```bash
npm run dev            # Tauri の開発起動(Vite は 1420 番)
npm run dev:web        # Vite の開発サーバーだけ
npm run build          # tsc -b && vite build && cargo build --workspace
npm test               # vitest + cargo test + node --test "scripts/*.test.mjs"
npm run lint           # tsc -b + biome lint(フックの 2 規則)+ clippy -D warnings + cargo fmt --check + 境界検査
npm run test:coverage  # 行 80% を src/logic と arin-core に課す(testreport/coverage/)
npm run gen:types      # Rust の型から src/ipc/generated/ を再生成
npm run e2e            # Playwright(WebKit、Vite の画面に対して)
npm run app            # 配布用 .app(tauri build --target aarch64-apple-darwin)
npm run app:debug      # デバッグ版の .app(target/aarch64-apple-darwin/debug/bundle/macos/。[arin:verify] が出る)
npm run smoke          # .app を一時の状態フォルダで起動し準備完了とターミナルの出力を確認(先に npm run app)
                       #   -- --env=NAME=VALUE(繰り返し可)で環境変数を足す、-- --app=<パス> で .app を指定
npm run perf           # 性能の計測(記録だけで失敗しない。結果は testreport/perf/。先に npm run app)
                       #   -- --tree-files=<数> でツリーの見本のファイル数(既定 100000、100 以上)
```

起動時の指定(検証用。リリースビルドでも有効。既定は DOM 描画と `unicode11`。M4 で外すか設定に移す): `ARIN_RENDERER`(`dom`/`webgl`)、`ARIN_UNICODE`(`unicode11`/`graphemes`)、`ARIN_VERIFY_GRID`(`1` で 9 枚の検証画面)。不正な値は既定に戻し標準エラーに `[arin:launch]` で出す。

## 技術スタック

| カテゴリ | 技術 |
| -------- | ---- |
| アプリ基盤 | Tauri 2.12.5(Rust 1.98.1、edition 2024) |
| 画面 | React 19.3.1、zustand 5.0.16(vanilla ストア)、Vite 8.3.5、TypeScript 6.0.3 |
| ターミナル | @xterm/xterm 6.0.0(addon-fit 0.11.0、addon-unicode11 0.9.0、addon-unicode-graphemes 0.4.0、addon-webgl 0.19.0)、portable-pty 0.9.0 |
| ファイルの監視 | notify 8.2.1(FSEvents) |
| macOS の API(`arin-platform` だけ) | objc2 0.6.5、objc2-foundation 0.3.2、objc2-app-kit 0.3.2、libc 0.2.191、base64 0.23.1(どれも Tauri と同じ版。ADR-009) |
| フックの検査 | @biomejs/biome 2.6.4(`useExhaustiveDependencies`・`useHookAtTopLevel` だけ。ADR-008) |
| 型の生成 | ts-rs 12.0.1(Rust → `src/ipc/generated/`) |
| テスト | Vitest 5.0.5、cargo test、node --test、Playwright 1.64.1(WebKit)、cargo-llvm-cov |
| 実行環境 | Node 24 以上 |

## ルーティング

なし(単一ウィンドウ。ルート部品は `AppShell`)。

## ストア一覧

| ストア | 責務 |
| ------ | ---- |
| `appStore`(`src/state/appStore.ts`) | Rust から届いた `AppSnapshot` を丸ごと置き換えて持つ。起動失敗の原因も持つ |
| `treeStore`(`src/state/treeStore.ts`) | ファイルツリーの一覧・開閉・選択(`tree-changed` で読み直す) |
| `uiStore`(`src/state/uiStore.ts`) | 表示だけの状態(終了の確認・削除の確認・カードとツリーのメニュー・名前の入力・衝突の確認・ショートカットの表・閉じた知らせ)。Rust には送らない |

## 制約事項

- パスエイリアス: なし(相対 import)
- E2E テスト: `npm run e2e`。CI(`.github/workflows/ci.yml`、macos-26)は lint・test・build・e2e を実行
- Git Hooks: なし
