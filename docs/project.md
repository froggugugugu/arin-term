# プロジェクト情報

## 概要

macOS 向けのライト・マルチターミナル「Arin」(Tauri 2、arm64、最低 OS 15.0)。M0(骨格)と M1(ターミナル 1 枚。ログインシェル・PTY・xterm.js・コピー/ペースト)の実装が済み、M1 は本人が Go と判定した(`output/design/ADR_001_terminal-engine.md`)。
M2(プロジェクトの登録・カード一覧・タブ・ペインの分割・`state.json` の保存・ルートへの追従・終了の確認)、M3(ファイルツリー・ファイル操作・ドラッグ・ショートカット)、M4(タイル表示・ページ送り・拡大・アテンションの印)の実装が済み、M2〜M4 は本人の完了確認待ち。M4.5(ショートカットの設定)の実装が済み、本人の完了確認待ち(ADR-012)。M4.6(文字の大きさの設定画面。値は `state.json`)の実装が済み、本人の完了確認待ち(ADR-013)。M4.7(透過・タイルの番号キー・Shift+Enter の改行。ADR-014)と M4.8(`.md`・`.markdown` だけのエディタのタブ。編集・プレビュー・保存・Vim の切替。ADR-015)の実装が済み、本人の完了確認待ち。描画は DOM、全角幅は `unicode11` に固定(ADR-001)。設定ファイル `settings.toml` は M4.5 で `[shortcuts]` だけ読む(ほかの設定は未実装)。設計は `output/design/ARCH_multi-terminal.md`、進捗は `output/tasks/PROGRESS.md`。

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
npm run screenshots    # README と紹介ページの画像を docs/media/ に撮り直す(e2e/screenshots/。npm run e2e には入らない)
npm run screenshots:pages  # 紹介ページ(.github/pages/index.html)を幅 390・1280、ライト・ダークで開き、横にはみ出さないかを見て testreport/pages/ に撮る
npm run app            # 配布用 .app(tauri build --target aarch64-apple-darwin)
npm run app:debug      # デバッグ版の .app(target/aarch64-apple-darwin/debug/bundle/macos/。[arin:verify] が出る)
npm run smoke          # .app を一時の状態フォルダで起動し準備完了とターミナルの出力を確認、続けて壊れた state.json で起動(先に npm run app)
                       #   -- --tiles で 9 プロジェクトのタイル表示で 1 回だけ起動し、シェルが 9 個揃うまで確認
                       #   -- --settings で、設定ファイルを置いた一時の状態フォルダで 1 回起動し、設定の診断の行と、設定ファイルが変わっていないことを確かめる
                       #   -- --fonts で、文字の大きさを入れた state.json で 1 回起動する
                       #   -- --env=NAME=VALUE(繰り返し可)でアプリに環境変数を足す、-- --app=<パス> で .app を指定
npm run perf           # 性能の計測(記録だけで失敗しない。結果は testreport/perf/。先に npm run app)
                       #   -- --tree-files=<数> でツリーの見本のファイル数(既定 100000、100 以上)
npm run dist:mac       # 配布物 release/Arin-<版>-arm64.dmg・.zip(署名の環境変数が無ければ ad-hoc で最後まで作る。
                       #   Developer ID の署名・公証は本人だけが実行する。docs/release-notarization.md)
npm run licenses       # 配布物に入る依存のライセンス全文 THIRD_PARTY_LICENSES.md を作り直す(依存を変えたら実行してコミット)
npm run licenses:check # THIRD_PARTY_LICENSES.md が依存と合っているかだけ確かめる(違えば失敗。release.yml が実行)
```

アプリ独自の環境変数は `ARIN_STATE_DIR`(状態フォルダの差し替え。絶対パス)と `ARIN_SMOKE`(`1` でスモークの印。差し替え時だけ)だけ。M1 の検証用の `ARIN_RENDERER`・`ARIN_UNICODE`・`ARIN_VERIFY_GRID` は M4 で外した(指定しても読まない)。

## 技術スタック

| カテゴリ | 技術 |
| -------- | ---- |
| アプリ基盤 | Tauri 2.12.5(Rust 1.98.1、edition 2024) |
| 画面 | React 19.3.1、zustand 5.0.16(vanilla ストア)、Vite 8.3.5、TypeScript 6.0.3 |
| ターミナル | @xterm/xterm 6.0.0(DOM 描画。addon-fit 0.11.0、addon-unicode11 0.9.0)、portable-pty 0.9.0 |
| Markdown エディタ(`src/editor/codemirror/` だけ) | codemirror 6.0.2、@codemirror/state 6.7.2・view 6.43.10・lang-markdown 6.5.2・language 6.12.4・commands 6.11.0・search 6.7.2、@lezer/markdown 1.7.2・highlight 1.2.3・common 1.5.2、@replit/codemirror-vim 6.4.0(推移的な @codemirror/lint は `overrides` で 6.9.7。ADR-015) |
| ファイルの監視 | notify 8.2.1(FSEvents) |
| 設定ファイルの解析(`arin-platform` だけ) | toml 1.2.1(`default-features = false`、`std`・`parse`。Tauri と同じ版。公開から 2 週間未満の例外を本人が認めた。ADR-012) |
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
| `uiStore`(`src/state/uiStore.ts`) | 表示だけの状態(終了の確認・削除の確認・カードとツリーのメニュー・名前の入力・衝突の確認・ショートカットの表・設定の知らせ・設定画面の開閉・閉じた知らせ。M4.8 でエディタのタブごとの表示の切替・帯・プレビューの木、閉じる確認、保存の衝突の確認、開いたあとのフォーカス)。Rust には送らない |

エディタの操作と写しへの追従は `src/state/editorController.ts`(ストアではない。`main.tsx` が作る)、部品は `src/editor/EditorManager.ts` が React の外で持つ。

## 制約事項

- パスエイリアス: なし(相対 import)
- E2E テスト: `npm run e2e`。CI(`.github/workflows/ci.yml`、macos-26)は lint・test・build・e2e を実行
- Git Hooks: なし
