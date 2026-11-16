# 設計ドキュメント: ライト・マルチターミナル「Arin」実装計画

> 生成元: `output/prd/PRD_multi-terminal.md`(承認済み。FR-001〜064、NFR-001〜009)、`output/design/ARCH_multi-terminal.md`(承認済み)
> 生成日: 2026-11-14
> ステータス: レビュー待ち(タスク分解ゲート。承認されるまで実装に着手しない)
> 進捗の引き継ぎ: `output/tasks/PROGRESS.md`

**この文書の読み方**

- マイルストーンは PRD §9 の M0〜M6 をそのまま使う(`PRD_multi-terminal.md:689-701`)。実行順は M0→M1→M2→M3→M4→M5→M6。各マイルストーンの完了条件と人間ゲートは変えていない。
- **M0 と M1 は 1 タスク=1 セッションで終わる粒度まで分解した(T1〜T27)。M2 以降(T28〜T62)は粗い分解にとどめた。** M1 の Go/No-Go の結果(描画方式、CSP、OSC 52、実装技術の見直しの有無)で中身が変わりうるため。各マイルストーンの最初のタスクで `/plan` を再実行し、1 セッション粒度に分解し直す。
- タスク ID は全体で通し番号(T1〜T62。2026-11-15 の要件の追加で M4.5 の T63〜T66 を足した。既存の番号は変えていない)。Phase も全体で通し番号で、Phase 内のタスクは同時に着手できる。
- 変更ファイルはリポジトリのルートからの相対パスで書いた(`src/` のほかに `crates/`、`src-tauri/`、`scripts/`、`e2e/` があるため)。
- 「検証」は pass/fail を返すコマンド。入口は設計書 §13.1 の npm スクリプト(`ARCH_multi-terminal.md:884-895`)。npm スクリプトは T7 で用意するので、T2〜T6 だけは中身のコマンドを直接書いた。
- 「未検証」は、この計画の作成時に動かして確かめていないこと。**本計画の作成では、コマンドの実行による確認は何もしていない**(読んだのは PRD、設計書、`project-config.md`、雛形だけ)。React・zustand などの最新版の番号も調べていない。
- 【要確認】はユーザーの判断が要る箇所、【仮定】は本計画が置いた前提。§リスク・懸念事項に集約した。
- 性能値 NFR-001〜005・009 は目安で、合否判定に使わない。計測タスクは記録するだけ。
- 非ゴール N1〜N7 に触れるタスクは無い。将来要望の簡易 Markdown エディタは N1・N7 に当たるため、タスクに入れていない。

**全タスク共通の決まり**

- TDD: 「先に書くテスト」を先に書き、失敗を確認してから実装する。
- 1 セッション 1 タスク。着手前に `npm run smoke`(T10 より前は `npm test`、T7 より前は各タスクの検証コマンド)を通す。終了時は検証コマンドが緑で、`output/tasks/PROGRESS.md` を更新する。コミットと push は人間の指示を受けてから行う。
- ファイルを触るテストは一時ディレクトリの中だけで行う。状態と設定の置き場所は環境変数で差し替え、本物の `~/Library/Application Support/Arin/` を読み書きしない(設計書 §10.2)。
- 依存を足すときは正確な版を指定し、理由を ADR に残す(PRD §7 C6)。
- `docs/*.md` と `project-config.md` §2・§3・§11 の同期は、各マイルストーンの完了確認タスクで `/implementing-features` がまとめて行う。

## 要件サマリー

- M0(骨格): CI が緑で、`npm run app` で作った `.app` が起動する(空ウィンドウ)
- M1(ターミナル 1 枚): FR-039(新規タブの初期 cwd)、FR-040、FR-041、FR-043 を満たし、Claude Code と Codex が実用に耐えるかを人間が判定する(Go/No-Go)
- M1 で確かめる項目(設計書 §2.2 の 12 行)をすべて記録し、結果を ADR-001 に残す
- M2(マルチプロジェクト): FR-001〜004、007〜010、042、044、056、059〜061(タブ表示の部分)、063、064 の AC に合格する
- M3(ツリーとファイル操作): FR-013〜018(Quick Look を除く)、024〜030、035〜037 の AC に合格し、VSCode を閉じて 1 週間ドッグフーディングする(人間ゲート)
- M4(タイル表示): FR-048、051〜055、FR-060・061 のタイル部分の AC に合格し、9 枚同時表示の体感を人間が判定する
- M4.5(ショートカットの設定。【要件の追加(本人。2026-11-15)】PRD に未反映): `settings.toml` でショートカットのキーを変えられる。設定ファイルの土台を M6 の T56 から前倒しする
- M5(公証・配布): 別ユーザーアカウントで Gatekeeper の警告なく起動する。秘密情報を扱う作業は人間が行う
- M6(日常の磨き): FR-005、006、011、012、019〜021、031〜034、038、045〜047、049、057、058、062、FR-018 の Quick Look の AC に合格し、ドッグフーディングの不満リストが空になる
- FR-022、FR-023、FR-050(Could)は M6 以降で、本計画ではタスクにしない
- NFR-006 は M1(T15)と M2(T35)の結合テストで検証する。NFR-007 は M0(T5)の設定で満たす。NFR-008 は M0(T6)から `npm run lint` で常時検査し、M5 でエンタイトルメントを確認する(PRD §9 が `/plan` に決定を委ねた点。`PRD_multi-terminal.md:707`)
- NFR-001〜003 は M2 と M6 の完了時、NFR-004 は M1、NFR-005 は M3、NFR-009 は M1(簡易)と M4 に計測して記録する(合否にしない)

## 影響調査

既存のソースコードは無い(リポジトリにあるのはハーネスと文書だけ)。下の表はすべて新規追加。

| カテゴリ | ファイル | 変更内容 |
| --- | --- | --- |
| 設定 | `.gitignore` | 追加: `node_modules/`、`target/`、`dist/` など(現状はハーネス用の行だけ) |
| 設定 | `Cargo.toml`、`Cargo.lock`、`rust-toolchain.toml` | 追加: Rust のワークスペースと版の固定(M0) |
| 設定 | `package.json`、`package-lock.json`、`tsconfig.json`、`vite.config.ts`、`index.html`、`playwright.config.ts` | 追加: 画面側のビルドとテストの設定(M0) |
| 設定 | `src-tauri/tauri.conf.json`、`src-tauri/capabilities/`、`src-tauri/icons/` | 追加: ウィンドウ、CSP、Bundle ID `com.froggugugugu.arin`、最低 OS 15.0、arm64(M0)。CSP は M1 で変更 |
| 設定 | `.github/workflows/ci.yml`、`.github/workflows/release.yml` | 追加: CI(M0)、tag のときの公証(M5) |
| 設定 | `scripts/check-boundaries.mjs`、`scripts/smoke.mjs`、`scripts/perf/`、`scripts/package-mac.sh` | 追加: 境界検査(M0)、起動スモーク(M0)、計測(M1〜)、署名・公証(M5。人間だけが実行) |
| スキーマ | `crates/arin-core/src/persistence/`、`crates/arin-core/src/settings/` | 追加: `state.json` 版 1(M2)、`settings.toml`(M6) |
| スキーマ | `src/ipc/generated/` | 追加: Rust の型から生成した TypeScript の型(M0 で配管、以降は自動生成) |
| ストア | `crates/arin-core/src/{project,terminal,layout,settings,workspace}/` | 追加: `ProjectStore`(M2)、`TerminalStore`(M1 で最小、M2 で複数化)、`LayoutStore`(M2、M4)、`SettingsStore`(M6)、`Workspace` |
| ストア | `src/state/` | 追加: `appStore`(M0 で最小)、`uiStore`(M2)、`treeStore`(M3)。zustand |
| ユーティリティ | `crates/arin-core/src/{paths,tree,fileops,ports}/` | 追加: パス整形とシェルエスケープ、除外の規則、ファイル操作の手順、ポート定義(M1〜M3) |
| ユーティリティ | `crates/arin-platform/src/{pty,fs,watch,bookmark,storage,desktop,git,process}/` | 追加: ポートの実装(M1: pty。M2: bookmark、storage。M3: fs、watch、desktop。M6: git、process) |
| ユーティリティ | `src/logic/{tree,dnd,keys,menu}/`、`src/ipc/`、`src/terminal/`、`src/terminal/xterm/` | 追加: 画面の純粋ロジック(M3 中心)、通信の包み(M0〜)、エンジンの抽象と xterm.js の実装(M1) |
| ユーティリティ | `src-tauri/src/{commands,events.rs,sessions.rs,menu.rs,lifecycle.rs,lib.rs,main.rs}` | 追加: コマンド、イベント、セッション管理、メニューバー、起動と終了 |
| コンポーネント | `src/ui/{shell,projects,tree,terminals,dialogs,menu,design}/` | 追加: React の部品。`shell`(M0 で空、M2 で分割)、`terminals`(M1、M2、M4)、`projects`・`dialogs`(M2)、`tree`・`menu`(M3) |
| ページ | `src/main.tsx`、`src/ui/shell/AppShell.tsx` | 追加: 単一ウィンドウの入口(ルーティングは無い)。設計書 §4 は `main.ts` と書いており、T1 で直す |
| テスト | `crates/arin-core/tests/`、`crates/arin-platform/tests/`、`src-tauri/tests/`、`src/**/*.test.ts`、`e2e/`、`scripts/*.test.mjs` | 追加: 単体、結合(実 PTY・実ファイル・コマンド)、画面の自動テスト、検査スクリプトのテスト |
| ドキュメント | `output/design/ARCH_multi-terminal.md` §4、`output/design/ADR_001`〜`ADR_007`、`output/design/ADR_INDEX.md` | 変更: §4 を React 前提に直す(T1)。追加: ADR(T1、T2、T27、T37) |
| ドキュメント | `docs/project.md`、`docs/architecture.md`、`docs/data-model.md`、`docs/development-patterns.md`、`docs/release-notarization.md` | 変更: 各マイルストーンの完了時に `/implementing-features` が同期する(本計画では変更しない) |

## タスク分解

### M0 骨格(完了条件: CI 緑、`npm run app` で作った `.app` が起動する)

#### Phase 1(逐次。最初に 1 つだけ)

- [x] T1 — 設計書 §4 を React 前提に直し、画面側の依存の版を決めて ADR-003 に残す(変更ファイル: output/design/ARCH_multi-terminal.md, output/design/ADR_003_ui-framework.md, output/design/ADR_INDEX.md | 依存: なし)
  - 対応: 設計書の冒頭が「M0 の着手時に React 前提へ直す」とした点(`ARCH_multi-terminal.md:11`)、版の固定(同 147 行)、PRD §7 C6
  - 直す内容: `src/main.ts` → `src/main.tsx`。`src/ui/` の部品を `.tsx` に。`src/state/` を zustand のストアに。`tsconfig` の分け方(`src/logic` は DOM と JSX なし、それ以外は JSX あり)。Vite の React プラグイン。React の部品を Vitest で単体テストするか(しないなら Playwright だけで見る)
  - 決める版: `react`、`react-dom`、`zustand`、`@vitejs/plugin-react`、`@types/react`、`@types/react-dom`。`npm view <名前> version` で調べ、正確な版を ADR-003 に書く。TypeScript 6.0 系との組み合わせで型が通るかは T4 で確かめる(未検証)
  - あわせて決める点(§リスク #1・#2): `ts-rs` を `arin-core` の依存に加えてよいか。境界検査の禁止リストに React の `dangerouslySetInnerHTML` を足すか
  - 並行: 不可(T2〜T4 の前提)
  - 先に書くテスト: なし(文書だけの変更)
  - 検証: `grep -nE 'main\.ts([^x]|$)' output/design/ARCH_multi-terminal.md` が 0 件。ADR-003 に 6 つの依存の正確な版が書いてある
  - 人間の確認: §4 の差分と ADR-003 を本人が読んで了承する(承認済みの設計書を直すため)

#### Phase 2(並行可能。Phase 1 完了後)

- [x] T2 — 設計ゲートで確定した判断を ADR-002・004・005・006 に起こす(変更ファイル: output/design/ADR_002_logic-placement.md, output/design/ADR_004_pty.md, output/design/ADR_005_settings-format.md, output/design/ADR_006_test-structure.md, output/design/ADR_INDEX.md | 依存: T1)
  - 対応: 設計書 §14 の ADR の候補(`ARCH_multi-terminal.md:939-949`)、PRD §7 C6
  - 並行: 可(コードを触らない。`ADR_INDEX.md` は T1 が作ったものに行を足すだけ)
  - 先に書くテスト: なし
  - 検証: `ls output/design/ADR_00{2,4,5,6}_*.md` が 4 件。`/adr` で作る
- [x] T3 — Rust のワークスペースと 2 つのクレートの骨格を作る(変更ファイル: Cargo.toml, Cargo.lock, rust-toolchain.toml, .gitignore, crates/arin-core/Cargo.toml, crates/arin-core/src/lib.rs, crates/arin-platform/Cargo.toml, crates/arin-platform/src/lib.rs | 依存: T1)
  - 対応: 設計書 §3.1・§3.2・§4。`arin-core` は `#![forbid(unsafe_code)]`、依存は `serde`・`serde_json`・`thiserror`(と T1 で認めたもの)。Rust は 1.98.1 に固定し、`clippy`・`rustfmt`・`llvm-tools-preview` を入れる。リリースのプロファイルに `panic = "abort"` を書かない(NFR-006)
  - `.gitignore` に `node_modules/`、`target/`、`dist/` を足す(T4 のぶんもここでまとめて足す。同じファイルを 2 つのタスクで触らないため)
  - 並行: 可(T4 とファイルが重ならない)
  - 先に書くテスト: `arin-core` に 1 件(クレートが読み込めることを確かめるだけの最小のテスト)。テストの土台が動くことの確認
  - 検証: `cargo build --workspace --locked && cargo test --workspace --locked && cargo clippy --workspace --all-targets --locked -- -D warnings && cargo fmt --check`
- [x] T4 — 画面の骨格(React、Vite、TypeScript のプロジェクト参照、Vitest)を作る(変更ファイル: package.json, package-lock.json, index.html, vite.config.ts, tsconfig.json, src/tsconfig.json, src/logic/tsconfig.json, src/main.tsx, src/ui/shell/AppShell.tsx, src/logic/index.ts, src/logic/index.test.ts | 依存: T1)
  - 対応: 設計書 §3.3(`src/logic` の独立をプロジェクト参照で強制)、§13.1 の「M0 で入れるもの」。開発用依存(`@tauri-apps/cli`、`vite`、`vitest`、`@vitest/coverage-v8`、`typescript`、`@playwright/test`、React 関係)と `@tauri-apps/api` をここでまとめて入れる(あとのタスクが `package.json` を触らずに済むようにする)。版は T1 の ADR-003 と設計書 §2 のとおりに正確に指定する
  - 並行: 可(T3 とファイルが重ならない)
  - 先に書くテスト: `src/logic/index.test.ts`(最小の 1 件)。加えて、`src/logic` から `src/ui` を import した一時ファイルで `tsc -b` が失敗することを手で 1 回確かめ、結果を PROGRESS に書く(一時ファイルは残さない)
  - 検証: `npm ci --ignore-scripts && npx tsc -b && npx vitest run && npx vite build`

#### Phase 3(並行可能。Phase 2 完了後)

- [x] T5 — Tauri のアプリ本体(`src-tauri`)を作り、空ウィンドウを出す(変更ファイル: Cargo.toml, Cargo.lock, src-tauri/Cargo.toml, src-tauri/build.rs, src-tauri/tauri.conf.json, src-tauri/capabilities/default.json, src-tauri/icons/, src-tauri/src/main.rs, src-tauri/src/lib.rs | 依存: T3, T4)
  - 対応: NFR-007(最低 OS 15.0、arm64 だけ)、NFR-008(CSP の `connect-src`、通信できるプラグインを入れない)、設計書 §1.3 #6(`backgroundThrottling` を `disabled`。効くかどうかは M1 で確かめる)、§12(`withGlobalTauri` は false)。Bundle ID は `com.froggugugugu.arin`、製品名は `Arin`。Tauri は 2.12 系で正確な版を指定する
  - 並行: 可(T6 とファイルが重ならない)
  - 先に書くテスト: `src-tauri/tests/` に、アプリの組み立てが失敗しないことを確かめる 1 件(Tauri の模擬ランタイムを使う。Tauri 2.12 系で使えるかは未検証。使えなければ `lib.rs` の組み立て関数の単体テストに替え、その旨を PROGRESS に書く)
  - 検証: `cargo build --workspace --locked && cargo test --workspace --locked && npx tauri build --debug --no-bundle`(最後のコマンドのオプションは未検証。通らなければ `npx tauri build` に替える)
- [x] T6 — 境界検査スクリプトとその単体テストを書く(変更ファイル: scripts/check-boundaries.mjs, scripts/check-boundaries.test.mjs, scripts/fixtures/ | 依存: T3, T4)
  - 対応: 設計書 §3.3 の検査(`ARCH_multi-terminal.md:244-254`)、NFR-008。検査する項目: `arin-core` の依存に禁止クレートが無い。`arin-core` のソースに `std::fs`・`std::process`・`std::net` が無い。`@xterm/*` は `src/terminal/xterm` だけ。`@tauri-apps/api` は `src/ipc` だけ。`src/ui` が `src/ipc` と `src/terminal/xterm` を import しない。`src/terminal/xterm` が `src/ipc` を import しない。HTTP クライアントのクレートが組み込まれていない。画面のソースに `fetch(`・`XMLHttpRequest`・`WebSocket`・`EventSource`・`sendBeacon` が無い。`innerHTML`・`outerHTML`・`insertAdjacentHTML`・`document.write`(と T1 で決めたら `dangerouslySetInnerHTML`)が無い。CSP に `connect-src` の制限がある
  - 並行: 可(T5 とファイルが重ならない。CSP の検査は `src-tauri/tauri.conf.json` を読むだけなので、ファイルが無いあいだは「未作成」として失敗させ、T5 の完了後に通ることを T7 で確かめる)
  - 先に書くテスト: `scripts/check-boundaries.test.mjs`。違反を仕込んだ見本(`scripts/fixtures/`)ごとに「失敗する」こと、違反の無い見本で「通る」ことを `node --test` で確かめる
  - 検証: `node --test scripts/`

#### Phase 4(逐次。Phase 3 完了後)

- [x] T7 — 検証コマンドを npm スクリプトにまとめる(変更ファイル: package.json | 依存: T5, T6)
  - 対応: PRD §7「ワンコマンド検証」(ビルド・テスト・lint・アプリ生成)、設計書 §13.1 の表のとおりに `build`、`test`、`lint`、`app`、`dev`、`test:coverage`、`e2e`、`smoke`、`perf` を登録する。`dist:mac` は M5(T51)で足す
  - `test:coverage`、`e2e`、`smoke`、`perf` は名前と中身を登録するだけで、動くようになるのは T9〜T11 と T24。それまでは実行すると失敗する(通ったふりをする仮の中身を置かない)
  - 並行: 不可(以降のタスクの検証の入口になる)
  - 先に書くテスト: なし(設定だけ)。代わりに、わざと `cargo fmt` に反する 1 行を入れて `npm run lint` が失敗することを 1 回確かめ、元に戻す
  - 検証: `npm run build && npm test && npm run lint && npm run app`(4 つとも終了コード 0。`npm run app` のあと `.app` があること)

#### Phase 5(並行可能。Phase 4 完了後)

- [x] T8 — Rust と TypeScript の型合わせ(`ts-rs`)と、通信の包みの最小を作る(変更ファイル: crates/arin-core/Cargo.toml, crates/arin-core/src/workspace/, src-tauri/src/commands/app.rs, src-tauri/src/lib.rs, src/ipc/, src/ipc/generated/, src/state/appStore.ts, src/test/fakeIpc.ts, Cargo.lock | 依存: T7)
  - 対応: 設計書 §15.1 #7(`ts-rs` で生成)、§5.2(状態の写し `AppSnapshot`、準備完了の通知、写しの取得)。`AppSnapshot` は中身が空に近い最小の形でよい
  - 型の置き場所は T1 の決定に従う(§リスク #1)
  - 並行: 可(T9 とファイルが重ならない)
  - 先に書くテスト: Rust は「写しの取得コマンドが `AppSnapshot` を返す」(`src-tauri/tests/`)。TypeScript は「`src/ipc` の包みが偽の通信に正しい名前と引数で届く」「`appStore` が届いた写しで丸ごと置き換わる」(Vitest)。生成した型が最新であること(生成し直して差分が出ない)を確かめるテストも足す
  - 検証: `npm test && npm run lint && npm run build`
- [x] T9 — カバレッジ付きテストを用意する(変更ファイル: vite.config.ts, scripts/coverage.mjs | 依存: T7)
  - 対応: C-15、設計書 §15.1 #8(80% を `arin-core` と `src/logic` のそれぞれで見る)。出力先は `testreport/coverage/`
  - `cargo-llvm-cov` の導入(`cargo install cargo-llvm-cov`)は手元の環境に道具を足すので、実行前に本人に伝える。この Mac には入っていない(`ARCH_multi-terminal.md:117`)
  - 並行: 可(T8 とファイルが重ならない)
  - 先に書くテスト: なし(道具の設定)。代わりに、しきい値を一時的に 100% にして失敗することを 1 回確かめる
  - 検証: `npm run test:coverage`(終了コード 0。`testreport/coverage/` に Rust と TypeScript の結果がある)

#### Phase 6(並行可能。Phase 5 完了後)

- [x] T10 — 実アプリの起動スモークを作る(変更ファイル: scripts/smoke.mjs, crates/arin-core/src/paths/app_paths.rs, crates/arin-core/src/lib.rs, src-tauri/src/lib.rs, src-tauri/src/commands/app.rs | 依存: T8)
  - 対応: 設計書 §10.1「実アプリのスモーク」、§11(置き場所 `AppPaths` を環境変数で差し替える)。ビルドした `.app` を一時の状態フォルダで起動し、画面が準備完了を通知したことを確かめて終了させる
  - 準備完了を外から知る方法は、ソケットを開かない方式にする(N2、NFR-008)。【仮定】スモーク用の環境変数があるときだけ、状態フォルダに印のファイルを書く。環境変数の名前はこのタスクで決める
  - 並行: 可(T11 とファイルが重ならない)
  - 先に書くテスト: `arin-core` の「置き場所の決定」(差し替えの指定があればそれを使い、無ければ `~/Library/Application Support/Arin/` になる。入力は引数で渡し、`arin-core` が環境変数を直接読まない)
  - 検証: `npm run app && npm run smoke`(終了コード 0。本物の `~/Library/Application Support/Arin/` が作られていないこと)
- [x] T11 — 画面の自動テスト(Playwright。ブラウザ方式、WebKit)の骨格を作る(変更ファイル: playwright.config.ts, e2e/fixtures/tauriMock.ts, e2e/app-shell.spec.ts, src/main.tsx | 依存: T8)
  - 対応: 設計書 §10.3、§15.1 #6。Vite の開発サーバーが配る画面を WebKit で開き、Rust との通信を偽物に差し替える。偽物は T8 で生成した型を使う。出力先は `testreport/e2e/`
  - `npx playwright install webkit` を実行する(ブラウザのダウンロードが起きるので、実行前に本人に伝える)
  - 並行: 可(T10 とファイルが重ならない)
  - 先に書くテスト: `e2e/app-shell.spec.ts`(画面を開くと `AppShell` が表示され、偽の Rust 側に準備完了が 1 回届く)
  - 検証: `npm run e2e`

#### Phase 7(逐次。Phase 6 完了後)

- [x] T12 — CI(push と pull request で build / test / lint / e2e)を書く(変更ファイル: .github/workflows/ci.yml | 依存: T7, T9, T10, T11)
  - 対応: PRD §7「CI」、設計書 §13.3。`macos-26` で `npm ci --ignore-scripts`、`npm run lint`、`npm test`、`npm run build`、`npm run e2e`。ランナーとアクションの版を固定する。公証は入れない(M5)
  - `macos-26` のランナーに Rust 1.98.1 と Playwright の WebKit を入れる手順が通るかは未検証
  - 並行: 不可
  - 先に書くテスト: なし
  - 検証: 手元では `npm run lint && npm test && npm run build && npm run e2e`。CI 自体の確認は push が要る(T13 で人間が行う)

#### Phase 8(逐次。人間ゲート)

- [ ] T13 — M0 の完了確認(変更ファイル: output/tasks/PROGRESS.md, docs/project.md, docs/architecture.md, project-config.md | 依存: T2, T12)
  - AI がやること: `npm run build && npm test && npm run lint && npm run app && npm run smoke && npm run e2e` の結果をまとめ、完了条件の合否一覧を `testreport/` に出す。`docs/project.md`・`docs/architecture.md`・`project-config.md` §2・§3 を実装に合わせる(`/implementing-features` の責務)
  - **人間がやること**: (1) `npm run app` で作った `.app` を開き、空ウィンドウが出ることを見る。(2) push して CI が緑になることを見る(push は毎回本人が確認する)
  - 完了条件: PRD §9 の M0 の完了条件(CI 緑、`.app` が起動する)

### M1 ターミナル 1 枚(完了条件: Claude Code と Codex が実用に耐えるかを人間が判定する)

M1 は検証専用(PRD §7)。対象は単一プロジェクト・単一タブで、プロジェクトの登録は M2。【仮定】M1 のターミナルの cwd はホームフォルダにする(§リスク #5)。FR-039 のうち M1 で見るのは「新規タブの初期 cwd が指定のフォルダになる」だけで、複数タブと「起動時に全プロジェクトで 1 タブ」は M2(T32)で扱う。

#### Phase 9(並行可能。M0 完了後)

- [x] T14 — ログインシェルの起動の仕様と PTY のポートを `arin-core` に書く(変更ファイル: crates/arin-core/src/terminal/, crates/arin-core/src/ports/, crates/arin-core/src/lib.rs | 依存: T13)
  - 対応: FR-040、FR-039(初期 cwd)。設計書 §7.1「ターミナルの起動」(`ARCH_multi-terminal.md:605-622`)。`ShellLaunchSpec`(シェルのパス、`argv[0]` を `-zsh` の形に、cwd、許可リストから組み立てた環境変数)と `PtyPort`
  - 並行: 可(T16 とファイルが重ならない)
  - 先に書くテスト: `argv[0]` が `-` とシェルのファイル名になる。環境変数が許可リストの名前だけになる(親の余計な変数が入らない)。`TERM` が `xterm-256color`、`COLORTERM` が `truecolor`。`PATH` が `/usr/bin:/bin:/usr/sbin:/sbin`。cwd が指定どおり。`LANG` の組み立て
  - 検証: `npm test && npm run lint`
- [x] T16 — ターミナルエンジンの抽象と xterm.js の実装(DOM 描画)を書く(変更ファイル: package.json, package-lock.json, src/terminal/engine.ts, src/terminal/TerminalManager.ts, src/terminal/TerminalManager.test.ts, src/terminal/xterm/ | 依存: T13)
  - 対応: FR-041、FR-043(スクロールバック 10,000 行、寸法合わせ)、PRD §7 C4。設計書 §5.1 の `TerminalEngine` / `TerminalSurface`。`@xterm/xterm` 6.0.0、`@xterm/addon-fit`、全角幅と絵文字のためのアドオン(`addon-unicode11` か `addon-unicode-graphemes`。どちらにするかは T26 の実機確認の材料にする)を正確な版で入れる。`write` は取り込みが済んだバイト数を返す
  - 並行: 可(T14 とファイルが重ならない)
  - 先に書くテスト: `TerminalManager` を偽のエンジンでテストする(ID とサーフェスの対応、同じ ID で二重に作らない、破棄)。xterm.js の実物の表示は T19 の Playwright で見る
  - 検証: `npm test && npm run lint && npm run build`

#### Phase 10(逐次。T14 完了後)

- [x] T15 — PTY の実装(`portable-pty`)と実 PTY の結合テストを書く(変更ファイル: Cargo.lock, crates/arin-platform/Cargo.toml, crates/arin-platform/src/pty/, crates/arin-platform/src/lib.rs, crates/arin-platform/tests/pty.rs | 依存: T14)
  - 対応: FR-039、FR-040、FR-043(寸法の変更)、NFR-006。`portable-pty` は 0.9.0 に固定。シェルはユーザー情報に登録された既定のシェルを使う(環境変数 `SHELL` だけに頼らない)
  - 並行: 不可(T16 とは並行できる)
  - 先に書くテスト(実 PTY。直列で実行し、待ち時間に上限を付ける): ログインシェルとして起動する(`argv[0]` が `-zsh` の形)。cwd が指定のフォルダ。`PATH` が、同じ最小の環境から `zsh -l` で得た値と同じ(AC-M02 の自動テスト版。`which claude` は T26 で人間が見る)。行・桁数の変更がシェルに届く。2 枚起動して 1 枚を終了させても、もう 1 枚が入出力を続ける(NFR-006)
  - 検証: `npm test && npm run lint`

#### Phase 11(逐次。T15 完了後)

- [x] T17 — セッション管理(読み取りスレッド、出力のまとめ、流量制御)とターミナルのコマンドを書く(変更ファイル: crates/arin-core/src/terminal/, crates/arin-core/src/workspace/, src-tauri/src/sessions.rs, src-tauri/src/commands/terminal.rs, src-tauri/src/lib.rs, src-tauri/tests/ | 依存: T15)
  - 対応: FR-043、NFR-004、NFR-006。設計書 §1.3 #2・#3、§7.1「ターミナルの入出力」。コマンドは 接続(出力チャネルを渡す)・入力の書き込み・処理済みの量の通知・寸法の変更。PTY は最初の接続のときに起動する。流量制御のしきい値は仮の値を置き、T26 の結果で決める
  - 並行: 不可
  - 先に書くテスト(偽の `PtyPort` と偽の送り先): 出力が順序どおりに届く。未処理の量が上限を超えたら読み取りが止まり、処理済みの通知で再開する。接続の前の出力を取りこぼさない。コマンドが `Workspace` を呼び、状態の写しにターミナルが 1 枚載る
  - 検証: `npm test && npm run lint`

#### Phase 12(逐次。T16・T17 完了後)

- [x] T18 — 画面とターミナルをつなぐ(変更ファイル: src/ipc/, src/ipc/generated/, src/terminal/TerminalManager.ts, src/terminal/TerminalManager.test.ts, src/ui/terminals/TerminalHost.tsx, src/ui/shell/AppShell.tsx, src/main.tsx, src/state/, src-tauri/tauri.conf.json, scripts/check-boundaries.mjs | 依存: T16, T17)
  - 対応: FR-039、FR-041、FR-043(リサイズ追従)。出力チャネル → `surface.write` → 処理済みの量を返す。キー入力 → 入力のコマンド。置き場所の大きさ → 行・桁 → 寸法のコマンド
  - CSP: xterm.js は実行時にスタイルの要素を作るので、`style-src 'self'` のままでは動かない見込み(設計書 §2.2)。動く最小の範囲まで緩め、緩めた内容と理由を PROGRESS に書く(ADR-001 への記録は T27)。境界検査の CSP の項目も合わせる
  - 並行: 不可
  - 先に書くテスト(Vitest。偽のエンジンと偽の通信): 出力がサーフェスに届き、取り込みが済んだ量が Rust へ返る。入力がコマンドに届く。寸法の変更がコマンドに届く。ターミナルの破棄で購読が外れる
  - 検証: `npm test && npm run lint && npm run build && npm run app && npm run smoke`。加えて `npm run dev` で起動し、シェルのプロンプトが出て `echo ok` が通ることを 1 回見る(結果を PROGRESS に書く)

#### Phase 13(並行可能。T18 完了後)

- [x] T19 — ターミナルの画面の自動テストを書く(変更ファイル: e2e/fixtures/tauriMock.ts, e2e/terminal.spec.ts | 依存: T18)
  - 対応: FR-043、設計書 §10.3(xterm.js の実物に偽のチャネルから出力を流す)
  - 並行: 可
  - 先に書くテスト(このタスク自体がテスト): 偽のチャネルから流した文字が画面に出る。キー入力が偽のコマンドに届く。ウィンドウの大きさを変えると行・桁数の通知が届く。10,000 行を超えて流したあと 10,000 行までさかのぼれる
  - 検証: `npm run e2e`
- [x] T20 — コピー・ペーストとキー入力の受け口を作る(変更ファイル: src/terminal/engine.ts, src/terminal/xterm/, src/ui/terminals/TerminalHost.tsx, src/logic/keys/, src-tauri/src/menu.rs, src-tauri/src/lib.rs | 依存: T18)
  - 対応: FR-043(コピー・ペースト)、設計書 §2.2「キー入力」「コピーとペースト」。⌘C・⌘V は画面のコピー・ペーストのイベントで受ける案(設計書 §9.1)で作る。メニューバーは「編集」の最小だけ
  - ⌘ のショートカットと画面内のキー処理のどちらが先に受け取るか、Option と Ctrl の扱い、貼り付けの許可を求める吹き出しが出ないかは、実機でしか分からない。T26 で人間が見る
  - 並行: 可(T19・T23・T24 とファイルが重ならない。T21 とは `src/terminal/xterm/` が重なるので逐次)
  - 先に書くテスト: `src/logic/keys` の振り分け(フォーカスがターミナルのとき ⌘C は選択のコピー、⌘V はペースト)を Vitest で。Playwright で「選択 → コピー → ペーストで入力のコマンドに届く」
  - 検証: `npm test && npm run e2e && npm run lint`
- [x] T23 — 検証用の記録(開発ビルドだけ)を足す(変更ファイル: src-tauri/src/sessions.rs, src-tauri/src/lib.rs, crates/arin-core/src/terminal/ | 依存: T18)
  - 対応: 設計書 §2.2 の「ウィンドウが隠れたとき」「クリップボードの制御シーケンス」「流量制御」。記録するもの: (1) PTY の生出力に OSC 52 が現れた回数(本文は記録しない)。(2) 画面が処理済みの量を返した時刻。(3) 未処理の量の最大値
  - リリースビルドには入れない(開発ビルドだけで有効にする)。出力先は標準エラー。ソケットやファイルの常設の口は作らない(N2)
  - 並行: 可
  - 先に書くテスト: OSC 52 の検出(`ESC ] 52 ;` の列を見つける。チャンクの切れ目をまたいでも見つける。OSC 9 や 777 を数えない)を `arin-core` の単体テストで。この走査は M4・M6 のアテンションの走査器の土台にする
  - 検証: `npm test && npm run lint`
- [x] T24 — 性能の計測スクリプトを作る(記録だけ)(変更ファイル: scripts/perf/, crates/arin-platform/tests/perf_pty.rs | 依存: T18)
  - 対応: NFR-004(`cat` で 10 万行)、NFR-009 の簡易確認、設計書 §2.2「流量制御」。出力先は `testreport/perf/`。合否にしない
  - 自動で測れるのは Rust 側(PTY から送り先までの所要時間、未処理の量が上限で頭打ちになること)と、起動中のアプリのメモリ(`footprint`)。画面の反応は人間が見る
  - 並行: 可
  - 先に書くテスト: なし(計測は記録用)。スクリプトが結果のファイルを書くことだけ確かめる
  - 検証: `npm run perf`(終了コード 0。`testreport/perf/` に結果がある。数値で失敗させない)

#### Phase 14(逐次。T20 完了後)

- [x] T21 — WebGL 描画に切り替えられるようにする(検証用)(変更ファイル: package.json, package-lock.json, src/terminal/engine.ts, src/terminal/xterm/ | 依存: T20)
  - 対応: 設計書 §2.2「描画方式」。`@xterm/addon-webgl` 0.19.0。DOM と WebGL を起動時の指定で切り替える(【仮定】開発用の環境変数。設定画面は作らない)。`setVisible(false)` のときに描画の資源を手放す
  - 並行: 不可(`src/terminal/xterm/` を T20 と共有)
  - 先に書くテスト: Playwright で、WebGL の指定でも T19 と同じ表示のテストが通る(Playwright の WebKit で WebGL が使えるかは未検証。使えなければ、切替の分岐だけ Vitest で確かめ、表示は T26 で人間が見る)
  - 検証: `npm test && npm run e2e && npm run lint`

#### Phase 15(逐次。T21 完了後)

- [x] T22 — 9 枚同時表示と非表示インスタンスの検証用の画面を作る(開発ビルドだけ)(変更ファイル: src/ui/terminals/VerificationGrid.tsx, src/ui/shell/AppShell.tsx, src-tauri/src/lib.rs, crates/arin-core/src/workspace/ | 依存: T21)
  - 対応: NFR-009 の簡易確認、設計書 §2.2「9 枚の簡易確認」「非表示のインスタンス」。開発用の指定があるときだけ、ターミナルを 9 枚起動して 3×3 に並べる。1 枚だけ表示に切り替える操作(残り 8 枚は隠す)も付ける。WebGL のとき、描画コンテキストの数の上限に当たらないかを見られるようにする
  - 隠したまま作ったインスタンスの行・桁数は、表示中のターミナルと同じ寸法で始める案で作る(設計書 §2.2)
  - これは M4 のタイル表示ではない。M4(T49)で撤去する
  - 並行: 不可
  - 先に書くテスト: Playwright で、9 枚ぶんの偽のチャネルに出力を流し、隠した 8 枚にも `write` が届き続ける(取り込みが済んだ量が返る)こと
  - 検証: `npm test && npm run e2e && npm run lint && npm run app`

#### Phase 16(逐次。T19〜T24 完了後)

- [x] T25 — Go/No-Go の確認手順書を作る(変更ファイル: output/tasks/M1_GO-NO-GO_CHECKLIST.md | 依存: T19, T20, T22, T23, T24)
  - 対応: 設計書 §2.2 の 12 行(`ARCH_multi-terminal.md:189-202`)を、1 行ずつ「手順・見るところ・結果の欄(DOM / WebGL)」にする。下の「M1 で確かめる項目の割当」の表をもとにする
  - 確認は `npm run app` で作った `.app` を Finder から開いて行う(`npm run dev` では環境変数と WKWebView の条件が変わるため)
  - 並行: 不可
  - 先に書くテスト: なし
  - 検証: 手順書に §2.2 の 12 行がすべて載っている(行数を数える)

#### Phase 17(逐次。人間ゲート)

- [x] T26 — **M1 の Go/No-Go(人間が判定する)**(結果: Go。2026-11-15。ADR-001)(変更ファイル: output/tasks/M1_GO-NO-GO_CHECKLIST.md | 依存: T25)
  - **人間がやること**: T25 の手順書に沿って、Claude Code と Codex を実際に動かし、結果を書き込む。Go / No-Go を決める。描画方式(DOM / WebGL)、全角幅のアドオン、OSC 52 を許すか(設計書 §15.2 #9)、流量制御のしきい値を決める
  - AI がやること: 手順の補助と、記録(T23)と計測(T24)の結果の整理だけ。合否は判定しない
  - No-Go のとき: 原因に応じて、描画方式の変更か実装技術の見直し(Swift ネイティブへ戻すことを含む)を本人が判断する(PRD §7)。T28 以降は止める
  - 日本語入力の不具合 #5778(英数キーでの確定で文字が二重になる)が再現したとき: 回避策の調査を別タスクとして T27 で起こす(いまは中身を決められない)

#### Phase 18(逐次。T26 完了後)

- [x] T27 — ADR-001 を書き、M1 を締め、M2 以降を計画し直す(描画方式と全角幅の切り替えは外さず M4 まで残した。M2 の再分解は T28)(変更ファイル: output/design/ADR_001_terminal-engine.md, output/design/ADR_INDEX.md, output/tasks/PLAN_multi-terminal.md, output/tasks/PROGRESS.md, docs/project.md, docs/architecture.md, project-config.md | 依存: T26)
  - 対応: 設計書 §14(ADR-001: エンジンと描画方式、アドオン、CSP、Go/No-Go の結果)。FR-039〜041、043 の AC の合否一覧を `testreport/` に出す。Go/No-Go の結論は ADR-001 にも書く(`testreport/` は Git の管理外のため)
  - T26 の決定を実装に反映する小さな変更(使わない描画方式のアドオンを外す、しきい値の確定、OSC 52)は、このタスクで必要なぶんを新しいタスクとして起こす
  - M2 以降(T28〜)を、結果に合わせて `/plan` で分解し直す
  - 検証: `npm run build && npm test && npm run lint && npm run e2e && npm run smoke`

**M1 で確かめる項目の割当**(設計書 §2.2。漏れの確認用)

| §2.2 の項目 | 用意するタスク | 判定 |
| ---- | ---- | ---- |
| TUI の表示(truecolor、代替スクリーン、マウス、ブラケットペースト、全角幅、絵文字) | T16、T18 | T26(人間) |
| 日本語入力(変換中の表示、候補ウィンドウの位置、Enter での確定、英数キーでの確定=#5778、変換中の Esc と矢印) | T18 | T26(人間) |
| 描画方式(DOM / WebGL) | T16、T21、T24 | T26(人間) |
| 9 枚の簡易確認(WebGL の描画コンテキストの上限を含む) | T22、T24 | T26(人間) |
| 非表示のインスタンス(描画せず取り込みだけ続ける。隠したまま作ったときの寸法) | T22 | T22 の自動テストと T26(人間) |
| ウィンドウが隠れたとき(`backgroundThrottling`) | T5(設定)、T23(記録) | T26(人間。最小化して 5 分以上置き、処理済みの通知が続いていたかを記録で見る) |
| キー入力(⌘ の受け口、Option、Ctrl) | T20 | T26(人間) |
| コピーとペースト(許可の吹き出しが出ない) | T20 | T26(人間) |
| 流量制御(メモリが増え続けない。しきい値) | T17、T23、T24 | T24 の記録と T26(人間) |
| CSP(どこまで緩めるか) | T18 | T27 で ADR-001 に記録 |
| スクロールバック 10,000 行 | T16、T19 | T19 の自動テスト |
| クリップボードの制御シーケンス(OSC 52 の要否) | T23 | T26(人間。Claude Code と Codex のコピー機能を使い、記録を見る) |

### M2 マルチプロジェクト(T28 で 1 セッション粒度に分解し直した。2026-11-15)

完了条件: FR-001〜004、007〜010、042、044、056、059〜061(タブ表示の部分)、063、064 の AC 合格(対応表は節の末尾)。

#### M2 の前提

司令塔が決めたこと(T28。【仮定】として扱い、異論があれば T36 までに直す):

- (a) 【仮定】React のフック向けの lint は、M2 の最初のタスク(T28a)で `eslint` + `eslint-plugin-react-hooks` を最小の設定で入れるかを調べて決める。入れない結論もありうる。結論は ADR に残す
- (b) 【仮定】アプリのショートカット(M2 では ⌘T・⌘W・⌘⇧[・⌘⇧])は、メニューバーの項目のアクセラレータで受ける。M1 の ⌘C・⌘V と同じ経路で、WKWebView より先に OS が受けるため。項目が選ばれたら Rust が `Workspace` を直接操作し、状態の写しで画面に返す。実機での確認は T36(人間)。⌘1〜9(FR-006)は M6(T58)のままで、同じ経路で足す
- (c) 【仮定】ブックマーク: Rust から作って解決できるかは別のエージェントが検証している。**T28 の検証結果(PROGRESS に記載)に従う。** 使うクレートは本計画では空欄にする(T31b で埋める)
- (d) 【仮定】`state.json` が読めないとき・版が新しすぎるとき: 壊れたファイルを消さず・上書きせず、`state.json.broken-<日時>` に改名して空の状態で起動し、画面に 1 行で知らせる(§リスク #4 の A。FR-058 の AC の確認は M6 の T56 に残す)
- 描画方式は DOM が既定のまま。WebGL との比較は M4。M2 では描画方式に手を入れない。起動時の指定(`ARIN_RENDERER`・`ARIN_UNICODE`・`ARIN_VERIFY_GRID`)と `RendererLabel` は M4(T49)まで残す(ADR-001)
- 9 枚の検証画面(`ARIN_VERIFY_GRID`)は M4 まで残し、M2 の複数化で壊さない。両立のさせ方: 検証画面のときは `Workspace` がプロジェクトを持たないターミナルを 9 枚持ち(T32b。いまの `with_terminal_count` を置き換える)、`state.json` を読まず書かず(T33a)、タブを自動で閉じない(T32a)。画面は M1 と同じく検証画面だけを出す(T34c)。`e2e/verify-grid.spec.ts` と `node scripts/smoke.mjs --env=ARIN_VERIFY_GRID=1` を、M2 の画面と Rust のタスクの検証に含める

本計画が置いた【仮定】(M2 の分):

1. プロジェクト ID は UUID v4 の形の文字列。乱数は `arin-platform` の `IdSource`(`/dev/urandom` を読む)、16 バイトから文字列への整形は `arin-core`。`uuid` クレートは足さない(§リスク #1 で「M2 の再分解のときに決める」とした分。`arin-core` の依存を増やさない)
2. ブックマークを `state.json` に書くときの Base64 の変換をどこに置くかも、T28 の (c) の結果に従う(`arin-platform` に置いて `arin-core` には文字列で渡すなら、`arin-core` は文字列のまま扱う)。結果で決まっていなければ `arin-core/persistence` に小さな関数で書く(`arin-core` にクレートを足さない)
3. 退避の名前の日時は UTC で `state.json.broken-20261115T093000Z` の形。同じ名前があれば `-2`、`-3` と足す。改名に失敗したら、その回の起動では `state.json` を書かない(壊れたファイルを上書きで失わないため)。知らせは閉じるまで出し、保存しない
4. 「動作中のプロセス」(FR-003、FR-063)は設計書 §15.2 #2 の A(シェル以外が前面で動いている)。M2 では削除と終了のときにだけ調べる。1 秒の見回りで実行中の印を出すのは M6(T57。FR-012)
5. シェルが終わったタブは自動で閉じる(設計書 §15.2 #3 の A。0 枚も許す)。検証画面だけは M1 と同じく閉じずに「終了しました」を出す
6. フォルダを登録したら、そのプロジェクトを選び、タブを 1 枚開く(起動時に全プロジェクトで 1 タブ=C-22 と揃える)。再リンクではタブを開かない(0 枚から ⌘T)
7. Finder からのドロップは、ウィンドウのどこに落としてもフォルダだけを登録する(ファイルは無視)。ツリーへのドロップの意味づけは M6(FR-031)
8. フォルダ選択ダイアログは Rust 側から開く(選んだパスが画面を経由しない。設計書 §9.1)。画面にダイアログのプラグインの権限を与えない
9. タブの表示名は「ターミナル N」(プロジェクト内の通し番号)。プロセス名に追従するタブ名は M6(FR-046)
10. ペインの最小寸法: カード領域 72px、ツリー領域 120px、左ペインの幅 180px、右ペインの幅 320px。初期値: 左ペインの幅 260px、カード領域の高さ 200px。値は T34c の画面で本人が見て直してよい
11. 寸法の上限(M1 の見送り): 行 1,000、桁 2,000。超えたら `InvalidSize`
12. プロジェクト色の識別子は 8 つで、この順がパレットの順: `red`・`orange`・`yellow`・`green`・`teal`・`blue`・`purple`・`pink`。色の値(ライト・ダーク)は `src/ui/design/palette.ts` が持つ
13. `state.json` の `bookmark` が無い(null)プロジェクトは、`lastKnownPath` から解決してブックマークを作り直す。起動スモークと性能の計測が、一時フォルダのプロジェクトを `state.json` に書いて渡すため
14. ⌘W は、タブに動作中のプロセスがあっても確認しない(PRD に定めが無い。要件を足さない)
15. 左ペインの折りたたみはボタンで行う(ショートカットは付けない)
16. ツリーの中身は M3。M2 の左ペイン下段は、プロジェクト色のヘッダ(プロジェクト名)、空の領域、「見つからない」のときの案内(再リンク / 一覧から削除)だけを出す。FR-060 の「ツリーのヘッダ」の着色は M2 で満たす
17. プロジェクトを削除したら、選択は次のカード(無ければ前、0 件なら無し)に移る。タブを閉じたら右隣(無ければ左)が表示中になる。前後のタブの切替は端で反対側に回り込む

#### 現状のコードから分かった、複数化で変えるところ

- `Workspace` はターミナルを `terminal-1` の 1 枚(検証画面では `terminal-1`〜`terminal-9`)だけ持ち、cwd はホーム(`crates/arin-core/src/workspace/state.rs` の `Workspace::new`・`with_terminal_count`)。プロジェクトもタブの並びも無い → T32a・T32b
- `AppSnapshot` は `schemaVersion`・`appVersion`・`terminals`(`id`・`status`)だけ(`crates/arin-core/src/workspace/snapshot.rs`)→ T32b
- `Workspace` は `SessionRegistry` の中の Mutex が持つ(`src-tauri/src/sessions.rs` の `Inner::workspace`)。設計書 §11 は `Workspace` を Tauri の管理状態に別に登録する形で書いている → 実装の形のまま進め、T28b で設計書を合わせる
- セッションを終わらせるのはアプリの終了(`SessionRegistry::shutdown`)だけで、タブ 1 枚を閉じる口が無い。シェルが終わってもセッションとスレッドが残る(`sessions.rs` の `close`)→ T33b
- `AppShell` は写しの最初のターミナル(`snapshot.terminals[0]`)を 1 枚だけ描く(`src/ui/shell/AppShell.tsx`)。`TerminalManager.dispose` はあるが呼ぶところが無い → T34b・T34d
- メニューバーは「Arin」(隠す・終了)と「編集」(コピー・ペースト・すべてを選択)の標準の項目だけ(`src-tauri/src/menu.rs`)→ T33e
- キーの振り分けでアプリが受ける ⌘ のキーは `c`・`v` だけ(`src/logic/keys/clipboard.ts` の `APP_COMMAND_KEYS`)→ T34a
- `arin-platform` の依存は `portable-pty` だけで、状態ファイル・ブックマーク・ペーストボードの実装が無い(`crates/arin-platform/Cargo.toml`)→ T31a〜T31c
- `arin-core` の依存は serde・serde_json・thiserror・ts-rs だけで、境界検査が強制する(`crates/arin-core/Cargo.toml`、`scripts/check-boundaries.mjs` の `checkCoreDependencies`)→ 【仮定】1・2 で依存を増やさない
- 起動スモークは、起動直後のターミナルの出力の印(`smoke-terminal`)を待つ(`scripts/smoke.mjs`)。M2 では登録が 0 件だとターミナルが無く、印が書かれない → T33a で、一時フォルダのプロジェクトを入れた `state.json` を渡す。T32b の後から T33a が終わるまでは、着手前のスモークを `--env=ARIN_VERIFY_GRID=1` で行う
- 画面の自動テストの偽物は、写しを `{ schemaVersion, appVersion, terminals }` で作る(`e2e/fixtures/tauriMock.ts` の `sampleSnapshot`)→ T32b で新しい形に合わせる
- `PtySession` に前面プロセスを調べる操作が無い(`crates/arin-core/src/ports/pty.rs`)。FR-003・FR-063 の確認に要る → T31d

#### 親タスクと子タスク

T29〜T35 は親として残し、作業は子タスクで行う。親は子がすべて済んだら済みにする。T36 は分けない。

| 親 | 子 |
| -- | -- |
| T28 — M2 の着手時の確認と再分解 | T28a(フック向けの lint)、T28b(設計書を M2 の前提に合わせる) |
| T29 — プロジェクトの規則 | T29a(ポートと ID 型、モジュールの骨格)、T29b(`ProjectStore`) |
| T30 — 状態の保存と復元、表示の状態 | T30a(`state.json` の読み書き)、T30b(ペインの寸法と保存のまとめ) |
| T31 — OS 側の実装 | T31a(状態ファイル)、T31b(ブックマークとルートの現在位置)、T31c(ペーストボード、乱数、時計)、T31d(前面プロセスの調査) |
| T32 — ストアをまたぐ操作とターミナルの複数化 | T32a(`TerminalStore`)、T32b(`Workspace` の複数化と状態の写し) |
| T33 — コマンド、イベント、メニューバー、終了の確認、見回り | T33a(起動時の読み込みと保存)、T33b(ターミナルの複数化とセッションの後片付け)、T33c(プロジェクトのコマンド)、T33d(ルートへの追従とペインの寸法)、T33e(メニューバーと終了の確認) |
| T34 — 画面 | T34a(画面の純粋ロジック)、T34b(ストアと操作、ターミナルの管理)、T34c(画面の枠とカード一覧)、T34d(ターミナル領域とタブ) |
| T35 — 結合テストと画面の自動テストの仕上げ | T35a(M2 のシナリオの画面の自動テスト)、T35b(実物の結合テストと計測の準備) |

#### 共有のファイルの持ち回り

同じ Phase で 2 つのタスクが同じファイルを触らないように割り当てた。下のファイルは表の順に逐次で触る。

| ファイル | 触るタスク(この順) |
| -------- | ------------------ |
| `package.json`、`package-lock.json` | T28a だけ |
| `crates/arin-core/src/lib.rs`、`crates/arin-core/src/ports/mod.rs`、`crates/arin-core/src/persistence/mod.rs`、`crates/arin-core/src/layout/mod.rs` | T29a だけ(Phase 20b の並行のため、子モジュールの宣言を先に済ませる) |
| `crates/arin-core/tests/generated_types.rs`、`src/ipc/generated/`、`AppSnapshot` の型 | T29a → T32b → T33b → T33c → T33e |
| `Cargo.toml`、`Cargo.lock`、`crates/arin-platform/Cargo.toml`、`crates/arin-platform/src/lib.rs` | T31b → T31c → T33c |
| `src-tauri/src/lib.rs`、`src-tauri/Cargo.toml` | T32b → T33a → T33b → T33c → T33d → T33e |
| `src-tauri/src/sessions.rs` | T32b(呼び出しを合わせるだけ)→ T33b → T33e |
| `src/ipc/client.ts`、`src/ipc/client.test.ts` | T33b → T33c → T33d → T33e |
| `src/test/fakeIpc.ts` | T32b → T34b |
| `e2e/fixtures/tauriMock.ts` | T32b → T34c → T34d → T35a |
| `scripts/smoke.mjs` | T33a だけ |
| `scripts/check-boundaries.mjs` | M2 では触らない見込み(足す検査が出たら T33c の後に逐次で足す) |
| `output/design/ADR_INDEX.md` | T28a(ADR-008)→ T31b(ADR-009) |
| `output/design/ADR_009_m2-platform-deps.md` | T31b → T31c → T33c |
| `output/design/ARCH_multi-terminal.md` | T28b → T36 |

#### Phase 19(逐次。M1 が Go のとき)

- [x] T28 — M2 の着手時の確認と再分解(変更ファイル: output/tasks/PLAN_multi-terminal.md, output/tasks/PROGRESS.md, output/design/ | 依存: T27)
  - (a) React のフック向けの lint を入れるか再検討する(設計書 §15.1 #10。`ARCH_multi-terminal.md:968`。Biome などを足すなら ADR に残し `npm run lint` に組み込む)
  - (b) ショートカットの受け口(メニューバーか画面内か)を実機で確かめる(設計書 §15.3 #7)。FR-044 と FR-006 の作り方が決まる
  - (c) Rust から macOS のブックマークを作って解決できることを小さなプログラムで確かめる(設計書 §5.4。Rust からは未実行)
  - (d) `state.json` が読めないときの M2 での振る舞いを決める(§リスク #4)
  - (e) T29〜T36 を 1 セッション粒度に分解し直す
  - 検証: (a) は `npm run lint`。(b)(c) は結果を PROGRESS に書く

#### Phase 20a(並行可能。T28 完了後)

- [x] T28a — React のフック向けの lint を入れるかを調べて決める(変更ファイル: package.json, package-lock.json, eslint.config.js, src/ui/(違反があれば直す), output/design/ADR_008_frontend-lint.md, output/design/ADR_INDEX.md | 依存: T28)
  - 対応: 設計書 §15.1 #10(`ARCH_multi-terminal.md:979`)、PRD §7 C6、決定 (a)
  - 調べること: `eslint` と `eslint-plugin-react-hooks` だけでは `.ts`・`.tsx` を読めず、構文解析器(`typescript-eslint` か `@typescript-eslint/parser`)が要る見込み(未検証)。入れる場合の直接の依存の数、推移的な依存の数とそのうち公開から 2 週間未満のもの、`npm ci --ignore-scripts` で動くか、`npm run lint` の所要時間の増え方。比べる案として Biome(1 つのバイナリにフックの規則がある)も ADR に並べる
  - 入れるとき: 規則は `react-hooks` の `rules-of-hooks` と `exhaustive-deps` だけ。対象は `src/**/*.{ts,tsx}`(`src/ipc/generated/` は除く)。`npm run lint` の `tsc -b` の後ろに足す。版は正確に固定する(npm は `^` なし)。M1 のコードに違反があれば直す(`src/ui/` だけ)
  - 入れないとき: 理由と、代わりの運用(フックを書くのは `src/ui` の部品だけにし、判定は `src/logic` に寄せる。設計書 §9.1)を ADR に書く
  - 並行: 可(`package.json` を M2 で触るのはこのタスクだけ。`src/ui/` は T34c まで誰も触らない)
  - 先に書くテスト: なし(道具の設定)。入れたときは、条件分岐の中で `useEffect` を呼ぶ一時ファイルで `npm run lint` が失敗することを 1 回確かめて消す(結果を PROGRESS に書く)
  - 検証: `npm run lint`(終了コード 0)。`ls output/design/ADR_008_*.md` が 1 件
  - 人間の確認: 依存を足す場合は、ADR-008 を本人が読んで了承する
- [x] T28b — 設計書を M2 の前提に合わせる(変更ファイル: output/design/ARCH_multi-terminal.md | 依存: T28)
  - 直す内容: (1) §6.2 の表と「破損時」「未知の版」、§7.2 の `state.json` の行: 退避の名前を `state.corrupt-<日時>.json` から `state.json.broken-<日時>` に、画面に 1 行で知らせる、改名に失敗したらその回は書かない(決定 (d)、【仮定】3)。(2) §6.2 の `projects[].bookmark` を「Base64 文字列または null」に(【仮定】13)。(3) §15.3 #7 にメニューバーで受けると決めた旨(決定 (b))。(4) §11 の Rust 側の表と §5.1: `Workspace` は `SessionRegistry` の中の Mutex が持つ(実装に合わせる)。(5) §1.2・§1.3 #15: 前面プロセスの調査は M2 では削除と終了のときだけ、1 秒の見回りは M6(【仮定】4)。(6) §5.2 の通信の一覧: フォルダ選択ダイアログと Finder からのドロップは Rust 側で受け、画面からのコマンドは「ダイアログを開く」だけ(【仮定】7・8)。(7) §3.1 と §6.2: プロジェクト ID の作り方(【仮定】1)。(8) §1.4 #12 と §5.4: T28 の (c) で分かったこと(Rust から作って解決できた、改名・移動で古くなった印が付く、削除後に同じパスへ作ったフォルダに解決される既知の制約)を PROGRESS から写す
  - 直さないもの: §15.1 #10(lint の結論)と §2(M2 で足した依存)は T28a・T31b・T31c・T33c の結果を見て T36 で直す(同じファイルを並行で触らないため)
  - 並行: 可(コードを触らない)
  - 先に書くテスト: なし(文書だけ)
  - 検証: `grep -n 'state.corrupt' output/design/ARCH_multi-terminal.md` が 0 件
  - 人間の確認: 承認済みの設計書を直すので、差分を本人が読む(T36 までに)
- [x] T29a — `arin-core` に M2 のポートと ID 型、モジュールの骨格を置く(変更ファイル: crates/arin-core/src/lib.rs, crates/arin-core/src/ports/mod.rs, crates/arin-core/src/ports/root.rs, crates/arin-core/src/ports/state_file.rs, crates/arin-core/src/ports/pasteboard.rs, crates/arin-core/src/ports/system.rs, crates/arin-core/src/project/mod.rs, crates/arin-core/src/project/id.rs, crates/arin-core/src/persistence/mod.rs, crates/arin-core/src/persistence/codec.rs, crates/arin-core/src/persistence/scheduler.rs, crates/arin-core/src/layout/mod.rs, crates/arin-core/tests/project_id.rs, crates/arin-core/tests/generated_types.rs, src/ipc/generated/ | 依存: T28)
  - 対応: 設計書 §5.1 の `arin-core/ports`(`BookmarkPort`・`RootLocatorPort`・`StateFilePort`・`PasteboardPort`・`Clock`)、§3.2(機能モジュールどうしは ID 型と `ports` だけを共有する)、§リスク #1(M2 の分)
  - 置くもの: `ProjectId`(16 バイトから作る UUID v4 の形の文字列と、その検査。【仮定】1)、`ProjectColor`(8 つの識別子。【仮定】12。ts-rs で生成する)。ポートの trait: ルート(パスを調べて正規化したパス・フォルダか・ブックマークを返す、ブックマークを解決する、開いたフォルダの現在位置を返す)、状態ファイル(読む・原子的に書く・退避の名前へ改名する)、ペーストボード(文字列を書く)、`IdSource`(16 バイトの乱数)、`Clock`(いまの時刻)。ブックマークの中身はクレートの型を出さない形にする(バイト列か Base64 の文字列かは【仮定】2 のとおり T28 の (c) の結果に合わせる)。解決の結果の形(新しいパスと「古くなった」の印、解決できないときの理由)も T28 の (c) の結果に合わせる
  - `persistence` と `layout` は、子モジュール(`codec`・`scheduler`)の宣言と空のファイルだけ置く。後のタスクは `lib.rs` と各 `mod.rs` を触らずに済む
  - 並行: 可(T28a・T28b・T34a とファイルが重ならない)。Phase 20b の全タスクの前提
  - 先に書くテスト: `プロジェクト_id_は_16_バイトから_uuid_v4_の形になる`、`uuid_の形でない文字列はプロジェクト_id_にできない`、`プロジェクト_id_の_json_は文字列`、`色の識別子は_8_つでパレットの順に並ぶ`、`色の識別子の_json_は小文字の名前`。`generated_types.rs` に `ProjectId`・`ProjectColor` を足す(生成し直して差分が出ない)
  - 検証: `npm test && npm run lint`
- [x] T34a — 画面の純粋ロジック(キー、メニュー、ペインの寸法)(変更ファイル: src/logic/keys/, src/logic/menu/, src/logic/panes/ | 依存: T28)
  - 対応: FR-044(タブ操作の ⌘ のキーを xterm.js に渡さない)、FR-009(ペインの寸法)、FR-003・FR-007・FR-059・FR-064(カードの右クリックの項目)、設計書 §5.1 の `src/logic/keys`・`src/logic/menu`、M1 の見送り(「すべてを選択」)
  - 置くもの: `terminalKeyOwner` で ⌘T・⌘W・⌘⇧[・⌘⇧]・⌘A を `app` にする(メニューが受けるので xterm.js に処理させない)。メニューから届く操作(「すべてを選択」)をフォーカス位置で振り分ける関数。`buildCardMenu`(絶対パスをコピー、色 ▸ 8 色、一覧から削除。見つからないときは再リンクと一覧から削除)。ペインの寸法を、ウィンドウの大きさと最小寸法(【仮定】10)から決める関数と、境界のドラッグ中の位置の計算
  - `src/logic` は他の `src/*` を import できないので、写しの型は使わず、必要な値だけの型をここで定める
  - 並行: 可(`src/logic` だけを触る)
  - 先に書くテスト(Vitest): `ターミナルにフォーカスがあっても_⌘T_⌘W_⌘⇧[_⌘⇧]_はアプリが受ける`、`⌘A_はアプリが受ける`、`Ctrl_と_Option_の組み合わせは今までどおりターミナルが受ける`、`すべてを選択はターミナルにフォーカスがあればターミナルの全選択`、`それ以外では画面の既定に任せる`、`カードのメニューに絶対パスをコピーと色と一覧から削除がある`、`見つからないカードのメニューは再リンクと一覧から削除`、`ウィンドウが狭くても各ペインが最小寸法を下回らない`、`カード領域を広げるとツリー領域が縮み最小で止まる`、`折りたたむと右ペインが全幅になり_戻すと前の幅`
  - 検証: `npm test && npm run lint && npm run test:coverage`(`src/logic` の行 80%)

#### Phase 20b(並行可能。T29a 完了後)

- [x] T29b — プロジェクトの規則(`ProjectStore`)(変更ファイル: crates/arin-core/src/project/mod.rs, crates/arin-core/src/project/store.rs, crates/arin-core/src/project/color.rs, crates/arin-core/tests/project_store.rs | 依存: T29a)
  - 対応: FR-001(登録)、FR-003(削除。ストアは入出力をしないのでフォルダに触れない)、FR-007(「見つからない」と再リンク。ゴミ箱の中は「見つからない」=設計書 §15.2 #7)、FR-010、FR-059、設計書 §5.1、§7.2「プロジェクトの登録」、§9.2(色の割当)、§15.2 #5(表示名はフォルダ名に追従)
  - 並行: 可(`project/` だけを触る)
  - 先に書くテスト: `登録すると末尾に_1_件増えて選択される`、`正規化したパスが同じなら増えず既存が選択される`(AC-P10)、`入れ子のフォルダは別のプロジェクトとして登録できる`(AC-P10)、`削除すると一覧から消え選択は次に移る`(【仮定】17)、`パレットが_8_色で_3_件登録すると色がすべて違う`(AC-C01)、`色は使われている数が最も少ないもの_同数ならパレットの順`、`削除で空いた色が次の登録で使われる`、`色をあとから変えられる`(FR-059)、`表示名はいまのパスのフォルダ名`、`パスが変わると表示名と表示用のパスが変わり_id_と色は変わらない`(AC-P07)、`表示用のパスはホームを_~_にする`、`ゴミ箱の中のパスは見つからない扱い`、`見つからないプロジェクトを再リンクすると_id_と色を保つ`、`再リンク先が既に登録済みのフォルダなら再リンクしない`
  - 検証: `npm test && npm run test:coverage`(`arin-core` の行 80%)
- [x] T30a — `state.json` 版 1 の読み書き(`arin-core/persistence/codec`)(変更ファイル: crates/arin-core/src/persistence/codec.rs, crates/arin-core/tests/persistence_codec.rs | 依存: T29a)
  - 対応: FR-004、FR-009(保存値)、FR-056、設計書 §6.2 のスキーマと永続化、決定 (d)、【仮定】2・3・13
  - 読んだ結果は「状態」「無い(空で始める)」「退避する(理由つき)」のどれか。退避の名前は時計の値から作る。ペインの寸法は数として読むだけで、範囲の検査は T30b の規則を T32b が当てる
  - 並行: 可(`persistence/codec.rs` だけ。`scheduler.rs` は T30b)
  - 先に書くテスト: `書いて読むと同じ状態に戻る`、`欠けたキーは既定値で補う`、`未知のキーは無視する`、`壊れた_json_は退避の指示になる`、`版が新しすぎると退避の指示になる`、`版が数でない_0_負のときも退避の指示になる`、`ブックマークの文字列の往復`、`ブックマークが壊れていたらそのプロジェクトだけ作り直しの指示にする`、(Base64 の変換を `arin-core` に置く場合は `base64_の往復` も)`bookmark_が_null_なら_lastKnownPath_から作り直す指示`、`退避の名前は_utc_の日時`(`state.json.broken-20261115T093000Z`)、`退避の名前が既にあれば_-2_を足す`、`選択中の_id_が一覧に無ければ_null_にする`、`同じ_id_が_2_回出たら後のものを捨てる`
  - 検証: `npm test && npm run test:coverage`
- [x] T30b — ペインの寸法と保存のまとめ(`arin-core/layout`、`persistence/scheduler`)(変更ファイル: crates/arin-core/src/layout/mod.rs, crates/arin-core/src/layout/panes.rs, crates/arin-core/src/persistence/scheduler.rs, crates/arin-core/tests/layout_panes.rs, crates/arin-core/tests/persistence_scheduler.rs | 依存: T29a)
  - 対応: FR-009(最小寸法、左右の幅、折りたたみ)、FR-056(保存の契機)、設計書 §6.2「書き込みの契機」、§10.2(時計を注入する)
  - `layout/mod.rs` は T29a が置いた宣言に `panes` を足す(Phase 20b で `layout/` を触るのはこのタスクだけ)
  - 並行: 可
  - 先に書くテスト: `最小を下回る寸法は最小に留める`(【仮定】10)、`数でない値_無限大_負の値は既定値`、`折りたたんでも幅を覚えていて戻すと同じ幅`、`変更から_0.5_秒たつまで保存しない`(偽の時計)、`0.5_秒のあいだに続いた変更は_1_回の保存になる`、`ウィンドウが前面でなくなったときと終了時は待たずに保存する`、`変更が無ければ保存しない`、`保存に失敗したら次の変更でもう一度予約する`、`書かない設定(退避に失敗した回、検証画面)では一度も保存しない`
  - 検証: `npm test && npm run test:coverage`
- [x] T32a — タブの規則(`arin-core/terminal` の `TerminalStore`)(変更ファイル: crates/arin-core/src/terminal/mod.rs, crates/arin-core/src/terminal/store.rs, crates/arin-core/src/terminal/state.rs, crates/arin-core/tests/terminal_store.rs | 依存: T29a)
  - 対応: FR-039(プロジェクトごとの複数タブ、0 枚を許す)、FR-044(追加・閉じる・前後)、設計書 §6.1 の `TerminalStore`、§15.2 #3、【仮定】5・9・11・17、M1 の見送り(寸法の上限)
  - 並行: 可(`terminal/` だけを触る。`ports/pty.rs` は T31d)
  - 先に書くテスト: `プロジェクトごとにタブを持ち_追加すると末尾に増えて表示中になる`、`閉じると右隣_無ければ左が表示中になる`、`最後の_1_枚を閉じると_0_枚になり自動で開かない`(C-21)、`0_枚から追加できる`、`前後のタブの切替は端で反対側に回り込む`、`タブが_1_枚なら前後の切替は何もしない`、`ターミナルの_id_は起動中に使い回さない`、`タブの表示名はプロジェクト内の通し番号`、`プロジェクトのタブをまとめて外せる`、`シェルが終わったタブは閉じる`、`検証画面の設定ではシェルが終わってもタブを閉じない`、`寸法は行_1000_桁_2000_まで_超えたら_invalid_size`(M1 の見送り)
  - 検証: `npm test && npm run test:coverage`
- [x] T31a — 状態ファイルの読み書きと退避(`arin-platform/storage`)(変更ファイル: crates/arin-platform/src/storage/mod.rs, crates/arin-platform/src/storage/state_file.rs, crates/arin-platform/tests/state_file.rs | 依存: T29a)
  - 対応: FR-056、FR-058 の土台、設計書 §6.2「書き込み方法」、決定 (d)。標準ライブラリだけで書く(依存を足さない)
  - 並行: 可(`storage/` だけ。`arin-platform/src/lib.rs` は既に `storage` を宣言している)
  - 先に書くテスト(一時ディレクトリだけ): `無ければ無いと返す`、`書くと同じフォルダの一時ファイルから置き換える`、`状態フォルダは_0700_ファイルは_0600`、`一時ファイルの段階で失敗しても元のファイルが残る`、`退避は改名だけで中身を変えない`、`退避先と同じ名前があれば上書きせず失敗する`、`渡したフォルダの外に書かない`
  - 検証: `npm test && npm run lint`
- [x] T31b — ブックマークとルートの現在位置(`arin-platform/bookmark`、`watch`)(変更ファイル: Cargo.toml, Cargo.lock, crates/arin-platform/Cargo.toml, crates/arin-platform/src/lib.rs, crates/arin-platform/src/bookmark/, crates/arin-platform/src/watch/, crates/arin-platform/tests/root.rs, output/design/ADR_009_m2-platform-deps.md, output/design/ADR_INDEX.md | 依存: T29a)
  - 対応: FR-007、FR-010(正規化したパス)、設計書 §1.3 #8、§5.4、§7.1「ルートフォルダへの追従」
  - 使うクレート: ブックマークは **T28 の (c) の検証結果(PROGRESS に記載)に従う**(クレート: ＿＿＿＿＿＿)。開いたフォルダの現在位置(`fcntl` の `F_GETPATH`)は 【仮定】`libc`(Tauri が既に引き込んでいる版に合わせて正確に固定する)。公開から 2 週間未満の版を避け、足した理由と版を ADR-009 に書く
  - T28 の (c) で Rust から作れないと分かったとき: このタスクは始めず、本人に方式(パスだけで記憶して見回りで追う、など。FR-007 の AC が変わりうる)を確かめる
  - 並行: 可(Phase 20b で Cargo の 3 ファイルと `arin-platform/src/lib.rs` を触るのはこのタスクだけ)
  - 先に書くテスト(一時ディレクトリだけ。直列で、どの待ちにも上限時間): `登録するフォルダは正規化したパスになる`(シンボリックリンクを経由しても同じ)、`フォルダでないものは登録できない`、`ブックマークは改名後に新しいパスへ解決される`、`ブックマークは移動後に新しいパスへ解決される`、`削除したフォルダのブックマークは解決できない`、`開いたフォルダは改名と移動のあとも現在のパスが取れる`、`削除後は現在のパスが取れない`
  - 検証: `npm test && npm run lint`。`cargo tree -i reqwest --target aarch64-apple-darwin` で通信のクレートが増えていない
- [x] T31d — 前面プロセスの調査(動作中かの判定の材料)(変更ファイル: crates/arin-core/src/ports/pty.rs, crates/arin-platform/src/pty/native.rs, crates/arin-platform/tests/pty.rs | 依存: T29a)
  - 対応: FR-003・FR-063 の「動作中のプロセス」(設計書 §15.2 #2 の A、【仮定】4)、設計書 §1.3 #15、M1 の見送り(前面プロセスグループの取得がポートに無い)
  - `PtySession` に「前面のプロセスグループがシェル自身か(分からなければ不明)」を返す操作を足す。既定の実装(不明を返す)を持たせ、既存の偽の PTY(`src-tauri/tests/` など)を書き換えずに済ませる。`portable-pty` の `process_group_leader()` とシェルの pid を比べる(設計書 §1.4 #11 で動作は確認済み)
  - 並行: 可(`ports/pty.rs` と `pty/` だけ)
  - 先に書くテスト(実 PTY。`crates/arin-platform/tests/pty.rs` の決まりに従う): `起動直後の前面はシェル`、`sleep_を前面で動かすとシェル以外になる`、`sleep_が終わるとシェルに戻る`、`シェルが終わったあとは不明を返す`
  - 検証: `npm test && npm run lint`

#### Phase 20c(並行可能)

- [x] T31c — ペーストボード、乱数、時計(`arin-platform/desktop`、`system`)(変更ファイル: Cargo.toml, Cargo.lock, crates/arin-platform/Cargo.toml, crates/arin-platform/src/lib.rs, crates/arin-platform/src/desktop/, crates/arin-platform/src/system/, crates/arin-platform/tests/desktop.rs, crates/arin-platform/tests/system.rs, output/design/ADR_009_m2-platform-deps.md | 依存: T31b)
  - 対応: FR-064(ルートの絶対パスをペーストボードへ)、【仮定】1(乱数)・3(時計)
  - 使うクレート: ペーストボードは 【仮定】`objc2-app-kit` の `NSPasteboard`(設計書 §1.4 #19。Tauri が既に使っている。T28 の (c) で使うクレートと同じ系統なら揃える)。乱数は `/dev/urandom` を読む、時計は `SystemTime`(どちらも依存なし)。ADR-009 に追記する
  - 並行: 可(T32b とファイルが重ならない。T31b の後なのは Cargo の 3 ファイルと `lib.rs` を共有するため)
  - 先に書くテスト: `テスト用の名前付きペーストボードに書いた文字列を読み戻せる`(ユーザーのクリップボードに触れない。書き込み先をポートの実装に渡せる形にする)、`乱数は_16_バイトで_2_回続けて同じにならない`、`時計は_unix_の秒とミリ秒を返す`
  - 検証: `npm test && npm run lint`。`cargo tree -i reqwest --target aarch64-apple-darwin`
- [x] T32b — `Workspace` の複数化と状態の写し(変更ファイル: crates/arin-core/src/workspace/, crates/arin-core/tests/workspace_terminal.rs, crates/arin-core/tests/workspace_projects.rs, crates/arin-core/tests/app_snapshot.rs, crates/arin-core/tests/generated_types.rs, src/ipc/generated/, src-tauri/src/lib.rs, src-tauri/src/sessions.rs, src-tauri/tests/support/mod.rs, src/test/fakeIpc.ts, src/state/, e2e/fixtures/tauriMock.ts, src/ui/shell/AppShell.tsx | 依存: T29b, T30a, T30b, T32a)
  - 対応: FR-002(切替でプロセスを止めない)、FR-003(削除の手順。動作中があれば確認を求める結果を返す)、FR-004(復元)、FR-039(起動時に解決できた全プロジェクトで 1 タブ。cwd はルート。C-22)、FR-042(切替はセッションに触らない)、FR-044、FR-063(終了の確認の材料)、設計書 §6.3・§7.1「起動」「プロジェクトの切替」
  - 写しの形(項目を足すだけで意味は変えないので、版は 1 のまま): `projects[]`(`id`・`name`・`color`・`displayPath`(`~` 省略。ツールチップ用)・`missing`・`terminalIds`(タブの順)・`activeTerminalId`)、`selectedProjectId`、`terminals[]`(`id`・`projectId`(検証画面では null)・`label`・`status`)、`layout`(左ペインの幅・折りたたみ・カード領域の高さ)、`notice`(退避したときの 1 行。閉じるまで)。絶対パスは写しに載せない(コピーは Rust が行う。設計書 §12)
  - `Workspace` は入出力をしない(M1 と同じ形)。起動時は「読んだ状態」と「各プロジェクトの解決の結果」を、削除と終了の確認は「動作中のターミナルの ID」を引数で受ける(調べるのは `src-tauri`)。保存する内容(`PersistedState`)を作る。検証画面は `Workspace::for_verify_grid`(プロジェクトなし、ホームを cwd にした 9 枚。`with_terminal_count` を置き換える)
  - `src-tauri` と画面は、組み立てと型を新しい形に合わせるだけ(状態の読み込みと保存は T33a)。この時点では起動時のプロジェクトは 0 件でターミナルも 0 枚になり、`npm run smoke` はターミナルの印で失敗する。T33a が終わるまで、着手前のスモークは `--env=ARIN_VERIFY_GRID=1` で行う
  - 並行: 可(T31c とファイルが重ならない)
  - 先に書くテスト: `起動時に解決できたプロジェクトだけに_1_タブずつ登録し_cwd_はルート`(C-22)、`見つからないプロジェクトにはタブを作らず_新しいタブも開かない`、`選択を変えてもターミナルの状態は変わらない`(FR-042 の Rust 側)、`復元すると同じ順で同じものが選択される`(AC-P04 の Rust 側)、`動作中があれば削除は確認を求め_確認なしでは何も変えない`(FR-003)、`確認して削除するとそのプロジェクトのタブが閉じる対象として返る`、`動作中が無ければすぐ削除できる`、`終了の確認の材料は動作中のターミナルのプロジェクト名とタブ名`(FR-063)、`写しの_json_の形`、`保存する内容に表示値(見つからない_表示用のパス)を含めない`、`検証画面は_9_枚でプロジェクトを持たず保存しない`。生成した型が最新
  - 検証: `npm test && npm run lint && npm run build && npm run e2e`(既存の画面の自動テストが新しい写しの形で通る)。`npm run app && node scripts/smoke.mjs --env=ARIN_VERIFY_GRID=1`

#### Phase 21(逐次。Phase 20 完了後。`src-tauri/src/lib.rs` と `src/ipc/` を順に触る)

- [x] T33a — 起動時の読み込み、保存、退避の知らせ(変更ファイル: src-tauri/src/lib.rs, src-tauri/src/persistence.rs, src-tauri/src/lifecycle.rs, src-tauri/Cargo.toml, src-tauri/tests/app_persistence.rs, scripts/smoke.mjs, scripts/smoke.test.mjs | 依存: T31a, T31b, T31c, T32b)
  - 対応: FR-004、FR-056、FR-001 の AC 3(プロジェクトのフォルダに何も作らない)、決定 (d)、設計書 §6.2「書き込みの契機」、§7.1「起動」、§11
  - 順: 置き場所 → `state.json` を読む(読めなければ退避して知らせを写しに載せる)→ 各プロジェクトのブックマークを解決する → `Workspace` → 保存のスレッド(0.5 秒まとめ。ウィンドウが前面でなくなったときと終了時は即時)。検証画面のときは読まず書かない
  - スモーク: 一時フォルダにプロジェクトのフォルダを作り、`bookmark: null` の `state.json` を一時の状態フォルダに置いて起動する(【仮定】13)。確かめる: ターミナルの印、終了後の `state.json` にブックマークが入っている、プロジェクトのフォルダに何も増えていない、本物の `~/Library/Application Support/Arin/` を作っていない。壊れた `state.json` を置いた起動も 1 回(退避のファイルが元の中身のままでき、新しい `state.json` が空の状態で書かれる)
  - 並行: 不可
  - 先に書くテスト(模擬ランタイム、一時の状態フォルダ。`harness = false` の `[[test]]` を足す): `状態ファイルが無ければ空で始まる`、`壊れた状態ファイルは退避され空で始まり知らせが写しに載る`、`版が新しすぎるときも同じ`、`退避に失敗したらその回は書かない`、`変更から少したつと状態ファイルが書かれる`、`終了のときに書かれる`、`検証画面では読まず書かない`。`scripts/smoke.test.mjs` に、状態ファイルを用意する部分の単体テストを足す
  - 検証: `npm test && npm run lint && npm run app && npm run smoke`(ここからスモークがふたたびそのまま通る)。`node scripts/smoke.mjs --env=ARIN_VERIFY_GRID=1`
- [x] T33b — ターミナルの複数化(タブのコマンドとセッションの後片付け)(変更ファイル: src-tauri/src/sessions.rs, src-tauri/src/commands/terminal.rs, src-tauri/src/lib.rs, src-tauri/tests/sessions.rs, src-tauri/tests/terminal_commands.rs, src-tauri/tests/session_pty.rs, crates/arin-core/src/workspace/, crates/arin-core/tests/generated_types.rs, src/ipc/generated/, src/ipc/client.ts, src/ipc/client.test.ts | 依存: T31d, T33a)
  - 対応: FR-039、FR-042、FR-044、NFR-006、M1 の見送り(終了済みセッションの解放、終了と同時の接続、`onBinary` の受け口、再接続で寸法を諦めたとき、書き込みが止まったままのシェル)
  - コマンド: `new_terminal`(選択中のプロジェクトに。メニューからも同じ関数を呼ぶ)、`close_terminal`、`select_terminal`、`write_terminal_bytes`(xterm.js の `onBinary` のバイト列をそのまま書く)。`connect_terminal` の応答に「寸法を反映したか」を足す(反映できなかったら画面が寸法を 1 回送り直す。T34b)
  - 後片付け: タブを閉じたとき・シェルが終わってタブが閉じたときに、セッションを一覧から外し、読み取りとまとめのスレッドを抜けさせる。PTY のロックが取れない(書き込みが止まっている)ときは印を付け、書き込みが戻った時点で終わらせる。終了の処理が始まったあとの接続はシェルを起動しない
  - 動作中の一覧: 各セッションの前面プロセス(T31d)を調べて返す関数を置く(T33c の削除と T33e の終了の確認で使う)
  - 並行: 不可
  - 先に書くテスト(偽 PTY): `タブを閉じるとそのセッションのスレッドが抜け一覧から消える`、`シェルが終わるとタブが閉じセッションが解放される`、`書き込みが止まったままのタブを閉じても他のコマンドが止まらず_書き込みが戻ったら終わる`、`終了の処理のあとの接続はシェルを起動しない`、`再接続で寸法を反映できなかったら応答でそう返す`、`バイト列の入力がそのまま書かれる`、`動作中の一覧は前面がシェル以外のものだけ`、`0_枚のプロジェクトに新しいタブを開ける`。実 PTY(`session_pty.rs`): `2_つのプロジェクトで_1_枚ずつ動かし片方を閉じてももう片方が入出力を続ける`(NFR-006)。Vitest: 新しいコマンドの包みが正しい名前と引数で届く
  - 検証: `npm test && npm run lint && npm run app && npm run smoke`
- [x] T33c — プロジェクトのコマンド(登録、選択、削除、色、再リンク、パスのコピー)(変更ファイル: src-tauri/src/commands/project.rs, src-tauri/src/commands/mod.rs, src-tauri/src/lib.rs, src-tauri/Cargo.toml, Cargo.toml, Cargo.lock, src-tauri/tests/project_commands.rs, crates/arin-core/src/workspace/, crates/arin-core/tests/generated_types.rs, src/ipc/generated/, src/ipc/client.ts, src/ipc/client.test.ts, output/design/ADR_009_m2-platform-deps.md | 依存: T33b)
  - 対応: FR-001(ダイアログ、ドロップ)、FR-002、FR-003、FR-007(再リンク)、FR-010、FR-059(色の変更)、FR-064、【仮定】6・7・8
  - ダイアログ: `tauri-plugin-dialog`(正確な版。公開から 2 週間未満を避ける)を Rust から呼ぶ。【要確認】このプラグインが `tauri-plugin-fs` を引き込むかを `cargo tree` で確かめる。引き込むなら、画面に何の権限も与えないこと(`capabilities/default.json` を変えない)を確かめて ADR-009 に書く。設計書 §12 は「ファイル操作のプラグインを入れない」としているので、`objc2-app-kit` の `NSOpenPanel` を直接呼ぶ案と並べて本人に選んでもらう
  - ドロップ: ウィンドウのドラッグ・ドロップのイベント(Rust)で、フォルダだけを登録する
  - コマンド: `add_project_by_dialog`、`select_project`、`remove_project`(確認の有無を引数に持つ。動作中があれば「確認が要る」と一覧を返す)、`relink_project`(ダイアログ)、`set_project_color`、`copy_project_path`(Rust がペーストボードに書く。選択を変えない)。削除ではそのプロジェクトのセッションを T33b の口で終わらせる
  - 並行: 不可
  - 先に書くテスト(模擬ランタイム、偽のダイアログ・偽のポート、一時フォルダ): `ダイアログで選んだフォルダが登録され_1_タブ開き選択される`、`ダイアログを取り消すと何も変わらない`、`同じフォルダは増えず既存が選択される`、`ドロップしたフォルダだけが登録されファイルは無視される`、`削除してもフォルダの中身は変わらない`(一時フォルダの中身を前後で比べる)、`動作中があれば確認を求め_中止なら何も変わらない`、`絶対パスのコピーは偽のペーストボードに正規化したパスを書き選択を変えない`、`再リンクで_id_と色を保つ`、`登録でプロジェクトのフォルダにファイルができない`
  - 検証: `npm test && npm run lint && npm run app && npm run smoke`。`cargo tree -i reqwest --target aarch64-apple-darwin` で通信のクレートが増えていない
- [x] T33d — ルートへの追従の見回りと、ペインの寸法の保存(変更ファイル: src-tauri/src/patrol.rs, src-tauri/src/commands/layout.rs, src-tauri/src/commands/mod.rs, src-tauri/src/lib.rs, src-tauri/Cargo.toml, src-tauri/tests/root_follow.rs, src/ipc/client.ts, src/ipc/client.test.ts | 依存: T33c)
  - 対応: FR-007(AC-P07 と「見つからない」)、FR-009(寸法の保存)、設計書 §7.1「ルートフォルダへの追従」
  - 1 秒間隔で開いたフォルダの現在位置を調べ、変わっていたら `Workspace` に反映する(ブックマークを作り直し、写しを送り、保存を予約する)。取れなくなったら「見つからない」にする(カードは残す。C-06)。監視の張り替えは M3(T42)。見回りの時刻は注入できるようにする
  - コマンド: `set_pane_sizes`、`set_sidebar_collapsed`(最小寸法の検査は T30b の規則)
  - 並行: 不可
  - 先に書くテスト(偽の時計と偽のポート。実物の一時フォルダで 1 件): `改名するとカードは同じプロジェクトのまま表示用のパスが新しくなる`(AC-P07 の Rust 側)、`削除すると見つからないになりカードは残る`、`ゴミ箱へ移すと見つからない`、`変わらなければ写しを送らない`、`寸法を送ると保存が予約され_最小を下回る値は最小になる`
  - 検証: `npm test && npm run lint && npm run app && npm run smoke`
- [x] T33e — メニューバーと終了の確認(変更ファイル: src-tauri/src/menu.rs, src-tauri/src/lifecycle.rs, src-tauri/src/events.rs, src-tauri/src/commands/app.rs, src-tauri/src/sessions.rs, src-tauri/src/lib.rs, src-tauri/Cargo.toml, src-tauri/tests/app_menu.rs, src-tauri/tests/app_quit.rs, crates/arin-core/tests/generated_types.rs, src/ipc/generated/, src/ipc/client.ts, src/ipc/client.test.ts | 依存: T33d)
  - 対応: FR-044(⌘T・⌘W・⌘⇧[・⌘⇧])、FR-063、決定 (b)、設計書 §9.1・§11(終了)、M1 の見送り(「すべてを選択」、終了時の後片付けが最大約 1.8 秒)
  - メニュー: 「ターミナル」に 新規タブ(⌘T)・タブを閉じる(⌘W)・前のタブ(⌘⇧[)・次のタブ(⌘⇧])。項目が選ばれたら Rust が `Workspace` を操作する(T33b の関数)。タブが 0 枚・プロジェクトが無い・見つからないときは何もしない。「編集」の「すべてを選択」は標準の項目から自前の項目に替え、`menu-command` イベントで画面へ送る(画面がフォーカス位置で振り分け、ターミナルなら xterm.js の全選択。T34b)
  - 終了: 終了の要求(⌘Q、ウィンドウの閉じるボタン)を一度止める。動作中があれば `quit-requested` イベント(プロジェクト名とタブ名の一覧)で画面に確認を出させ、`confirm_quit` で状態を即時保存して全セッションを終わらせてから終了する。動作中が無ければ確認せず、保存して終了する。【仮定】標準の「終了」の項目が終了の要求(`RunEvent::ExitRequested`)を経由するかを Tauri 2.12.5 のソースで確かめ、経由しなければ自前の項目に替える
  - 後片付け: 全セッションへ先に終了を送り、待ちはまとめて 1 回にする(全体でおよそ 1 枚ぶん。M1 では 9 枚で最大約 1.8 秒)
  - 並行: 不可
  - 先に書くテスト(模擬ランタイム、偽 PTY): `メニューに新規タブ_タブを閉じる_前後のタブがありアクセラレータが付いている`、`新規タブの項目で選択中のプロジェクトにタブが増える`、`タブが_0_枚のときタブを閉じるは何もしない`、`すべてを選択は画面へ_menu-command_を送る`、`動作中が無ければ終了は止めず保存してから終わる`、`動作中があれば終了を止め確認の要求を送る`、`確認すると保存してから終わり_取り消すと何も変わらない`、`9_枚の後片付けがおよそ_1_枚ぶんの時間で終わる`(偽 PTY の終了に 200ms 待たせ、全体の上限を 600ms にする)
  - 検証: `npm test && npm run lint && npm run app && npm run smoke`。`node scripts/smoke.mjs --env=ARIN_VERIFY_GRID=1`(終了後にシェルが残らない)

#### Phase 22(逐次。Phase 21 と T34a の完了後)

- [x] T34b — 画面のストアと操作、ターミナルの管理の複数化(変更ファイル: src/state/, src/terminal/TerminalManager.ts, src/terminal/TerminalManager.test.ts, src/terminal/engine.ts, src/terminal/xterm/XtermEngine.ts, src/test/fakeTerminalEngine.ts, src/test/fakeIpc.ts, src/ui/terminals/clipboardRouting.ts, src/main.tsx | 依存: T33e, T34a)
  - 対応: FR-002・FR-042(切替でサーフェスを破棄しない)、FR-044、設計書 §6.1 の `uiStore`・`actions`、§9.1、M1 の見送り(隠したまま作るインスタンスの寸法、`onBinary`、「すべてを選択」、出力のバイト列への変換の失敗、再接続で寸法を諦めたとき)
  - `uiStore`(開いている確認・メニュー、フォーカス位置)と `actions`(コマンドの包み)を生成関数で作る(設計書 §11)。写しから表示用の値を取り出す関数。`TerminalManager`: 写しから消えたターミナルを破棄して購読を外す。隠したまま作るサーフェスは、最後に表示していたターミナルの寸法で始める(エンジンに行・桁を指定する操作を足す)。`onBinary` を `write_terminal_bytes` へ直列で送る。`selectAll` を足す。接続の応答が「寸法を反映していない」なら寸法を 1 回送り直す。チャネルの中身をバイト列にできなかったら診断に報告する(回復は先送り。下の割当表)
  - 並行: 不可
  - 先に書くテスト(Vitest。偽のエンジンと偽の通信): `写しから消えたターミナルは破棄され購読が外れる`、`プロジェクトを切り替えてもサーフェスを破棄しない`、`隠したまま作ったサーフェスは表示中の寸法で接続する`、`表示中が無ければエンジンの既定の寸法で接続する`、`onBinary_のバイト列が順に送られる`、`すべてを選択はフォーカスのあるターミナルを全選択する`、`寸法を反映していない応答なら寸法を送り直す`、`バイト列にできない中身は報告する`、`終了の確認の要求で確認が開く`、`actions_は正しいコマンドを呼ぶ`
  - 検証: `npm test && npm run lint && npm run build && npm run e2e`(既存の画面の自動テストが通る)
- [x] T34c — 画面の枠: ペインの分割、カード一覧、ツリーのヘッダ、色、確認ダイアログ、知らせ(変更ファイル: src/ui/shell/, src/ui/projects/, src/ui/tree/TreeHeader.tsx, src/ui/dialogs/, src/ui/menu/, src/ui/design/, src/ui/terminals/VerificationGrid.tsx, e2e/fixtures/tauriMock.ts, e2e/projects.spec.ts | 依存: T34b)
  - 対応: FR-001(「フォルダを追加」)、FR-002(カードとツリーのヘッダの切替。ツリーの中身は M3)、FR-003(一覧から削除と確認)、FR-007(「見つからない」の表示、再リンク、ツールチップ)、FR-008、FR-009、FR-059(色の変更)、FR-060(カードのタイトル部、ツリーのヘッダ)、FR-064、設計書 §8「プロジェクト 0 件」「見つからない」、§9.2、【仮定】10・15・16
  - 色の値は `src/ui/design/palette.ts` にライト・ダークの 2 値で持つ(設計書 §9.3。M6 の T56 で両モードを人間が確認する)。退避の知らせは 1 行で出し、閉じられる
  - 検証画面のときは M1 と同じく検証画面だけを出す
  - 並行: 不可
  - 先に書くテスト(Playwright。偽の Rust 側): `1_プロジェクトに_1_枚のカードが出て選択中が強調される`(FR-008)、`カードの合計の高さが上段を超えると上段だけがスクロールしツリーの位置と高さが変わらない`(FR-008)、`カードをクリックすると選択のコマンドが呼ばれツリーのヘッダが切り替わる`、`フォルダを追加のボタンで登録のコマンドが呼ばれる`、`カードを右クリックすると絶対パスをコピーがあり_選ぶとコピーのコマンドが呼ばれ選択は変わらない`(FR-064。クリップボードの中身は T33c の Rust のテストで見る)、`色を選ぶと色の変更のコマンドが呼ばれる`、`見つからないカードは表示が変わり再リンクと一覧から削除を選べる`、`ツールチップに_~_の表示用のパスが出る`、`上段と下段の境界をドラッグすると寸法のコマンドが呼ばれ最小で止まる`、`左右の境界も同じ`、`左ペインを折りたためる`、`カードのタイトル部とツリーのヘッダが同じ色相`(計算済みのスタイルを比べる)、`削除の確認で中止すると削除のコマンドが呼ばれない`、`退避の知らせが_1_行で出て閉じられる`、`プロジェクトが_0_件ならフォルダを追加の案内が出る`。`e2e/verify-grid.spec.ts` がそのまま通る
  - 人間が見る範囲(T36): 色とペインの見た目、フォルダ選択ダイアログ、Finder からのドロップ、境界のドラッグの手触り
  - 検証: `npm test && npm run lint && npm run build && npm run e2e`
- [x] T34d — ターミナル領域: タブバー、タブ 0 枚、終了の確認(変更ファイル: src/ui/terminals/TerminalArea.tsx, src/ui/terminals/TabBar.tsx, src/ui/terminals/TerminalArea.css, src/ui/dialogs/, src/ui/shell/AppShell.tsx, e2e/fixtures/tauriMock.ts, e2e/tabs.spec.ts | 依存: T34c)
  - 対応: FR-039(複数タブ、0 枚、起動時に全プロジェクトで接続)、FR-042、FR-044、FR-060(タブの着色)、FR-061 の M2 の分(タブに文字の名前を出す)、FR-063、設計書 §8「タブ表示」「タブ 0 枚」、M1 の見送り(隠したまま作るインスタンスの寸法の画面での確認)
  - 全プロジェクトの全ターミナルの置き場所を描き、選択中のプロジェクトの表示中のタブだけを出す(他は `setVisible(false)`。起動時に全プロジェクトのターミナルが接続する=C-22)。表示中のタブが変わったらそのターミナルにフォーカスを移す。描画方式の表示(`RendererLabel`)は表示中のターミナルについて出す(M4 まで)。【実装での変更】置き場所は表示中の 1 つだけを描き、ほかのターミナルは `TerminalManager` が DOM の外に隠したまま持つ(接続と取り込みは続く。隠したまま作ったターミナルへ切り替えたときの描画と寸法は `e2e/tabs.spec.ts` で確認)
  - 並行: 不可
  - 先に書くテスト(Playwright): `タブバーに選択中のプロジェクトのタブが並び表示中が強調される`、`タブをクリックすると切り替わる`、`写しでタブが増えると新しいタブが表示されフォーカスが移る`(⌘T のキー自体は OS のメニューが受けるので、自動テストでは写しのイベントで代える)、`0_枚のときターミナルがありませんと新規ターミナルのボタンが出る`(C-21)、`新規ターミナルのボタンで新しいタブのコマンドが呼ばれる`、`プロジェクトを切り替えても前のプロジェクトのターミナルに出力が取り込まれ続ける`(隠したターミナルからも処理済みの量が返る)、`隠したまま起動したターミナルの最初の寸法が表示中と同じ`、`タブがプロジェクト色で着色されタブ名の文字がある`、`終了の確認の要求で動作中の一覧が出て_終了で確定のコマンド_取り消しで何も呼ばない`。`e2e/verify-grid.spec.ts` がそのまま通る
  - 人間が見る範囲(T36): ⌘T・⌘W・⌘⇧[・⌘⇧] の実際のキー、⌘Q と閉じるボタンでの確認、「すべてを選択」
  - 検証: `npm test && npm run lint && npm run build && npm run e2e && npm run app && npm run smoke`

#### Phase 23a(並行可能。T34d 完了後)

- [x] T35a — M2 のシナリオの画面の自動テスト(変更ファイル: e2e/m2-scenario.spec.ts, e2e/fixtures/tauriMock.ts | 依存: T34d)
  - 対応: PRD §8「UI: 起動 → プロジェクト追加 → 切替」、FR-042(AC-M04)の画面側、FR-004(AC-P04)と FR-009(AC-P09)の画面側
  - 並行: 可(T35b とファイルが重ならない)
  - 先に書くテスト(このタスク自体がテスト): `起動_追加_切替_タブ操作_削除の確認_終了の確認を通す`、`A_で_1_秒ごとに出力を流し_B_に切り替えて_10_秒後に_A_へ戻ると途切れず進んでいる`(偽のチャネルで流す。【仮定】間隔は短くしてよい)、`届いた写しのとおり同じ順_同じ選択_同じペイン配分で描かれる`
  - 検証: `npm run e2e`(3 回続けて合格)
- [x] T35b — 実物の結合テストと計測の準備(変更ファイル: src-tauri/tests/session_pty.rs, crates/arin-platform/tests/pty.rs, scripts/perf/, scripts/perf-run.test.mjs | 依存: T34d)
  - 対応: FR-042(AC-M04 の Rust 側)、NFR-006、M1 の見送り(終了の合図をシェルの pid にだけ送るので子孫が残らないか)、NFR-001〜003 の計測の準備
  - 計測: 5 プロジェクト・各 1 タブの状態を作れるようにする(一時フォルダを 5 つ作り、`bookmark: null` の `state.json` を渡す。【仮定】13)。数値で失敗させない
  - 並行: 可
  - 先に書くテスト(実 PTY。直列、上限時間): `A_で_for_i_in_seq_1_30_を流しながら_B_を操作しても_A_の出力が続く`(AC-M04 の Rust 側)、`A_のタブを閉じても_B_が入出力を続ける`、`A_のシェルを外から_kill_しても_B_が続く`(NFR-006)、`前面で_sleep_を動かしたタブを閉じると_sleep_も終わる`(子孫が残らない)
  - 検証: `npm test && npm run app && npm run smoke && npm run perf`

#### Phase 23b(逐次。人間ゲート)

- [ ] T36 — M2 の完了確認(変更ファイル: output/tasks/PROGRESS.md, output/reports/test/M2_acceptance.md, output/design/ARCH_multi-terminal.md, docs/project.md, docs/architecture.md, docs/data-model.md, docs/development-patterns.md, project-config.md | 依存: T28a, T28b, T35a, T35b)
  - AI がやること: `npm run build && npm test && npm run lint && npm run test:coverage && npm run e2e && npm run app && npm run smoke && npm run perf` と `node scripts/smoke.mjs --env=ARIN_VERIFY_GRID=1` を通し、下の対応表に沿って AC の合否一覧を `output/reports/test/M2_acceptance.md` と `testreport/` に出す。NFR-001〜003 を 5 プロジェクト・各 1 タブで計測して記録する(合否にしない)。`docs/`(`state.json` 版 1、写しの形、コマンドとイベント、`uiStore`、新しい落とし穴)と `project-config.md` §2・§3 を同期する。設計書の §15.1 #10(T28a の結論)と §2(M2 で足した依存)を直す。fresh context の `/code-review` と `verifier` を 1 回ずつ回す
  - **人間がやること**(`npm run app` で作った `.app` を Finder から開く): (1) ターミナルにフォーカスがあるときに ⌘T・⌘W・⌘⇧[・⌘⇧] が効く(US と JIS の配列。決定 (b))。(2) ⌘C・⌘V・「すべてを選択」(⌘A)がターミナルで効く(M1 で未確認のまま。メニューを変えたので見直す)。(3) フォルダ選択ダイアログと Finder からのドロップで登録できる(AC の FR-001)。(4) Finder でルートを改名するとツールチップが変わり、削除すると「見つからない」になる(AC-P07)。(5) Claude Code を A で動かしたまま B に切り替え、戻ると続いている(AC-M04)。(6) 動作中があると ⌘Q と閉じるボタンで確認が出る(FR-063)。(7) 再起動すると同じ並び・選択・ペイン配分で開く(AC-P04、AC-P09)。(8) 体感。(9) 任意: `npm run dev` で起動できるか(M1 の T18 で未確認の `devCsp`)
  - 完了条件: 下の「M2 の完了条件」をすべて満たす

#### M2 の完了条件

- 下の対応表の AC が、自動テストと T36 の人間の確認で全件合格している(M3・M4 に回した部分を除く)
- `npm run build`・`npm test`・`npm run lint`・`npm run test:coverage`(`arin-core` と `src/logic` のそれぞれで行 80%)・`npm run e2e`・`npm run smoke`(通常と `--env=ARIN_VERIFY_GRID=1`)が終了コード 0
- 本物の `~/Library/Application Support/Arin/` を、テストとスモークが作っていない
- 下の「M1 の見送り項目の割当」の M2 の分が済んでいる
- T28a の結論が ADR-008、M2 で足した依存が ADR-009 にある
- 本人が T36 の (1)〜(8) を確認した

#### FR の受け入れ基準とタスクの対応(漏れの確認用)

| FR | 受け入れ基準 | 規則・OS | コマンド | 画面 | 人間(T36) |
| -- | ------------ | -------- | -------- | ---- | ---------- |
| FR-001 | ダイアログでフォルダを選ぶとカードが 1 枚増える | T29b | T33c | T34c | (3) |
| FR-001 | Finder からのドロップでカードが 1 枚増える | T29b | T33c | — | (3) |
| FR-001 | 登録でプロジェクトのフォルダに Arin のファイルができない | T31b | T33a(スモーク)、T33c | — | — |
| FR-002 | B をクリックすると B のタブ群が出て、A のプロセスが終わらない(ツリーのルートの部分は M3 の T43) | T32b | T33c | T34c、T34d、T35a | (5) |
| FR-003 | 削除でカードが消え、フォルダとその中身が残る | T29b | T33c | T34c | — |
| FR-003 | 動作中があると確認が出て、中止で何も変わらない | T31d、T32b | T33b、T33c | T34c | — |
| FR-004 | A・B・C を登録し B を選んで再起動すると、同じ順で B が選択されている(AC-P04) | T30a、T31a、T32b | T33a | T35a | (7) |
| FR-007 | Finder で改名すると、同じカードのままツールチップのパスが変わる(AC-P07。「ツリーも表示され続ける」は M3 の T43) | T29b、T31b | T33d | T34c | (4) |
| FR-007 | 削除すると「見つからない」で残り、再リンクと一覧から削除を選べる | T29b、T31b | T33c、T33d | T34a、T34c | (4) |
| FR-008 | 1 プロジェクト 1 カードで、選択中が強調される | — | — | T34c | (8) |
| FR-008 | カードが上段を超えると上段だけがスクロールし、下段が動かない | — | — | T34c | (8) |
| FR-064 | カードの右クリックに「絶対パスをコピー」がある | — | — | T34a、T34c | — |
| FR-064 | 選ぶとクリップボードにルートの絶対パスが入る | T31c | T33c | T34c | — |
| FR-064 | 選択中でないカードでコピーしても選択が変わらない | — | T33c | T34c | — |
| FR-009 | 上段と下段の配分が再起動後も同じ(AC-P09) | T30a、T30b | T33d | T34c、T35a | (7) |
| FR-009 | 上段・下段とも最小高さを下回らない(AC-P09) | T30b | T33d | T34a、T34c | — |
| FR-009 | 左右の幅が再起動後も同じ | T30a、T30b | T33d | T34c、T35a | (7) |
| FR-009 | 左ペインを折りたためる | T30b | T33d | T34a、T34c | — |
| FR-010 | 同じフォルダを再登録しても増えず、既存が選択される(AC-P10) | T29b、T31b | T33c | — | — |
| FR-010 | `~/dev/app/frontend` は別のカードとして増える(AC-P10) | T29b | T33c | — | — |
| FR-039(M2 の分) | 複数タブを持て、0 枚も許す | T32a | T33b | T34d | — |
| FR-039(M2 の分) | 新規タブの初期 cwd がプロジェクトルート | T32b | T33b | — | — |
| FR-039(M2 の分) | 起動時に全プロジェクトで 1 タブずつ開く(C-22) | T32b | T33a | T34d | (7) |
| FR-042 | A でループを流し、B に切り替えて 10 秒後に戻ると途切れず進んでいる(AC-M04) | T32b | T33b、T35b | T34b、T34d、T35a | (5) |
| FR-044 | ⌘T で追加、⌘W で閉じる、⌘⇧[・⌘⇧] で前後に切り替わる | T32a | T33b、T33e | T34a、T34d | (1) |
| FR-044 | 最後の 1 枚を ⌘W で閉じると 0 枚で、自動で開かない。0 枚から ⌘T で開ける(C-21) | T32a | T33e | T34d | (1) |
| FR-056 | プロジェクト一覧と UI 状態が状態フォルダの JSON に保存される | T30a、T30b、T31a | T33a | — | — |
| FR-056 | プロジェクトのフォルダ内に何も保存しない | — | T33a(スモーク)、T33c | — | — |
| FR-059 | 8 色で 3 件登録すると 3 件の色がすべて違う(AC-C01) | T29b | — | — | — |
| FR-059 | 割り当てた色をあとから変えられる | T29b | T33c | T34c | — |
| FR-060 | タブ表示で、カードのタイトル部・ツリーのヘッダ・タブが同じ色相 | — | — | T34c、T34d | (8) |
| FR-060 | タイル表示の着色(AC-C02) | M4(T47・T49) | | | |
| FR-061 | タイルのタイトルに「プロジェクト名: タブ名」(M2 の分はタブに文字の名前を出すこと) | T32a | — | T34d | — |
| FR-063 | 終了時に動作中の子プロセスがあると確認が出る | T31d、T32b | T33b、T33e | T34b、T34d | (6) |
| NFR-006 | 1 枚の終了が他のプロジェクトを巻き込まない | — | T33b、T35b | — | — |
| NFR-001〜003 | 計測して記録する(合否にしない) | — | T35b | — | T36 |

#### M1 の見送り項目の割当

`output/tasks/PROGRESS.md` のセッションログ(2026-11-15 の M1 の実装と締め)と ADR-001 の「影響」から拾った。

| 項目(出どころ) | 扱うタスク | 扱い |
| ---- | ---- | ---- |
| 寸法に上限が無い(65535×65535 も通る。レビューで見送り) | T32a | 行 1,000・桁 2,000 を上限にする(【仮定】11) |
| シェルの終了後もセッションと読み取りスレッドがアプリの終了まで残る(同上) | T32a、T33b、T34b | タブを自動で閉じ(【仮定】5)、セッションを外してスレッドを抜けさせ、画面はサーフェスを破棄する |
| 終了時の後片付けが 1 枚につき最大約 200ms ずつ順に進む(9 枚で最大約 1.8 秒)(同上) | T33e | 先に全部へ終了を送り、待ちをまとめて 1 回にする |
| 終了と同時に走っている接続が後片付けから漏れ得る(同上) | T33b | 終了の処理が始まったあとの接続はシェルを起動しない |
| 出力のバイト列への変換に失敗した分は処理済みの通知が返らない(同上) | T34b(報告だけ) | 失敗を診断に報告する。回復(再接続で未処理の量を捨てる)は M6 に先送りし、T55 の再分解で取り込むかを決める。Tauri 2.12.5 は生のバイト列を常に `ArrayBuffer` で渡すので、起きる経路が見つかっていないため |
| xterm.js の `onBinary` を PTY へつないでいない(同上) | T33b、T34b | バイト列を受けるコマンド `write_terminal_bytes` を足し、直列で送る |
| メニューの「すべてを選択」が xterm.js ではなく文書全体を選ぶ可能性(同上。手順書 2.7-6) | T33e、T34a、T34b、T36(人間) | 標準の項目を自前の項目に替え、画面がフォーカス位置で振り分ける |
| 再接続で寸法の変更を諦めたとき、画面の寸法が変わらないと PTY の寸法が古いまま残る(同上) | T33b、T34b | 接続の応答に「寸法を反映したか」を足し、反映していなければ画面が送り直す |
| 隠したまま作るインスタンスの寸法(T22。M1 の画面では起きないので未実装) | T34b、T34d | 最後に表示していたターミナルの寸法で始める(設計書 §2.2 の案) |
| 前面プロセスグループの取得がポートに無い(残っている課題) | T31d | M2 では削除と終了の確認に使う。1 秒の見回りと実行中の印は M6(T57) |
| `kill` はシェルの pid にだけ送るので、切り離された子孫が残らないか未確認(同上) | T35b | 前面で `sleep` を動かしたタブを閉じて確かめる。残るなら不具合として直す |
| 書き込みが止まったままのシェルは、アプリの終了時に明示的に終わらせず OS の SIGHUP に任せる(同上) | T33b | タブを閉じるときは、書き込みが戻った時点で終わらせる。アプリの終了時は SIGHUP に任せたまま(PTY の親側が閉じるため。変えない) |
| `npm run dev` が未実行で、`devCsp` の `script-src` を緩めていないため開発時に動かない可能性(T18 の未確認) | T36(人間。任意) | 本人が `npm run dev` で 1 回起動して見る。AI は `npm run dev` を使わない |
| `docs/data-model.md` と `docs/architecture.md` の更新(残っている課題) | T36 | M2 の分とまとめて同期する |
| M1 の実機で未確認の項目のうち ⌘C・⌘V(ADR-001「確かめた範囲」) | T36(人間) | メニューを変えるので M2 の完了確認で見る。それ以外(WebGL の見た目、9 枚、最小化して 5 分、Option・Ctrl、OSC 52、Codex)は M2 で扱わない。見つかった時点で不具合として扱う(ADR-001) |
| 描画方式と全角幅の方式の決定、起動時の指定 3 つと `RendererLabel` の撤去(ADR-001) | M4(T49) | M2 では触らない。T34d で表示中のターミナルについて `RendererLabel` を出し続ける |
| 複数枚のときの流量制御のしきい値(ADR-001) | M4 | 1 枚の値のまま |
| Option を Meta にする設定(ADR-001) | M6(T56) | M2 では触らない |
| `scripts/smoke.test.mjs` の一時フォルダの数の検査が、ほかのテストと同時に動かすと落ちる(既知) | 扱わない | 単独で再実行して確かめる運用のまま(`docs/development-patterns.md` §6)。T33a で `smoke.mjs` を触るときに、同じ検査を増やさない |

### M3 ツリーとファイル操作(T37 で 1 セッション粒度に分解し直した。2026-11-15)

完了条件: FR-013〜018(Quick Look を除く)、024〜030、035〜037 の AC 合格と、VSCode を閉じて 1 週間のドッグフーディング(対応表は節の末尾)。

#### M3 の前提

T37 で決めたこと(ADR-007。【仮定】として扱い、異論があれば T45 までに直す):

- (a) ゴミ箱の方式: **【検証中】**(司令塔が実機で検証している)。候補は `NSFileManager` の `trashItemAtURL` → `NSWorkspace` の `recycleURLs` → Finder に頼む、の順。**結果は ADR-007 の (a) の欄に書く。ゴミ箱の実装(T40c)とそれに依存するタスクは、結果が出るまで始めない**
- (b) 【仮定】アプリ内のドラッグ(ツリー内の移動・コピー、ツリーからターミナルへの挿入)は pointer のイベントで自前に作る。HTML 標準のドラッグ&ドロップは使わない(設計書 §1.4 #8: Tauri が Finder からのドロップを受ける設定では画面に届かない見込み)。規則は `src/logic/dnd`(T41b)、確かめるのは Vitest と Playwright(WebKit。`page.mouse`)。実機の WKWebView での確認は本人が T45 で行う
- (c) 【仮定】ツリーの仮想スクロールは、依存を足さずに自作する(行の高さを 1 つに固定した窓掛け。計算は `src/logic/tree` の純粋な関数。T41a)。React 用のライブラリは入れない
- (d) M3 で足す依存は `notify`(T40b)だけ。正確な版は実装のときに `cargo search notify` と crates.io の公開日で確かめ、公開から 2 週間未満を避けて `=` で固定し、ADR-007 の追記に書く。`objc2-foundation`・`objc2-app-kit` は版を変えず feature だけ足す(T40c・T40d)
- (e) 【要件の追加(本人。2026-11-15)】ショートカットのキーはあとで設定ファイルで変えられるようにする(M4.5)。そのため M3 で足すショートカットは、キーの文字列をコードのあちこちに直書きせず、「コマンドの ID → 既定のキーと受け口」の 1 つの表(`arin-core/shortcuts`。T38d)から引く。メニューバーの項目のアクセラレータ(`src-tauri/src/menu.rs`)は T42e で表から作るように替え、画面(`src/logic/keys`)は T41b で表を受け取って振り分けるように替える。OS のメニューで受けるキーと、画面(ツリーにフォーカスがあるとき)で受けるキーを、同じ ID で扱う

安全に関わる決まり(M3 の全タスク共通。各タスクの「安全」にも書いた):

- 画面から絶対パスを受け取らない。ファイルは「プロジェクト ID + ルート基準の相対パス」で指す(設計書 §5.2・§12)。絶対パスとブックマークを画面へ返さない(パスのコピーは Rust がペーストボードに書く)
- ルートの外に出る経路を拒否する: 絶対パス、`..`・`.`・空の要素、NUL を含む相対パス、シンボリックリンクで親がルートの外に出るもの(設計書 §7.2)。ルート自体は名称変更・移動・切り取り・ゴミ箱の対象にしない(FR-035)
- 上書きしない: 作成・移動・コピーは「既にあれば失敗する」API だけで行う。「置き換える」は既存をゴミ箱に入れてから置く(設計書 §1.3 #11)
- 削除はゴミ箱だけ。ゴミ箱を経由しない削除の口をポートにも作らない(FR-028)
- テストでファイルに触るのは、補助が作る一時フォルダの中だけ(補助は渡された経路がシステムの一時領域の下にあることを確かめる。設計書 §10.2)。本物のゴミ箱を使うテストは、自分が入れた項目だけを片付ける

本計画が置いた【仮定】(M3 の分):

1. ツリーのヘッダ(M2 の `TreeHeader`)をルートの行として扱う。クリックでルートが選択され、右クリックでフォルダの上のメニュー(名称変更・切り取り・ゴミ箱に入れるは無効)を出す。FR-035 の AC(ルートを選択したとき)をこれで満たす。ルートは常に開いている
2. 並び順: フォルダ(とフォルダへのリンク)が先、その中は名前の順。名前は大文字・小文字を区別せずに比べ、同じならバイトの順(決定的にするため)。数字の自然順は使わない(Finder と違う)
3. 隠しファイルの扱いは設計書 §15.2 #1 の A(既定で隠すのは `.git`・`node_modules`・`.DS_Store` の 3 つだけ。切替を ON にすると 3 つも出す)。切替はプロジェクトごとに `state.json` の `projects[].tree.showHidden` に保存する。展開状態(`tree.expanded`)は保存しない(FR-020 は M6 の T59)
4. 画面はプロジェクトを切り替えてもツリーの読み込み済みの中身と展開を、起動中はメモリに持つ(戻ったら読み込み済みのフォルダを読み直す。設計書 §7.1「プロジェクトの切替」)。保存はしない
5. 行の高さは 22px に固定する(`src/ui/design/tokens.css`。ADR-007 の (c))。値は T43b で本人が見て直してよい
6. クリックで選択、ダブルクリックでファイルは既定のアプリで開き、フォルダは開閉する。開閉の印(▸)のクリックでも開閉する。矢印キーでの行の移動は PRD に無いので入れない(要ればドッグフーディングの不満として M6 で扱う)。選択は 1 つだけ(複数選択は FR-033。M6)
7. M3 のメニューに出すのは PRD §6.4 の Must の項目だけ。Should の項目(このアプリケーションで開く、クイックルック、ここでターミナルを開く、複製)は M6 まで出さない(無効にして並べることもしない)
8. 「ペースト」は常に有効にし、ペーストボードにファイルが無ければ何もしない(メニューを開くたびに Rust へ問い合わせない)
9. インライン編集(新規作成・名称変更)は Enter で確定、Esc で取り消し。入力欄からフォーカスが外れたら取り消す(意図せずファイルを作らないため。VSCode は確定する)
10. 名称変更の選択範囲: ファイルは最後の `.` の前まで(`a.tar.gz` は `a.tar`)。`.` で始まり他に `.` が無い名前(`.env`)と拡張子の無い名前、フォルダは全体
11. 「両方残す」の名前は Finder にならい、拡張子の前に「空白 + 番号」を足す(`a.md` → `a 2.md`、`docs` → `docs 2`、`.env` → `.env 2`)。番号は 2 から空いている最小のもの
12. 移動の確認(FR-029、C-05)は、ツリー内のドロップでの移動にだけ出す。⌥ でのコピーと、切り取り → ペーストでの移動には出さない(ペーストは利用者が明示した操作のため)。確認の省略の設定は M6(T56。下の「判断が要る点」)
13. 切り取りはアプリ内の状態(切り取った項目とペーストボードの変更番号)で表し、ペーストボードにはファイルの URL を書く。ペーストのときにペーストボードが切り取り時から変わっていなければ移動、変わっていればコピー(設計書 §9.1)。ペーストのコピー元はルートの外(Finder でコピーしたもの)でもよい(コピー元は読むだけ。変更するのは貼り付け先=ルートの中だけ)。別のプロジェクトのルートへの切り取り → ペーストも許す(どちらも登録済みのルートの中)
14. 別のボリュームへの移動(`EXDEV`)は失敗として「別のディスクへは移動できません」を出し、コピーしてから消すことはしない
15. 大文字・小文字だけを変える名称変更(`a.ts` → `A.ts`)は、同じ項目なので衝突にしない(APFS は既定で大文字・小文字を区別しないため、`RENAME_EXCL` だけでは失敗する。T40a で確かめる)
16. 「パスをターミナルに挿入」(⌘↩)の宛先は、選択中のプロジェクトの表示中のタブ(タブ表示では表示中=フォーカス中とみなす)。タブが無ければ項目は無効。別のプロジェクトのターミナルへの挿入(タイル表示。M4)は、設計書 §15.2 #6 の A(違うプロジェクトなら絶対パス)の規則を T38b で先に入れておく
17. 制御文字(改行・タブなど C0 と DEL)を含む名前は、挿入の文字列を全体を `$'…'` で包み、制御文字を `\xNN` で書く(PTY に改行のバイトを送らない。設計書 §7.1・§12)。それ以外の特殊文字はバックスラッシュで 1 文字ずつエスケープする
18. 更新の保留(FR-037)の理由は、ドラッグ中・名前の入力中・コンテキストメニューの表示中に加えて、確認ダイアログ(衝突・移動)の表示中も含める
19. 監視の変更が 1 回に 1,000 件を超えたら、変わったフォルダの一覧ではなく「全部読み直し」の印だけを送る(設計書 §7.1。件数は【仮定】)
20. ファイル操作が成功したら、画面は FSEvents を待たずに関わるフォルダをすぐ読み直す(操作の手応えを速くするため。後から届く変更は差分が無ければ何も変えない)
21. ショートカットの受け口: ⌘C・⌘X・⌘V は標準の「編集」の項目(画面のコピー・切り取り・ペーストのイベント)で受け、フォーカス位置でターミナルかツリーに振り分ける(M1 の方式。「切り取り」の項目は T42e で足す)。ツリーのほかのキー(⌘⌫・⌘N・⇧⌘N・⌘O・⌥⌘R・⌘↩・⇧⌥⌘C・⌥⌘C・Enter)は、メニューバーに項目を足さず、ツリーにフォーカスがあるときに画面の keydown で受ける。WKWebView がメニューに無い ⌘ の組み合わせを画面へ渡すかは実機で確かめる(T45 の (5))。渡らないキーがあれば、表の受け口をそのキーだけ「メニューの項目」に替える(表の 1 行の変更で済む形にしておく)
22. ⌥ を含むキー(⌥⌘R・⇧⌥⌘C・⌥⌘C)は、`KeyboardEvent.key` が ⌥ で別の文字になるため、文字のキーは `code`(物理的な位置。`KeyR` など)で比べる。US と JIS の配列で効くかを T45 で確かめる
23. 本物のゴミ箱を使うテストは `#[ignore]` にし、T40c と T45 で `cargo test -p arin-platform --test trash -- --ignored` を明示して動かす(`npm test` のたびにユーザーのゴミ箱へ出し入れしないため)。コマンドの結合テスト(T42c)は偽のゴミ箱のポートを使う
24. 名前が UTF-8 でない項目は扱わない(APFS は UTF-8 でない名前を作れない。HFS+ などの古いボリュームは確かめない)

#### 現状のコードから分かった、M3 で足すところ・変えるところ

- `arin-core` の `lib.rs` は `launch_options`・`layout`・`paths`・`persistence`・`ports`・`project`・`terminal`・`workspace` だけを宣言している。`tree`・`fileops`・`shortcuts` は無い → T38a で宣言と骨格を置く
- `arin-core/paths` は状態の置き場所(`AppPaths`)だけ(`crates/arin-core/src/paths/mod.rs` の「相対・絶対パスの整形とシェルエスケープは M3 で足す」)→ T38a(相対パスの型)、T38b(整形とエスケープ)
- `arin-core/ports` に `FileSystemPort`・`WatcherPort`・`DesktopPort` が無い。`PasteboardPort` は文字列を書く口だけ(`ports/pasteboard.rs`)→ T38a でポートを足す。ファイルの URL の読み書きは、既存の偽物(`src-tauri/tests/` など)を書き換えずに済むよう、別の trait(`FilePasteboardPort`)にする
- `ProjectStore` と `state.json` の codec はツリーの保存設定を持たない。写しの `ProjectCardSnapshot` にも無い → T38e(`tree.showHidden` だけ)
- `arin-platform` に `fs/` が無い(`lib.rs` は `bookmark`・`desktop`・`pty`・`storage`・`system`・`watch`)→ T40a。`watch/mod.rs` はルートの正規化と `F_GETPATH` だけで、FSEvents は「M3 で足す」とある → T40b。`desktop/` はペーストボードの文字列とフォルダ選択のダイアログだけ → T40d
- ルートへの追従の見回り(`src-tauri/src/patrol.rs`)は、ルートの位置が変わったら `SessionRegistry::follow_project_root` で写しを送る。監視の張り替えは「M3」とある → T42b は見回りを変えず、写しのたびに選択中のプロジェクトとそのルートのパスを比べて張り替える(写しの受け口は `lib.rs` の `listener`)
- `src-tauri/src/commands/` は `app`・`terminal`・`project`・`layout` だけ。イベントは `app-snapshot`・`menu-command`・`quit-requested` だけ(`events.rs`)→ T42a〜T42e
- `src-tauri/src/menu.rs` は項目の ID とアクセラレータの文字列(`Cmd+T` など)を定数で直書きしている。「編集」は コピー・ペースト(標準)とすべてを選択(自前)だけで、「切り取り」が無い → T42e で表から作るように替え、「切り取り」を足す
- `src/logic/keys/keyOwner.ts` は、アプリが受ける ⌘ のキーを集合(`COMMAND_KEYS`・`COMMAND_SHIFT_KEYS`)で直書きしている。`clipboard.ts` のフォーカス位置はターミナルかそれ以外だけで、「切り取り」が無い → T41b で表から作り、フォーカス位置に「ツリー」と、種類に「切り取り」を足す
- `src/ui/tree/` は `TreeHeader.tsx` だけ。`src/ui/shell/TreePane.tsx` はヘッダと空の領域と「見つからない」の案内だけを出す(M2 の【仮定】16)→ T43b
- `src/ui/menu/ContextMenu.tsx` は無効の項目とショートカットの表示を持たない(カードのメニュー用)→ T43c で足す。`src/ui/dialogs/` は削除と終了の確認だけ → T43c(衝突)、T43d(移動の確認)
- `uiStore` はフォーカス位置を持たず、コピー・ペーストは `TerminalManager` がフォーカスのあるサーフェスを調べて振り分ける(`src/state/uiStore.ts` の冒頭)→ T43a でツリーにフォーカスがあるかを足す
- 画面の自動テストの偽物(`e2e/fixtures/tauriMock.ts`)は、写しの形とプロジェクトのコマンドだけを持つ → T38e(写しの形)、T43b〜T43d(ツリーとファイル操作のコマンド、`tree-changed` イベント)

#### 親タスクと子タスク

T38〜T44 は親として残し、作業は子タスクで行う。親は子がすべて済んだら済みにする。T37 と T45 は分けない。T38 は「パスと除外の規則」に加えて、ショートカットの表(T38d、【要件の追加】)と隠しファイル表示の保存値(T38e)も受け持つ(`arin-core` の規則と型の土台をまとめるため)。

| 親 | 子 |
| -- | -- |
| T37 — M3 の最初の検証と再分解 | 分けない((a) は司令塔が検証中。(b)(c)(d) は ADR-007 と本節) |
| T38 — パスと除外の規則(と `arin-core` の土台) | T38a(ポートと型、モジュールの骨格、相対パスの型)、T38b(パスの整形とシェルエスケープ)、T38c(除外・並び順・変更のまとめ)、T38d(ショートカットの表)、T38e(隠しファイル表示の保存値と写し) |
| T39 — ファイル操作の手順 | T39a(範囲・名前・重複しない名前の検査)、T39b(操作の手順・衝突・切り取り) |
| T40 — OS 側の実装 | T40a(フォルダの読み込みと上書きしない作成・移動・コピー)、T40b(ファイル監視)、T40c(ゴミ箱。**T37 (a) の結果に従う**)、T40d(Finder で表示・既定のアプリで開く・ファイルのペーストボード) |
| T41 — 画面の純粋ロジック | T41a(ツリーの行・差分・保留・仮想スクロール・名前の選択範囲)、T41b(ドラッグ・メニュー・キーの振り分け) |
| T42 — コマンドとイベント、監視の張り替え | T42a(ツリーのコマンドと隠しファイル表示)、T42b(監視の張り替えと `tree-changed`)、T42c(ファイル操作のコマンド)、T42d(デスクトップとパスのコマンド)、T42e(メニューバーとショートカットの表) |
| T43 — 画面 | T43a(ツリーのストアと操作)、T43b(ツリーの表示)、T43c(メニュー・ショートカット・インライン編集・衝突の確認)、T43d(ドラッグ) |
| T44 — 同時操作のテスト、画面の自動テスト、NFR-005 の計測 | T44a(実ファイルの同時操作・シェルへの往復・NFR-005 の計測)、T44b(M3 のシナリオの画面の自動テスト) |
| T45 — M3 の完了確認とドッグフーディング | 分けない |

#### 共有のファイルの持ち回り

同じ Phase で 2 つのタスクが同じファイルを触らないように割り当てた。下のファイルは表の順に逐次で触る。

| ファイル | 触るタスク(この順) |
| -------- | ------------------ |
| `crates/arin-core/src/lib.rs`、`crates/arin-core/src/ports/mod.rs`、`crates/arin-core/src/paths/mod.rs`、`crates/arin-core/src/tree/mod.rs`、`crates/arin-core/src/fileops/mod.rs`、`crates/arin-core/src/shortcuts/mod.rs` | T38a だけ(Phase 25b の並行のため、子モジュールの宣言を先に済ませる) |
| `crates/arin-core/src/ports/`(ポートの trait) | T38a だけ(後のタスクで足りない操作が出たら、T42c の後に逐次で足す) |
| `crates/arin-core/src/tree/types.rs`、`crates/arin-core/src/fileops/types.rs`(画面とやり取りする型) | T38a だけ(T39b は型を変えない。足りなければ T42c で足す) |
| `crates/arin-core/tests/generated_types.rs`、`src/ipc/generated/` | T38a → T38d → T38e → (T42a〜T42e で型を足すときだけ、その順) |
| `crates/arin-core/src/project/store.rs`、`crates/arin-core/src/persistence/codec.rs`、`crates/arin-core/src/workspace/` | T38e だけ |
| `crates/arin-core/tests/support/fake_fs.rs`(偽のファイルシステム) | T39a → T39b |
| `Cargo.toml`、`Cargo.lock`、`crates/arin-platform/Cargo.toml` | T40b → T40d → T40c |
| `crates/arin-platform/src/lib.rs` | T40a だけ(`fs` の宣言。`watch`・`desktop` は宣言済み) |
| `crates/arin-platform/src/fs/mod.rs` | T40a → T40c |
| `crates/arin-platform/src/watch/mod.rs` | T40b だけ |
| `crates/arin-platform/src/desktop/mod.rs` | T40d だけ |
| `src-tauri/src/lib.rs`、`src-tauri/src/commands/mod.rs`、`src-tauri/Cargo.toml`(`[[test]]`) | T42a → T42b → T42c → T42d → T42e |
| `src-tauri/src/events.rs` | T42b だけ |
| `src-tauri/src/menu.rs`、`src-tauri/tests/app_menu.rs` | T42e だけ |
| `src/ipc/client.ts`、`src/ipc/client.test.ts` | T42a → T42b → T42c → T42d → T42e |
| `src/logic/tree/` | T41a だけ |
| `src/logic/dnd/`、`src/logic/menu/`、`src/logic/keys/` | T41b だけ |
| `src/state/`、`src/test/fakeIpc.ts`、`src/main.tsx` | T38e(`fakeIpc.ts` の写しの形だけ)→ T43a(T43b〜T43d で足すものが出たら、その順) |
| `e2e/fixtures/tauriMock.ts` | T38e(写しの形だけ)→ T43b → T43c → T43d → T44b |
| `src/ui/tree/`、`src/ui/shell/TreePane.tsx` | T43b → T43c → T43d |
| `src/ui/menu/`、`src/ui/terminals/clipboardRouting.ts` | T43c だけ |
| `src/ui/dialogs/` | T43c → T43d |
| `src/ui/terminals/TerminalHost.tsx` | T43d だけ |
| `scripts/perf/`、`scripts/perf-run.test.mjs` | T44a だけ |
| `scripts/check-boundaries.mjs` | M3 では触らない見込み(足す検査が出たら T42e の後に逐次で足す) |
| `output/design/ADR_007_finder-integration.md` | T37 → T40b → T40d → T40c → T45 |
| `output/design/ARCH_multi-terminal.md` | T45 だけ |

#### Phase 24(逐次。M2 完了後)

- [ ] T37 — M3 の最初の検証と再分解(変更ファイル: output/design/ADR_007_finder-integration.md, output/design/ADR_INDEX.md, output/tasks/PLAN_multi-terminal.md, output/tasks/PROGRESS.md | 依存: T36)
  - (a) **ゴミ箱の「戻す」**(設計書 §15.3 #3。`ARCH_multi-terminal.md:1039`): 一時ディレクトリのファイルで、`trashItemAtURL` → `NSWorkspace` の `recycleURLs` → Finder に頼む方式 の順に試す。Finder のゴミ箱に「戻す」が出るかは**人間が目で見る**。テストが入れた項目だけを片付ける。**【検証中】(司令塔)。結果は ADR-007 の (a) の欄に書く**
  - (b) アプリ内のドラッグ(設計書 §15.3 #4): 【仮定】pointer のイベントによる自前の実装にする(ADR-007 の (b))。実機の確認は T45 の人間の項目に回した
  - (c) ツリーの仮想スクロール(設計書 §9.1): 【仮定】依存を足さず、固定の行の高さで自作する(ADR-007 の (c))
  - (d) 結果を ADR-007 に書き、T38〜T45 を 1 セッション粒度に分解し直す(本節。2026-11-15)
  - 検証コードは使い捨てにする(スクラッチパッドに置き、リポジトリに入れない)

#### Phase 25a(並行可能。T37 の (b)(c)(d) の完了後。(a) の結果は待たない)

- [x] T38a — `arin-core` に M3 のポートと型、モジュールの骨格、相対パスの型を置く(変更ファイル: crates/arin-core/src/lib.rs, crates/arin-core/src/ports/mod.rs, crates/arin-core/src/ports/fs.rs, crates/arin-core/src/ports/watch.rs, crates/arin-core/src/ports/desktop.rs, crates/arin-core/src/ports/pasteboard.rs, crates/arin-core/src/paths/mod.rs, crates/arin-core/src/paths/relative.rs, crates/arin-core/src/tree/mod.rs, crates/arin-core/src/tree/types.rs, crates/arin-core/src/fileops/mod.rs, crates/arin-core/src/fileops/types.rs, crates/arin-core/src/shortcuts/mod.rs, crates/arin-core/tests/rel_path.rs, crates/arin-core/tests/generated_types.rs, src/ipc/generated/ | 依存: T37)
  - 対応: 設計書 §5.1 の `arin-core/ports`(`FileSystemPort`・`WatcherPort`・`DesktopPort`)・`tree`・`fileops`・`paths`、§5.2(ファイルはプロジェクト ID と相対パスで指す)、§7.2「画面から届く相対パス」、§3.2(機能モジュールどうしは ID 型と `paths`・`ports` だけを共有する)
  - 置くもの:
    - `RelPath`(ルート基準の相対パス。`paths/relative.rs`): 画面から届く文字列から作る。区切りは `/`、空の文字列はルート。拒否: `/` で始まるもの、`..`・`.`・空の要素(`a//b`)、末尾の `/`、NUL。親と名前に分ける・子を足す操作。JSON は文字列
    - ポートの trait: ファイルシステム(フォルダの中身=名前と種類、項目の種類=リンクをたどらない、フォルダの実パス、空のファイルを新しく作る、フォルダを新しく作る、上書きしない移動、上書きしないコピー、ゴミ箱。**消す操作と上書きする操作は置かない**)、監視(ルートに再帰の監視を張り、絶対パスの束を受け口へ渡す。返した値を捨てると止まる)、デスクトップ(Finder で表示、既定のアプリで開く)、ファイルのペーストボード(`FilePasteboardPort`: ファイルの URL を書く・読む、変更番号)。ゴミ箱の戻り値の形(ゴミ箱の中の場所が返るか)は T37 (a) の結果で変わりうるので、`Option` にしておく
    - 画面とやり取りする型(ts-rs で生成する): 項目の種類(ファイル・フォルダ・フォルダへのリンク=展開しない(設計書 §15.2 #8)・ファイルへのリンク・その他)、項目(名前・種類)、フォルダの中身の結果(中身 / 見つからない / フォルダでない / ルートの外)、ツリーの変更(プロジェクト ID・変わったフォルダの相対パス・全部読み直し)、ファイル操作の結果(済み(選ぶ項目の相対パス)/ 見つからない / 衝突(名前)/ 名前が不正(理由)/ 対象にできない(ルートの外・ルート自体・自分の子孫)/ 失敗(文言))、衝突の対処(置き換える・両方残す)、パスの種類(相対・絶対)
    - `tree`・`fileops`・`shortcuts` は子モジュールの宣言と空のファイルだけ置く。後のタスクは `lib.rs` と各 `mod.rs` を触らずに済む
  - 安全: 型の段階で絶対パスを受けない(画面から届く経路は `RelPath` だけ)。ポートに削除と上書きの操作を置かない
  - 並行: 可(T41a・T41b とファイルが重ならない)。Phase 25b・26a の全タスクの前提
  - 先に書くテスト: `相対パスはスラッシュ区切りの名前の並びとして読める`、`空の文字列はルートを指す`、`スラッシュで始まるものは相対パスにできない`、`ドットドットとドットの要素は拒否する`、`空の要素と末尾のスラッシュは拒否する`、`nul_を含むものは拒否する`、`チルダで始まる名前はただの名前として読む`、`相対パスの_json_は文字列`、`親と名前に分けられ_ルートの親は無い`、`子を足しても検査を通る形のまま`。`generated_types.rs` に足した型を加える(生成し直して差分が出ない)
  - 検証: `npm test && npm run lint`
- [x] T41a — 画面の純粋ロジック: ツリーの行、差分、保留、仮想スクロールの窓、名前の選択範囲(変更ファイル: src/logic/tree/ | 依存: T37)
  - 対応: FR-013(展開時の読み込み)、FR-014(変更の反映)、FR-026(拡張子を除いた選択)、FR-037(更新の保留)、NFR-005(見える行だけ)、設計書 §5.1 の `src/logic/tree`(`TreeModel`・`applyDirListing`・`UpdateGate`)、§7.1「外部でのファイル変更」、ADR-007 の (c)、【仮定】4・6・10・18・19
  - 置くもの: ツリーの表示モデル(読み込み済みのフォルダの中身、展開の集合、選択 1 つ、平らにした行の列=深さ・種類・開閉・読み込み中)。展開すると「読み込みが要る」を返す。折りたたんでも子の展開は覚えておき、もう一度開くと戻す。すべて折りたたむ。中身の差分の適用(消えたフォルダの子孫の中身・展開・選択を外す)。変更の受け取り(読み込み済みなら再読込に積む、未読込なら無視。目印は M6)。`UpdateGate`(保留の理由が 1 つでも残れば積むだけ、全部解けたら積んだフォルダを一度に返す)。仮想スクロールの窓(`scrollTop`・見える高さ・行の高さ・行の数・余分に描く数 → 描く範囲と上下の余白。選択した行を見える位置へ動かす量)。名称変更の選択範囲
  - `src/logic` は他の `src/*` を import できないので、生成した型は使わず、必要な値だけの型をここで定める
  - 並行: 可(`src/logic/tree/` だけを触る)
  - 先に書くテスト(Vitest): `ルートを読み込むと直下の行が深さ_0_で並ぶ`、`フォルダを展開すると読み込みが要ると返し_読み込むと子の行が続く`(FR-013)、`折りたたむと子の行が消え_もう一度開くと前の展開も戻る`、`すべて折りたたむとルートの直下だけになる`、`中身の差分を当てると追加と削除だけが変わり選択は残る`、`選択していた項目が消えたら選択を外す`、`消えたフォルダの子孫の読み込み済みの中身を捨てる`、`読み込み済みのフォルダの変更は再読込に積む`、`読み込んでいないフォルダの変更は無視する`、`全部読み直しの印で読み込み済みのフォルダをすべて積む`、`保留中は変更を積むだけで行が変わらない`(AC-T25)、`ドラッグ_名前の入力_メニュー_確認のどれかが残っていれば保留が続く`、`保留が解けると_100_件の追加で変わったフォルダをまとめて_1_回で返す`(AC-T25)、`見える範囲は_scrollTop_と高さから決まり上下に余分を描く`、`10_万行でも計算は描く行の数だけで行の配列を作らない`、`端では範囲が行の数を超えない`、`選択した行が見えるように動かす量`、`名称変更の選択範囲は最後のドットの前まで`(`a.ts`→`a`、`a.tar.gz`→`a.tar`)、`ドットで始まる名前と拡張子の無い名前とフォルダは全体を選ぶ`(`.env`・`Makefile`)
  - 検証: `npm test && npm run lint && npm run test:coverage`(`src/logic` の行 80%)
- [x] T41b — 画面の純粋ロジック: ドラッグ、ツリーのメニュー、キーの振り分け(変更ファイル: src/logic/dnd/, src/logic/menu/treeMenu.ts, src/logic/menu/treeMenu.test.ts, src/logic/keys/ | 依存: T37)
  - 対応: FR-016・FR-029(ドラッグ)、FR-024(メニューの項目とショートカット)、FR-025(作成先)、FR-035(ルートで無効)、FR-037(ドラッグ中の印)、C-04(フォーカス位置での使い分け)、【要件の追加】(表から引く)、設計書 §5.1 の `src/logic/dnd`・`src/logic/menu`・`src/logic/keys`、ADR-007 の (b)、【仮定】1・7・8・12・16・21・22
  - 置くもの:
    - ドラッグの状態(`DragMachine`): 押した → しきい値(4px【仮定】)を超えたらドラッグ → 離した / Esc で取り消し。ドロップ先の判定(`resolveDropTarget`): フォルダの行はそのフォルダ、ファイルの行はその親、ヘッダはルート、ターミナルの置き場所は挿入、自分自身と子孫は不可、同じ親のフォルダは何もしない。離したときの ⌥ でコピー
    - `buildTreeMenu`: ファイル・フォルダ・ルート(ヘッダ)・余白のそれぞれの項目と有効・無効(PRD §6.4 の Must の項目だけ。【仮定】7)。ルートでは名称変更・切り取り・ゴミ箱を無効(AC-T23)。タブが無ければ「パスをターミナルに挿入」を無効。各項目にショートカットの表のキーの表示を付ける。新規作成の作成先(フォルダならその中、ファイルなら同じ階層、余白ならルート)
    - キー: 画面が受け取るショートカットの表(コマンドの ID → キーと受け口。T38d の形に合わせ、型はここで定める)と押されたキーとフォーカス位置(ツリー・ターミナル・名前の入力中・その他)から、コマンドの ID を返す関数。⌥ を含むキーは `code` で比べる(【仮定】22)。`terminalKeyOwner` は、受け口がメニューの項目・「編集」の項目のキーを表から集めて `app` にする形に替える(M2 の振る舞いは変えない。直書きの `COMMAND_KEYS`・`COMMAND_SHIFT_KEYS` を消す)。`routeClipboard` にフォーカス位置「ツリー」と種類「切り取り」を足す(ツリーではコピー・切り取り・ペーストがツリーの操作になる)
  - 並行: 可(`src/logic/dnd/`・`src/logic/menu/treeMenu.ts`・`src/logic/keys/` だけ。T41a とは別のフォルダ)
  - 先に書くテスト(Vitest): `しきい値より小さい動きはクリックのまま`、`しきい値を超えるとドラッグになる`、`フォルダの行に落とすとそのフォルダへ`、`ファイルの行に落とすとその親のフォルダへ`、`ヘッダに落とすとルートへ`、`自分自身と子孫には落とせない`、`同じ親のフォルダに落とすと何もしない`、`option_を押して離すとコピー`、`ターミナルの置き場所に落とすと挿入`、`esc_で取り消す`、`ファイルの上のメニューは_prd_の表の_must_の項目がこの順`、`フォルダの上のメニューも同じ`、`余白のメニューは新規ファイル_新規フォルダ_ペースト_finder_で表示_ルートの絶対パスをコピー_隠しファイルを表示_すべて折りたたむ`(FR-024)、`ルートでは名称変更_切り取り_ゴミ箱が無効`(AC-T23)、`タブが無いとパスをターミナルに挿入が無効`、`メニューの項目に表のキーが出る`、`新規作成の作成先はフォルダならその中_ファイルなら同じ階層_余白ならルート`(FR-025)、`ツリーにフォーカスがあると表のキーをコマンドの_id_にする`(表の全行)、`ターミナルにフォーカスがあるとツリーのキーは何もしない`、`名前の入力中はツリーのキーを受けない`、`option_を含むキーは押した文字が変わっても一致する`、`表から作ったターミナルのキーの受け手は_m2_と同じ`(既存の `keyOwner.test.ts` がそのまま通る)、`ツリーにフォーカスがあるとコピー_切り取り_ペーストはツリーの操作になる`
  - 検証: `npm test && npm run lint && npm run test:coverage`

#### Phase 25b(並行可能。T38a 完了後)

- [x] T38b — パスの整形とシェルエスケープ(`arin-core/paths`)(変更ファイル: crates/arin-core/src/paths/format.rs, crates/arin-core/src/paths/shell.rs, crates/arin-core/tests/path_format.rs, crates/arin-core/tests/shell_escape.rs | 依存: T38a)
  - 対応: FR-015、FR-016、FR-024(挿入)、設計書 §5.1 の `PathFormatter`・`ShellEscaper`、§7.1「パスをターミナルへ」、§12「シェルへの注入」、§15.2 #6、【仮定】16・17
  - 置くもの: 相対パスの文字列(先頭に `./` を付けない。フォルダの末尾に `/` を付けない)、絶対パス(ルートと相対パスをつなぐ)、挿入の文字列(同じプロジェクトなら相対・違うプロジェクトなら絶対、エスケープして末尾に半角スペース 1 つ。Enter を含めない)
  - 安全: 挿入の文字列に改行(`\n`・`\r`)のバイトが入らないことをテストで保証する
  - 並行: 可(`paths/format.rs`・`paths/shell.rs` だけ)
  - 先に書くテスト: `相対パスはルート基準でスラッシュ区切り`(AC-T03 の `src/a b.ts`)、`絶対パスはルートと相対パスをつなぐ`(`/Users/x/dev/app/src/a b.ts`)、`空白はバックスラッシュでエスケープし末尾に半角スペースを付ける`(AC-T04 の `src/a\ b.ts `)、`シェルの特殊文字をすべてエスケープする`(`'`・`"`・`\`・`$`・`` ` ``・`!`・`&`・`*`・`?`・`(`・`)`・`[`・`]`・`{`・`}`・`;`・`<`・`>`・`|`・`#`・`^` を表で)、`先頭のチルダとイコールをエスケープする`、`改行やタブを含む名前はドル引用符で包み改行のバイトを含まない`、`日本語と絵文字はそのまま`、`別のプロジェクトのターミナルへは絶対パスにする`(§15.2 #6 の A)、`挿入の文字列は_enter_を含まない`。zsh に渡して元に戻るかの往復は T44a(実 zsh)
  - 検証: `npm test && npm run test:coverage`(`arin-core` の行 80%)
- [x] T38c — 除外・並び順・監視の変更のまとめ(`arin-core/tree`)(変更ファイル: crates/arin-core/src/tree/exclusion.rs, crates/arin-core/src/tree/order.rs, crates/arin-core/src/tree/changes.rs, crates/arin-core/tests/tree_rules.rs | 依存: T38a)
  - 対応: FR-013、FR-014、FR-017、C-07、PRD C7(除外のフォルダは監視の対象外)、設計書 §5.1 の `ExclusionRule`・`ChangeBatch`、§7.1「外部でのファイル変更」、§15.2 #1・#8、【仮定】2・3・19
  - 置くもの: 除外の判定(名前の完全一致。どの階層でも)、並び順、監視で届いた絶対パスの束 → ルート基準の「変わったフォルダ」の集合(項目の親のフォルダ。除外の中は隠しファイル表示が OFF なら捨てる。ルートの外は捨てる。上限を超えたら全部読み直し)
  - 並行: 可(`tree/` の子ファイルだけ)
  - 先に書くテスト: `既定では_git_node_modules_ds_store_を隠す`(C-07)、`ほかのドットファイルは既定でも出す`(`.claude`・`.github`)、`隠しファイル表示をオンにすると_3_つも出す`、`入れ子の階層でも名前が一致すれば隠す`、`名前の一部が一致するだけなら隠さない`(`node_modules_backup`)、`フォルダが先でその中は名前の順`、`大文字小文字が違うだけの名前はバイトの順で決まる`、`フォルダへのシンボリックリンクはフォルダと同じ並びで展開しない種類`、`ファイルの変更はその親のフォルダの変更にまとまる`、`同じフォルダの変更は_1_つにまとまる`、`除外の中の変更は捨てる`、`隠しファイル表示がオンなら除外の中の変更も渡す`、`ルートの外のパスは捨てる`、`1000_件を超えたら全部読み直しの印だけにする`
  - 検証: `npm test && npm run test:coverage`
- [x] T38d — ショートカットの表(`arin-core/shortcuts`。【要件の追加(本人。2026-11-15)】)(変更ファイル: crates/arin-core/src/shortcuts/table.rs, crates/arin-core/src/shortcuts/key.rs, crates/arin-core/tests/shortcuts.rs, crates/arin-core/tests/generated_types.rs, src/ipc/generated/ | 依存: T38a)
  - 対応: 【要件の追加】(M4.5 でキーを設定ファイルで変えるための土台)、FR-024(メニューの各操作をショートカットでも)、FR-044(M2 のタブ操作)、C-04、前提 (e)、【仮定】21・22
  - 置くもの:
    - コマンドの ID(列挙。文字列は M2 の `menu.rs` の ID をそのまま使う: `terminal.new-tab`・`terminal.close-tab`・`terminal.previous-tab`・`terminal.next-tab`・`edit.select-all`・`app.quit`。M3 で足す: `edit.copy`・`edit.cut`・`edit.paste`(ターミナルとツリーで共有)、`tree.open`・`tree.reveal-in-finder`・`tree.insert-path`・`tree.copy-relative-path`・`tree.copy-absolute-path`・`tree.rename`・`tree.trash`・`tree.new-file`・`tree.new-folder`)
    - コマンドの ID → 既定のキーと受け口の表。受け口は「メニューの項目」(OS がアクセラレータで受ける)・「編集の項目」(標準の項目が画面のコピー・切り取り・ペーストのイベントになる)・「画面」(ツリーにフォーカスがあるときに画面の keydown で受ける)の 3 つ。既定のキーは PRD §6.4 と M2 のメニューのとおり(⌘T・⌘W・⌘⇧[・⌘⇧]・⌘A・⌘Q・⌘C・⌘X・⌘V・⌘O・⌥⌘R・⌘↩・⇧⌥⌘C・⌥⌘C・Enter・⌘⌫・⌘N・⇧⌘N)
    - キーの文字列は Tauri(muda)のアクセラレータの書式(`Cmd+Shift+N`)にそろえ、修飾キーの集合とキーに分けて読む(M4.5 の T64 で設定の値を検査する土台)
    - 生成: 表の型を ts-rs で生成し、既定の表を `src/ipc/generated/defaultShortcuts.json` にも書く(偽の Rust 側と Vitest が同じ値を使うため。`generated_types.rs` が差分を検査する)【仮定】
  - 並行: 可(`shortcuts/` と生成物だけ。生成物を触るのは Phase 25b ではこのタスクだけ)
  - 先に書くテスト: `すべてのコマンドの_id_が既定のキーを持つ`、`同じフォーカスで受けるキーに重なりが無い`、`既定のキーは_prd_の表と_m2_のメニューのとおり`(表で列挙)、`id_の文字列は_m2_のメニューの_id_と同じ`、`キーの文字列を修飾キーとキーに分けて読める`、`修飾キーの順が違っても同じキーとみなす`、`読めない文字列は理由つきで拒否する`、`表の_json_の形`。生成した型と既定の表の JSON が最新
  - 検証: `npm test && npm run lint && npm run test:coverage`
- [x] T39a — ファイル操作の検査(範囲・名前・重複しない名前)(`arin-core/fileops`)(変更ファイル: crates/arin-core/src/fileops/scope.rs, crates/arin-core/src/fileops/name.rs, crates/arin-core/src/fileops/unique.rs, crates/arin-core/tests/fileops_scope.rs, crates/arin-core/tests/fileops_name.rs, crates/arin-core/tests/support/fake_fs.rs | 依存: T38a)
  - 対応: FR-025・FR-026(名前の検証)、FR-030(両方残すの名前)、FR-035、設計書 §5.1 の `PathScopeGuard`・`NameValidator`・`UniqueNameGenerator`、§7.2、§15.2 #8、【仮定】11
  - 置くもの: 範囲の検査(ルートの実パスと、対象の親フォルダの実パス=ファイルシステムのポートで解決したもの、を要素ごとに比べる。文字列の前方一致にしない。対象そのものはリンクをたどらない=リンク自体を操作する)。ルート自体を名称変更・移動・切り取り・ゴミ箱の対象にしない。フォルダを自分自身・子孫へ移さない。名前の検査(空でない、`/` と NUL を含まない、`.`・`..` でない、UTF-8 で 255 バイト以内)。両方残すの名前
  - 偽のファイルシステム(`tests/support/fake_fs.rs`。メモリの中のフォルダとシンボリックリンク。**上書きや削除を求められたら panic する**)をここで作り、T39b が使う
  - 安全: 経路の検査は `arin-core` で行い、画面からは迂回できない。シンボリックリンクでの脱出をテストで確かめる
  - 並行: 可(`fileops/` の子ファイルと `tests/` だけ)
  - 先に書くテスト(偽のポート): `ルートの中の相対パスは通す`、`シンボリックリンクで親がルートの外に出るものは拒否する`、`名前の前方が同じだけの別のフォルダはルートの外`(`/x/app` と `/x/app2`)、`ルート自体の名称変更_移動_切り取り_ゴミ箱は拒否する`、`ルートへの新規作成とペーストは通す`、`フォルダを自分自身や子孫へ移すことは拒否する`、`空の名前_スラッシュ_nul_ドット_ドットドットを拒否する`、`255_バイトを超える名前を拒否する`(日本語 85 文字=255 バイトは通り、86 文字は拒否)、`両方残すの名前は空白と番号を拡張子の前に足す`、`番号は_2_から空いている最小のもの`、`拡張子の無い名前とドットで始まる名前は末尾に足す`
  - 検証: `npm test && npm run test:coverage`

#### Phase 25c(並行可能)

- [x] T38e — 隠しファイル表示の保存値と写し(変更ファイル: crates/arin-core/src/project/store.rs, crates/arin-core/src/persistence/codec.rs, crates/arin-core/src/workspace/, crates/arin-core/tests/persistence_codec.rs, crates/arin-core/tests/workspace_projects.rs, crates/arin-core/tests/app_snapshot.rs, crates/arin-core/tests/generated_types.rs, src/ipc/generated/, src/test/fakeIpc.ts, e2e/fixtures/tauriMock.ts | 依存: T38d)
  - 対応: FR-017(切替の保存)、FR-056、設計書 §6.2 の `projects[].tree.showHidden`、【仮定】3
  - 置くもの: `ProjectStore` のプロジェクトに隠しファイル表示(既定 false)、`state.json` の `projects[].tree.showHidden` の読み書き(キーの追加なので版は 1 のまま。`tree` が無い・形が違うときはそのプロジェクトを既定値にし、退避しない【仮定】)、写しの `ProjectCardSnapshot` に `showHidden`、`Workspace` に切替の操作。展開状態は足さない
  - `fakeIpc.ts` と `tauriMock.ts` は写しの新しい項目を既定値で埋めるだけ(M2 の T32b と同じ)
  - 並行: 可(T39b とファイルが重ならない)。生成物の後なので T38d の後
  - 先に書くテスト: `隠しファイル表示の既定はオフ`、`隠しファイル表示を書いて読むと同じに戻る`、`tree_キーの無い状態ファイルも読める`、`tree_の形が違えばそのプロジェクトだけ既定値で退避しない`、`隠しファイル表示を変えると写しが変わる`、`写しの_json_に_showHidden_が載る`、`保存する内容に展開状態を含めない`。生成した型が最新
  - 検証: `npm test && npm run lint && npm run build && npm run e2e`(既存の画面の自動テストが新しい写しの形で通る)
- [x] T39b — ファイル操作の手順(`FileOperationService`)、衝突、切り取り(変更ファイル: crates/arin-core/src/fileops/service.rs, crates/arin-core/src/fileops/clipboard.rs, crates/arin-core/tests/fileops_service.rs, crates/arin-core/tests/support/fake_fs.rs | 依存: T39a)
  - 対応: FR-025〜030、FR-035、FR-036、設計書 §5.1 の `FileOperationService`・`ConflictResolution`・`FileClipboard`、§5.2(衝突は結果で返し、対処を付けて呼び直す)、§7.1「ファイル操作」、§9.1(切り取り)、【仮定】12〜15。取り消しの履歴(`FileOperationHistory`)は M6(T59)
  - 操作: 新規ファイル(空)・新規フォルダ、名称変更、移動、コピー、ゴミ箱、ペースト(ペーストボードのファイルを貼り付け先へ。切り取りの状態とペーストボードの変更番号で移動かコピーかを決める)。どれも T39a の検査を先に通し、**実行の瞬間に**存在を確かめ、上書きしないポートの操作の失敗で衝突を判定する(事前の確認だけに頼らない)。置き換えるは既存をゴミ箱に入れてから置く(ゴミ箱に入れたあとにまた同名ができたら、もう一度衝突を返す)。両方残すは T39a の名前で置く
  - T38a の型を変えない(足りなければ T42c で足す。生成物の持ち回りのため)
  - 安全: 偽のポートは上書きと削除で panic するので、その経路が無いことがテストで分かる。ルートの外・ルート自体はポートを 1 回も呼ばずに拒否する
  - 並行: 可(T38e とファイルが重ならない)
  - 先に書くテスト(偽のポート): `新規ファイルは空で作られ作った相対パスを返す`(AC-T13 の規則)、`新規フォルダを作る`、`同名があれば作らず衝突を返す`、`名称変更でディスクの名前が変わる`、`名称変更の対象が実行の時点で無ければ何も変えず見つからないを返す`(AC-T24)、`メニューを開いたあとに同名ができていたら衝突を返す`(FR-036)、`移動は宛先に同名があれば何も変えず衝突を返す`(AC-T18)、`置き換えるは既存をゴミ箱に入れてから置き上書きしない`、`置き換えの途中でまた同名ができたら衝突を返す`、`両方残すは番号付きの名前で置く`、`コピーは元を残す`、`ゴミ箱に入れると対象が消えゴミ箱の操作だけが呼ばれる`、`ルートの外とルート自体への操作はポートを呼ばずに拒否する`、`切り取ってペーストボードが変わっていなければ移動になる`、`切り取ったあとにペーストボードが変わっていたらコピーになる`、`フォルダの上でペーストするとその中に置く`、`ルートの外のファイルをペーストでコピーできる`(Finder でコピーしたもの)、`ペーストボードにファイルが無ければ何もしない`、`別のボリュームへの移動は失敗を返し何も変えない`、`大文字小文字だけを変える名称変更は衝突にしない`
  - 検証: `npm test && npm run test:coverage`

#### Phase 26a(並行可能。T38a 完了後。Phase 25b・25c と同時に進めてよい)

- [x] T40a — フォルダの読み込みと、上書きしない作成・移動・コピー(`arin-platform/fs`)(変更ファイル: crates/arin-platform/src/lib.rs, crates/arin-platform/src/fs/mod.rs, crates/arin-platform/src/fs/local.rs, crates/arin-platform/tests/fs.rs | 依存: T38a)
  - 対応: FR-013、FR-025〜027、FR-029、FR-030、FR-036、設計書 §1.4 #14、§5.4「上書きしない移動・コピー」、§7.1「スレッドの分担」(読み込みはメインスレッドの外)、【仮定】14・15・24。依存は足さない(`libc` =0.2.191 の `renamex_np`・`copyfile`、標準ライブラリ)
  - 置くもの: フォルダの中身(`read_dir` の種類で判定し、シンボリックリンクだけ先をたどってフォルダか調べる)、空のファイルの新規作成(`create_new`)、フォルダの新規作成、移動(`renamex_np` の `RENAME_EXCL`。宛先が同じ項目=大文字小文字だけの変更なら普通の改名)、コピー(`copyfile` の `COPYFILE_EXCL`。フォルダは中身ごと)、フォルダの実パス
  - 安全: テストの補助は、使うパスがシステムの一時領域の下にあることを確かめてから使う。上書きしないことを実ファイルで確かめる
  - 並行: 可(`arin-platform/src/lib.rs` を触るのは M3 ではこのタスクだけ。Cargo の 3 ファイルは触らない)
  - 先に書くテスト(一時ディレクトリだけ): `フォルダの中身を名前と種類で返す`、`ファイルへのリンクとフォルダへのリンクを見分ける`、`壊れたリンクも項目として返す`、`新規作成は既にあれば失敗し中身を変えない`、`フォルダの作成は既にあれば失敗する`、`移動は宛先に同名があれば失敗しどちらも変えない`、`移動はフォルダごと動く`、`コピーは宛先に同名があれば失敗しどちらも変えない`、`フォルダのコピーは中身ごと複製される`、`大文字小文字だけを変える名称変更ができる`、`テスト用のパスが一時領域の外なら補助が拒否する`
  - 検証: `npm test && npm run lint`
- [x] T40b — ファイル監視(FSEvents。`arin-platform/watch`)(変更ファイル: Cargo.toml, Cargo.lock, crates/arin-platform/Cargo.toml, crates/arin-platform/src/watch/mod.rs, crates/arin-platform/src/watch/fsevents.rs, crates/arin-platform/tests/watch.rs, output/design/ADR_007_finder-integration.md | 依存: T38a)
  - 対応: FR-014、PRD C7、NFR-003(監視は選択中の 1 本だけ)、設計書 §1.3 #7、§5.4「ファイル監視」、前提 (d)
  - 使うクレート: `notify`。**正確な版は着手のときに `cargo search notify` と crates.io の公開日で確かめ、公開から 2 週間未満を避けて `=` で固定する**(計画と設計書の「8.2 系」は目安)。既定の feature のうち FSEvents 以外を切れるか確かめる。`git diff Cargo.lock` で増えたクレートを数え、`cargo tree -i reqwest --target aarch64-apple-darwin` が空であることと合わせて ADR-007 の追記に書く。増えるクレートが多すぎる・通信を含むと分かったら、このタスクを止め、FSEvents を `objc2` 系で直接呼ぶ案と並べて本人に選んでもらう
  - 置くもの: ルートに再帰の監視を 1 本張り、待ち時間 0.2 秒程度でまとめて絶対パスの束を受け口へ渡す。返した値を捨てると止まる。除外の判定は `arin-core`(T38c)を呼び出し側(T42b)が当てる
  - 並行: 可(T40a とファイルが重ならない。Cargo の 3 ファイルを触るのは Phase 26a ではこのタスクだけ)
  - 先に書くテスト(一時ディレクトリ。直列で、どの待ちにも上限時間): `ファイルを作ると_2_秒以内にそのパスが届く`(AC-T02 の土台)、`削除と名称変更も届く`、`監視をやめると届かなくなる`、`監視しているフォルダの外の変更は届かない`、`監視中のルートを改名しても落ちずにやめられる`
  - 検証: `npm test && npm run lint`。`cargo tree -i reqwest --target aarch64-apple-darwin` が空

#### Phase 26b(逐次。T40b 完了後。Phase 25c と同時に進めてよい)

- [x] T40d — Finder で表示、既定のアプリで開く、ファイルのペーストボード(`arin-platform/desktop`)(変更ファイル: Cargo.toml, Cargo.lock, crates/arin-platform/Cargo.toml, crates/arin-platform/src/desktop/mod.rs, crates/arin-platform/src/desktop/workspace.rs, crates/arin-platform/src/desktop/file_pasteboard.rs, crates/arin-platform/tests/desktop.rs, output/design/ADR_007_finder-integration.md | 依存: T38a, T40b)
  - 対応: FR-018(Finder で表示、開く)、FR-027(Finder と互換のコピー・ペースト)、設計書 §1.4 #19、§5.4、§9.1(ツリーでコピーした項目はファイルの URL としてペーストボードに書く)
  - 使うクレート: `objc2-app-kit` =0.3.2(版は変えない)の `NSWorkspace`(`activateFileViewerSelectingURLs`・`openURL`)と `NSPasteboard` のファイルの URL。足す feature を Tauri がすでに有効にしているか `cargo tree -e features -i objc2-app-kit --target aarch64-apple-darwin` で確かめ、ADR-007 の追記に書く
  - 安全: テストはユーザーのクリップボード(一般のペーストボード)に触れず、名前付きのペーストボードを使って終わったら解放する(M2 の T31c と同じ)。Finder と既定のアプリを実際に開く確認は自動テストにしない(T45 の人間)
  - 並行: 不可(Cargo の 3 ファイルを T40b・T40c と順に触る)。Phase 25c とはファイルが重ならない
  - 先に書くテスト: `名前付きのペーストボードにファイルの_url_を書いて読み戻せる`、`複数のファイルの_url_を順のまま読み戻せる`、`文字列だけのときはファイルが無いと返す`、`書くと変更番号が変わる`、`無いパスの_finder_で表示と開くは失敗を返す`
  - 検証: `npm test && npm run lint`。`cargo tree -i reqwest --target aarch64-apple-darwin` が空

#### Phase 26c(逐次。T37 の (a) の結果と、T40a・T40d の完了後)

- [x] T40c — ゴミ箱(`arin-platform/fs`)(変更ファイル: Cargo.toml, Cargo.lock, crates/arin-platform/Cargo.toml, crates/arin-platform/src/fs/mod.rs, crates/arin-platform/src/fs/trash.rs, crates/arin-platform/tests/trash.rs, output/design/ADR_007_finder-integration.md | 依存: T37 の (a), T40a, T40d)
  - **T37 の (a) の結果(ADR-007 の (a) の欄)に従う。結果が「【検証中】」のままなら始めない。** どの方式も「戻す」が出ないと分かったら、このタスクを止めて本人に方式を確かめる(FR-028 の AC が変わりうる)
  - 対応: FR-028、FR-030(置き換える=既存をゴミ箱へ)、設計書 §1.3 #11、§5.4「ゴミ箱と Finder の「戻す」」、§15.3 #3
  - 使うクレート: (a) で採った方式による(方式 1 は `objc2-foundation` の `NSFileManager`、方式 2 は `objc2-app-kit` の `NSWorkspace`、方式 3 は Apple Events)。版は変えず feature だけ足し、ADR-007 の追記に書く。方式 3 ならエンタイトルメント(設計書 §15.3 #2)を M5 の T51 に申し送る
  - 安全: 本物のゴミ箱を使うテストは `#[ignore]`(【仮定】23)で、テストが入れた項目だけを片付ける。片付けに失敗したら残った名前を標準エラーに出して失敗にする。ゴミ箱に入れられなかったときは、別の方式や削除に切り替えない
  - 並行: 不可(Cargo の 3 ファイルと `fs/mod.rs` を順に触る)
  - 先に書くテスト(一時ディレクトリ。`#[ignore]`): `ゴミ箱に入れると元の場所から消えゴミ箱の中にある`、`フォルダもゴミ箱に入る`、`無いものをゴミ箱に入れると見つからないを返す`、`テストは自分が入れた項目だけを片付ける`。(方式 1・2 で)`ゴミ箱の中の場所が返る`
  - 検証: `npm test && npm run lint && cargo test -p arin-platform --test trash -- --ignored`(終わったあとにテストの項目がゴミ箱に残っていない)

#### Phase 26d(並行可能。T40c 完了後。Phase 27・28a と同時に進めてよい)

- [x] T44a — 実ファイルの同時操作、シェルへの往復、NFR-005 の計測(変更ファイル: crates/arin-platform/tests/fs_race.rs, crates/arin-platform/tests/shell_roundtrip.rs, crates/arin-platform/tests/perf_tree.rs, scripts/perf/, scripts/perf-run.test.mjs | 依存: T38b, T39b, T40c)
  - 対応: FR-036(実ファイルでの同時操作)、FR-030(上書きしない)、FR-016(エスケープの往復)、NFR-005(10 万ファイルの見本で展開の時間を記録。合否にしない)、PRD §8「同時操作」
  - 計測: 一時フォルダに見本を 2 つ作る(1 つのフォルダに 10 万ファイル、1,000 フォルダ × 100 ファイル)。フォルダの読み込み・除外・並べ替えの時間を記録する(`#[ignore]`、`npm run perf` が動かす)。画面側の時間は T44b
  - 安全: 一時フォルダの中だけ。ゴミ箱は偽のポート(本物は T40c のテストだけ)
  - 並行: 可(`crates/arin-platform/tests/` と `scripts/perf/` だけ。Phase 27・28a とファイルが重ならない)
  - 先に書くテスト(実ファイル・実 zsh。直列、上限時間): `名称変更の直前に外から消すと何も変わらず見つからない`、`移動の直前に外から同名を作ると衝突になり両方残る`、`コピーの直前に宛先に同名ができても上書きしない`、`エスケープした挿入の文字列を_zsh_に渡すと元の名前に戻る`(T38b の表の名前すべてと、改行を含む名前。`HOME` は一時フォルダ、`/bin/zsh` 固定)、`10_万ファイルの見本でフォルダの読み込みの時間を記録する`(`#[ignore]`)
  - 検証: `npm test && npm run perf`

#### Phase 27(逐次。Phase 25・26 完了後(T44a を除く)。`src-tauri/src/lib.rs` と `src/ipc/` を順に触る)

- [x] T42a — ツリーのコマンドと隠しファイル表示(変更ファイル: src-tauri/src/tree.rs, src-tauri/src/commands/tree.rs, src-tauri/src/commands/mod.rs, src-tauri/src/lib.rs, src-tauri/Cargo.toml, src-tauri/tests/tree_commands.rs, src/ipc/client.ts, src/ipc/client.test.ts | 依存: T38c, T38e, T39a, T40a)
  - 対応: FR-013、FR-017、FR-035、設計書 §5.2(ツリーの中身は写しに含めず、展開したときに問い合わせる)、§7.1「スレッドの分担」(読み込みはコマンドの非同期タスク)
  - コマンド: `list_directory`(プロジェクト ID と相対パス。除外と並び順を当てて返す。ルートが見つからなければ「見つからない」)、`set_show_hidden`(写しが変わり保存が予約される)
  - 安全: 相対パスは `RelPath` で受け、T39a の範囲の検査を通してから読む。絶対パスを受け取らず返さない
  - 並行: 不可
  - 先に書くテスト(模擬ランタイム、一時フォルダ。`harness = false` の `[[test]]` を足す): `ルートの中身が除外と並び順のとおりに返る`、`隠しファイル表示をオンにすると_3_つも返り写しと状態ファイルに反映される`、`ドットドットや絶対パスを渡すと拒否し何も読まない`、`シンボリックリンクでルートの外を指すフォルダは中身を返さない`、`見つからないプロジェクトは見つからないを返す`、`ファイルを指すとフォルダでないを返す`。Vitest: 新しいコマンドの包みが正しい名前と引数で届く
  - 検証: `npm test && npm run lint && npm run app && npm run smoke`
- [x] T42b — 監視の張り替えと `tree-changed` イベント(変更ファイル: src-tauri/src/tree_watch.rs, src-tauri/src/events.rs, src-tauri/src/lib.rs, src-tauri/Cargo.toml, src-tauri/tests/tree_watch.rs, src/ipc/client.ts, src/ipc/client.test.ts | 依存: T38c, T40b, T42a)
  - 対応: FR-014、FR-002 のツリーの部分と FR-007 の「ツリーも表示され続ける」(M2 から回した分)、設計書 §1.3 #7、§7.1「プロジェクトの切替」「ルートフォルダへの追従」「外部でのファイル変更」
  - 張り替え: 写しを送るたびに、選択中のプロジェクト・そのルートのパス・見つからない・隠しファイル表示を前回と比べ、変わっていれば張り替える(プロジェクトの切替、再リンク、見回りでのルートの追従、見つからない、削除、0 件)。`patrol.rs` は変えない。張り替えの入出力は `Workspace` のロックの外。検証画面では張らない
  - 送るもの: 届いたパスを T38c の規則でまとめ、`tree-changed`(プロジェクト ID・変わったフォルダの相対パス・全部読み直し)で送る。絶対パスは送らない
  - 並行: 不可
  - 先に書くテスト(偽の監視のポート。実物の一時フォルダで 1 件): `選択中のプロジェクトのルートにだけ監視を張る`、`プロジェクトを切り替えると前の監視をやめ新しいルートに張る`、`ルートが改名に追従すると新しいパスに張り替える`、`見つからないになると監視をやめ見つかると張り直す`、`除外の中の変更は送らない`、`変わったフォルダを相対パスでまとめて送る`、`実物の一時フォルダでファイルを作ると_2_秒以内に_tree-changed_が届く`(AC-T02 の Rust 側)、`検証画面では監視を張らない`
  - 検証: `npm test && npm run lint && npm run app && npm run smoke`。`node scripts/smoke.mjs --env=ARIN_VERIFY_GRID=1`
- [x] T42c — ファイル操作のコマンド(変更ファイル: src-tauri/src/fileops.rs, src-tauri/src/commands/fileops.rs, src-tauri/src/commands/mod.rs, src-tauri/src/lib.rs, src-tauri/Cargo.toml, src-tauri/tests/fileops_commands.rs, crates/arin-core/tests/generated_types.rs, src/ipc/generated/(型を足すときだけ), src/ipc/client.ts, src/ipc/client.test.ts | 依存: T39b, T40c, T40d, T42b)
  - 対応: FR-025〜030、FR-035、FR-036、設計書 §5.2(衝突は結果で返し、対処を付けて呼び直す)、§7.1「ファイル操作」「スレッドの分担」、【仮定】12〜15・20
  - コマンド: `create_entry`(作成先のフォルダ・名前・ファイルかフォルダか)、`rename_entry`、`move_entry`・`copy_entry`(ツリー内のドロップ。宛先のフォルダ)、`trash_entry`、`copy_entries`(ファイルの URL をペーストボードへ)、`cut_entries`、`paste_entries`(貼り付け先のフォルダ)。衝突の対処は任意の引数で受ける。どれもプロジェクト ID と相対パスで受け、T39b の手順を待ってよいスレッドで呼ぶ
  - 安全: 絶対パスを受け取らない。範囲の検査は `arin-core` の手順の中で行い、コマンドは素通しする。コマンドの結合テストはゴミ箱を偽のポートにする(【仮定】23)
  - 並行: 不可
  - 先に書くテスト(模擬ランタイム、一時フォルダ、偽のゴミ箱、名前付きのペーストボード): `新規ファイルで空のファイルができ作った相対パスが返る`(AC-T13 の Rust 側)、`名称変更でディスクの名前が変わる`、`実行の時点で消えていた対象の名称変更は何も変えず見つからないを返す`(AC-T24)、`宛先に同名があると衝突を返し両方の中身が変わらない`(AC-T18)、`置き換えるを付けて呼び直すと既存がゴミ箱に渡ってから置かれる`、`両方残すを付けて呼び直すと番号付きの名前で置かれる`、`ゴミ箱はゴミ箱の操作だけを呼ぶ`、`ルート自体とルートの外は拒否し何も変えない`、`ドットドットと絶対パスとシンボリックリンクでの脱出を拒否する`、`ツリーでコピーするとペーストボードにファイルの_url_が載る`、`ペーストボードのファイルをフォルダへペーストできる`、`切り取ってペーストすると元の場所から無くなる`、`どの操作の前後でも一時フォルダの外が変わらない`。Vitest: 新しいコマンドの包み
  - 検証: `npm test && npm run lint && npm run app && npm run smoke`
- [x] T42d — デスクトップとパスのコマンド(Finder で表示、開く、パスのコピー、ターミナルへの挿入)(変更ファイル: src-tauri/src/commands/desktop.rs, src-tauri/src/commands/mod.rs, src-tauri/src/lib.rs, src-tauri/Cargo.toml, src-tauri/tests/desktop_commands.rs, src/ipc/client.ts, src/ipc/client.test.ts | 依存: T38b, T42c)
  - 対応: FR-015、FR-016、FR-018、FR-024(パスをターミナルに挿入)、設計書 §7.1「パスをターミナルへ」、§12、【仮定】16
  - コマンド: `reveal_entry`、`open_entry`(ファイルだけ。フォルダは画面が開閉する)、`copy_entry_path`(相対か絶対か。Rust がペーストボードに文字列を書く。ルートの絶対パスは相対パスを空にして呼ぶ)、`insert_entry_path`(宛先のターミナル ID。T38b の規則で作った文字列を PTY に書く。Enter は送らない。ターミナルが動いていなければ何もしない)
  - 安全: 絶対パスは画面へ返さない。挿入は必ず T38b のエスケープを通す
  - 並行: 不可
  - 先に書くテスト(模擬ランタイム、偽 PTY、名前付きのペーストボード、偽のデスクトップのポート): `相対パスのコピーで_src/a_b.ts_がペーストボードに入る`(AC-T03)、`絶対パスのコピーでルートとつないだパスが入る`、`ルートの絶対パスをコピーできる`、`挿入で偽_pty_に_src/a\_b.ts_と空白が書かれ改行が書かれない`(AC-T04 の Rust 側)、`別のプロジェクトのターミナルへは絶対パスが書かれる`、`finder_で表示と開くは偽のポートに正しいパスで届く`、`ルートの外とドットドットは拒否する`。Vitest: 新しいコマンドの包み
  - 検証: `npm test && npm run lint && npm run app && npm run smoke`
- [x] T42e — メニューバーとショートカットの表(変更ファイル: src-tauri/src/menu.rs, src-tauri/src/commands/app.rs, src-tauri/src/lib.rs, src-tauri/tests/app_menu.rs, src/ipc/client.ts, src/ipc/client.test.ts | 依存: T38d, T42d)
  - 対応: 【要件の追加】、FR-024(ショートカット)、FR-027(切り取り)、FR-044(M2 のタブ操作を壊さない)、C-04、前提 (e)、【仮定】21
  - 変えるもの: `menu.rs` の項目のアクセラレータを T38d の表から作る(ID とキーの文字列を `menu.rs` に直書きしない。文言は `menu.rs` に残す)。表の受け口が「メニューの項目」のものだけをメニューに載せる。「編集」に標準の「切り取り」(⌘X)を足す(画面の `cut` イベントになる)。`get_shortcuts` で表を画面へ渡す(起動時に 1 回。M4.5 で設定の値を混ぜる口)
  - 並行: 不可
  - 先に書くテスト(模擬ランタイム): `メニューの項目のアクセラレータが表の既定のキーと同じ`、`受け口がメニューの項目のものだけがメニューに載る`、`編集に切り取り_コピー_ペースト_すべてを選択がこの順である`、`get_shortcuts_が既定の表を返す`。既存の `app_menu.rs` のテストがそのまま通る(タブ操作・終了・すべてを選択)
  - 検証: `npm test && npm run lint && npm run app && npm run smoke`

#### Phase 28a(逐次。T42e と T41a・T41b の完了後)

- [x] T43a — 画面のツリーのストアと操作(変更ファイル: src/state/treeStore.ts, src/state/treeStore.test.ts, src/state/actions.ts, src/state/actions.test.ts, src/state/uiStore.ts, src/state/uiStore.test.ts, src/state/selectors.ts, src/state/startApp.ts, src/state/startApp.test.ts, src/test/fakeIpc.ts, src/main.tsx | 依存: T41a, T41b, T42e)
  - 対応: FR-013、FR-014、FR-025〜030、FR-036、FR-037、設計書 §6.1 の `treeStore`・`uiStore`、§6.3、§7.1「外部でのファイル変更」、【仮定】4・12・18・20
  - 置くもの: `treeStore`(プロジェクトごとの T41a の表示モデルと保留。生成関数で作る)。`tree-changed` の購読と再読込、保留が解けたときのまとめての再読込。操作の結果の扱い(見つからない=1 行の知らせを出して親を読み直す、衝突=衝突の確認を開いて選んだ対処で呼び直す、済み=関わるフォルダを読み直して作った項目を選ぶ)。`uiStore` に、ツリーのメニュー、インライン編集(新規作成・名称変更)、衝突の確認、移動の確認、ツリーの知らせ、ツリーにフォーカスがあるか、を足す。起動時に `get_shortcuts` で表を受け取る
  - 並行: 不可
  - 先に書くテスト(Vitest。偽の通信): `選択したプロジェクトのルートを読み込む`、`展開でフォルダの中身を求め届いたら行が増える`、`tree-changed_で読み込み済みのフォルダを読み直す`、`保留中は読み直さず解けたらまとめて読み直す`、`名称変更が見つからないを返したら知らせを出し親を読み直す`(AC-T24)、`衝突を返したら衝突の確認を開き選んだ対処で呼び直す`、`衝突の確認で中止なら呼び直さない`、`新規作成が済んだら作った項目を選ぶ`(AC-T13)、`ドロップでの移動は確認を開き確認すると移動のコマンドを呼ぶ`(FR-029)、`option_のドロップは確認なしでコピーのコマンドを呼ぶ`、`プロジェクトを切り替えて戻ると読み込み済みのツリーを保ち読み直す`、`隠しファイル表示の切替でコマンドを呼び読み直す`、`起動時にショートカットの表を受け取る`
  - 検証: `npm test && npm run lint && npm run build`
- [x] T43b — ツリーの表示(仮想スクロール、開閉、ルートの行、監視の反映)(変更ファイル: src/ui/tree/FileTreeView.tsx, src/ui/tree/TreeRow.tsx, src/ui/tree/TreeHeader.tsx, src/ui/tree/TreeView.css, src/ui/tree/icons.tsx, src/ui/shell/TreePane.tsx, src/ui/design/tokens.css, e2e/fixtures/tauriMock.ts, e2e/tree.spec.ts | 依存: T43a)
  - 対応: FR-013、FR-014、FR-017(切替)、FR-018(ダブルクリックで開く)、FR-060(ツリーのヘッダの着色を保つ)、FR-002 のツリーの部分と FR-007 の「ツリーも表示され続ける」(M2 から回した分)、NFR-005(見える行だけを描く)、設計書 §9.1、§15.2 #12(アイコンは自作の少数の SVG)、ADR-007 の (c)、【仮定】1・5・6
  - 作るもの: `FileTreeView`(コンテナ。T41a の窓で描く範囲を決め、上下の余白と見える行だけを描く)、`TreeRow`(字下げ・開閉の印・アイコン・名前。名前は文字として差し込む)、`TreeHeader` をルートの行として選べるようにする(色は M2 のまま)。偽の Rust 側にツリーのコマンドと `tree-changed` を足す
  - 安全: ファイル名は信頼しない文字列として扱い、HTML として解釈させない(設計書 §12。境界検査が `dangerouslySetInnerHTML` を禁止している)
  - 並行: 不可
  - 先に書くテスト(Playwright。偽の Rust 側): `選択中のプロジェクトのルートの直下が並ぶ`(FR-013)、`フォルダを展開すると中身を求めるコマンドが呼ばれ子が並ぶ`(FR-013)、`tree-changed_で_src_に_new.txt_が現れる`(AC-T02 の画面側)、`削除と改名も反映される`(FR-014)、`隠しファイルを表示の切替でコマンドが呼ばれ返った項目が出る`、`ファイルをダブルクリックすると開くのコマンドが呼ばれる`、`フォルダをダブルクリックすると開閉する`、`1_万行のフォルダを展開しても描かれる行は見える数だけ`、`スクロールすると見える行が入れ替わる`、`html_のような名前もそのまま文字で出る`、`ヘッダを選ぶとルートが選ばれる`、`プロジェクトを切り替えるとツリーが切り替わる`(FR-002)、`ルートの表示用のパスが変わってもツリーの行が残る`(AC-P07)、`フォルダへのリンクには開閉の印が無い`、`見つからないプロジェクトはツリーの代わりに案内が出る`(M2 の既存)。`e2e/verify-grid.spec.ts` がそのまま通る
  - 人間が見る範囲(T45): 行の高さ・字下げ・アイコンの見た目
  - 検証: `npm test && npm run lint && npm run build && npm run e2e`
- [x] T43c — コンテキストメニュー、ショートカット、インライン編集、衝突の確認(変更ファイル: src/ui/menu/ContextMenu.tsx, src/ui/menu/ContextMenu.css, src/ui/menu/runTreeMenuCommand.ts, src/ui/menu/runTreeMenuCommand.test.ts, src/ui/tree/, src/ui/dialogs/ConflictDialog.tsx, src/ui/terminals/clipboardRouting.ts, src/ui/terminals/clipboardRouting.test.ts, e2e/fixtures/tauriMock.ts, e2e/tree-ops.spec.ts | 依存: T43b)
  - 対応: FR-015、FR-018、FR-024〜028、FR-030、FR-035、FR-036、FR-037(メニュー・入力中)、【要件の追加】(キーは表から引く)、C-04、【仮定】7〜10・21
  - 作るもの: `ContextMenu` に無効の項目とキーの表示を足す(カードのメニューは変わらない)。ツリーの右クリック(ファイル・フォルダ・ヘッダ・余白)でT41b の項目を出す。ツリーにフォーカスがあるときの keydown を T41b の関数で振り分ける(キーの文字列を部品に書かない)。コピー・切り取り・ペーストのイベントの入口(`clipboardRouting.ts`)にツリーを足す(ツリーにフォーカスがあればツリーの操作にし、ターミナルに貼り付けない)。インライン編集(`InlineNameInput`。T41a の選択範囲)。衝突の確認(置き換える・両方残す・中止)
  - 並行: 不可
  - 先に書くテスト(Playwright): `ファイルの上で右クリックすると表の項目が出る`(FR-024)、`余白の右クリックで_7_項目が出る`、`ルートでは名称変更_切り取り_ゴミ箱が無効`(AC-T23)、`新規ファイルで_src_に入力欄が出て_a.ts_と_enter_で作成のコマンドが呼ばれ作った行が選ばれる`(AC-T13)、`ファイルの上の新規ファイルは同じ階層_余白ではルート`(FR-025)、`esc_で取り消すと何も呼ばない`、`名称変更は拡張子を除いた部分が選ばれた入力欄になる`(FR-026)、`確定すると名称変更のコマンドが呼ばれ新しい名前が出る`、`相対パスをコピーと絶対パスをコピーで正しいコマンドが呼ばれる`(FR-015)、`cmd_backspace_でゴミ箱のコマンドが呼ばれ行が消える`(AC-T16 の画面側)、`見つからないが返ると見つかりませんと出て行が消える`(AC-T24)、`衝突が返ると置き換える_両方残す_中止が出て中止では何も呼ばない`(AC-T18)、`メニューの表示中と入力中は届いた変更で行が動かない`(FR-037)、`ツリーにフォーカスがあるとコピー_切り取り_ペーストのイベントがツリーの操作になりターミナルに貼り付かない`、`ターミナルにフォーカスがあるときのコピーとペーストは今までどおり`(`e2e/terminal.spec.ts` が通る)、`表の画面で受けるキーでメニューと同じ操作が呼ばれる`(FR-024。メニューの項目で受けるキーは OS が受けるので自動テストでは扱わない)、`パスをターミナルに挿入で表示中のターミナルへの挿入が呼ばれる`、`finder_で表示のコマンドが呼ばれる`
  - 人間が見る範囲(T45): 実際のキー(US と JIS)、メニューの見た目
  - 検証: `npm test && npm run lint && npm run build && npm run e2e`
- [x] T43d — ドラッグ(ツリー内の移動・コピー、ターミナルへの挿入)(変更ファイル: src/ui/tree/, src/ui/terminals/TerminalHost.tsx, src/ui/dialogs/MoveConfirmDialog.tsx, e2e/fixtures/tauriMock.ts, e2e/tree-dnd.spec.ts | 依存: T43c)
  - 対応: FR-016、FR-029、FR-030、FR-037、ADR-007 の (b)、【仮定】12・16
  - 作るもの: 行の pointer のイベントを T41b の `DragMachine` につなぐ(押した行に `setPointerCapture`、離した位置の要素からドロップ先を決める)。ドラッグ中は保留の理由を立てる。ドロップ先の目印の `data-` 属性をフォルダの行・ヘッダ・ターミナルの置き場所(`TerminalHost`。xterm.js の要素には手を入れない)に付ける。移動の確認ダイアログ
  - 並行: 不可
  - 先に書くテスト(Playwright。`page.mouse` の down・move・up): `ツリーの項目をターミナルへ落とすと挿入のコマンドが呼ばれる`(AC-T04 の画面側)、`ほかのフォルダへ落とすと移動の確認が出て中止なら何も呼ばない`(FR-029)、`確認すると移動のコマンドが呼ばれる`、`option_を押して落とすとコピーのコマンドが呼ばれる`、`ドラッグ中に_100_件の変更が届いても行が動かず落としたあとに反映される`(AC-T25)、`落とした先に同名があると衝突の確認が出る`(AC-T18 の経路)、`自分の子孫へは落とせない`、`esc_で取り消す`、`小さな動きはクリックのまま`
  - 人間が見る範囲(T45): 実際のマウスとトラックパッドでの手触り(WKWebView)
  - 検証: `npm test && npm run lint && npm run build && npm run e2e`

#### Phase 28b(逐次。T43d 完了後)

- [x] T44b — M3 のシナリオの画面の自動テストと、画面側の展開の時間の記録(変更ファイル: e2e/m3-scenario.spec.ts, e2e/fixtures/tauriMock.ts | 依存: T43d)
  - 対応: PRD §8「UI」(ツリーの操作)、FR-037(AC-T25)の通し、NFR-005 の画面側(記録だけ)
  - 先に書くテスト(このタスク自体がテスト): `ツリーの展開_新規作成_名称変更_コピーとペースト_ゴミ箱_ドラッグでの移動_ターミナルへの挿入を通す`、`プロジェクトを切り替えるとツリーと読み込みの対象が切り替わる`、`10_万件のフォルダを展開して行が出るまでの時間を記録する`(偽の Rust 側が 10 万件を返す。数値で失敗させず `testreport/perf/` に書く)
  - 並行: 可(T44a とファイルが重ならない)
  - 検証: `npm run e2e`(3 回続けて合格)

#### Phase 28c(逐次。人間ゲート)

- [ ] T45 — M3 の完了確認とドッグフーディング(変更ファイル: output/tasks/PROGRESS.md, output/tasks/DOGFOODING_NOTES.md, output/reports/test/M3_acceptance.md, output/design/ADR_007_finder-integration.md, output/design/ARCH_multi-terminal.md, docs/, project-config.md | 依存: T44a, T44b)
  - AI がやること: `npm run build && npm test && npm run lint && npm run test:coverage && npm run e2e && npm run app && npm run smoke && npm run perf` と `node scripts/smoke.mjs --env=ARIN_VERIFY_GRID=1`、`cargo test -p arin-platform --test trash -- --ignored` を通し、下の対応表に沿って AC の合否一覧を `output/reports/test/M3_acceptance.md` と `testreport/` に出す。NFR-005 を記録する(合否にしない)。`docs/`(`state.json` の `tree.showHidden`、写しの形、コマンドとイベント、`treeStore`・`uiStore`、ショートカットの表、新しい落とし穴)と `project-config.md` §2(`notify` の版)・§3 を同期する。設計書を直す: §2(`notify` の版)、§5.2 の通信の一覧(ツリー・ファイル操作・デスクトップ)、§6.1(`treeStore`・`uiStore`)、§6.2(`tree.expanded` は M6)、§9.1(ショートカットの表と受け口、ツリーのヘッダ=ルートの行、仮想スクロールは自作)、§15.3 #3・#4。ADR-007 の状態を本人の了承に合わせて直す。fresh context の `/code-review` と `verifier` を 1 回ずつ回す
  - **人間がやること**(`npm run app` で作った `.app` を Finder から開く): (1) ツリーで ⌘⌫ でゴミ箱に入れたファイルを、Finder のゴミ箱の「戻す」で戻せる(AC-T16。T37 (a) で採った方式)。(2) `src/a b.ts` をツリーからターミナルへドラッグすると `src/a\ b.ts `(末尾に空白)が入り、Enter が送られない(AC-T04。マウスとトラックパッド)。(3) ツリー内のドラッグで移動の確認が出て、⌥ を押すとコピーになる(FR-029)。(4) Finder でコピーしたファイルをツリーにペーストでき、ツリーでコピーしたファイルを Finder にペーストできる(FR-027)。ツリーで ⌘C したあと、クリップボードの中身が WebKit に上書きされていない。(5) ツリーにフォーカスがあるときに ⌘⌫・⌘N・⇧⌘N・⌘O・⌥⌘R・⌘↩・⇧⌥⌘C・⌥⌘C・Enter・⌘C・⌘X・⌘V が効き、ターミナルにフォーカスがあるときのターミナルの動き(⌘C・⌘V・⌘T など)が M2 から変わっていない(US と JIS の配列。【仮定】21・22。効かないキーがあれば表の受け口を替える)。(6) ターミナルで `touch src/new.txt` を実行すると 2 秒以内にツリーに出る(AC-T02)。`git checkout` などで大量に変わっても固まらない。(7) 「Finder で表示」と「開く」・ダブルクリック(FR-018)。(8) 大きな実リポジトリでの展開の体感(NFR-005。参考)。(9) 行の見た目と、メニュー・ダイアログの見た目。(10) **VSCode を閉じて 1 週間使い、不満を `DOGFOODING_NOTES.md` に書く**(M6 の完了条件の「不満リスト」になる)。期間中は取り消し(FR-032)が無い点に注意する(PRD §11)
  - 完了条件: 下の「M3 の完了条件」をすべて満たす

#### M3 の完了条件

- 下の対応表の AC が、自動テストと T45 の人間の確認で全件合格している(M6 に回した部分=FR-018 の Quick Look、FR-029 の「設定で確認を省略」を除く)
- `npm run build`・`npm test`・`npm run lint`・`npm run test:coverage`(`arin-core` と `src/logic` のそれぞれで行 80%)・`npm run e2e`・`npm run smoke`(通常と `--env=ARIN_VERIFY_GRID=1`)が終了コード 0。`cargo test -p arin-platform --test trash -- --ignored` が通り、テストの項目がゴミ箱に残っていない
- テストとスモークが、本物の `~/Library/Application Support/Arin/` と一時フォルダの外のファイルを作っていない・変えていない
- `cargo tree -i reqwest --target aarch64-apple-darwin` が空(通信のクレートが増えていない)
- ADR-007 の (a) の欄が埋まり、M3 で足した依存(`notify` の版、feature)が追記にある
- `src-tauri/src/menu.rs` と `src/logic/keys/` にショートカットのキーの文字列の直書きが残っていない(表から引く。【要件の追加】)
- 本人が T45 の (1)〜(9) を確認し、(10) のドッグフーディングを終えた

#### FR の受け入れ基準とタスクの対応(漏れの確認用)

| FR | 受け入れ基準 | 規則・OS | コマンド | 画面 | 人間(T45) |
| -- | ------------ | -------- | -------- | ---- | ---------- |
| FR-013 | ルートを起点にフォルダとファイルが階層表示される | T38c、T40a | T42a | T41a、T43b | (9) |
| FR-013 | フォルダの中身は展開したときに読み込まれる | — | T42a | T41a、T43a、T43b | — |
| FR-014 | `src` を展開中に `touch src/new.txt` で 2 秒以内に `new.txt` が現れる(AC-T02) | T38c、T40b | T42b | T41a、T43a、T43b | (6) |
| FR-014 | 外部での削除・リネームも手動操作なしで反映される | T38c、T40b | T42b | T41a、T43b | (6) |
| FR-015 | `src/a b.ts` の相対パスコピーでクリップボードが `src/a b.ts`(AC-T03) | T38b | T42d | T41b、T43c | — |
| FR-015 | 絶対パスコピーで `/Users/x/dev/app/src/a b.ts` | T38b | T42d | T41b、T43c | — |
| FR-016 | ターミナルへのドラッグで `src/a\ b.ts `(末尾に空白)が入り Enter が送られない(AC-T04) | T38b、T44a(zsh との往復) | T42d | T41b、T43d | (2) |
| FR-017 | 切替で隠しファイルの表示/非表示が切り替わる | T38c、T38e | T42a | T43a、T43b | — |
| FR-017 | 既定で `.git` `node_modules` `.DS_Store` が表示されない | T38c | T42a | — | — |
| FR-018 | 「Finder で表示」で Finder が対象を選択して開く | T40d | T42d | T43c | (7) |
| FR-018 | 「開く」・ダブルクリックで既定アプリで開く | T40d | T42d | T43b、T43c | (7) |
| FR-018 | スペースで Quick Look(Should) | M6(T59) | | | |
| FR-024 | ファイル/フォルダの右クリックで §6.4 の項目(M3 は Must の項目) | — | — | T41b、T43c | (9) |
| FR-024 | 余白の右クリックで 7 項目 | — | — | T41b、T43c | — |
| FR-024 | 「パスをターミナルに挿入」でフォーカス中のターミナルへ相対パス | T38b | T42d | T41b、T43c | — |
| FR-024 | 各操作が割り当てたショートカットでも実行できる | T38d | T42e | T41b、T43c | (5) |
| FR-025 | `src` で「新規ファイル…」→ `a.ts` → Enter で空ファイルができ選択される(AC-T13) | T39b、T40a | T42c | T41b、T43a、T43c | — |
| FR-025 | ファイル上なら同じ階層、余白ならルート直下に作られる | T41b | T42c | T43c | — |
| FR-026 | インライン編集で拡張子を除いた部分が選択される | — | — | T41a、T43c | — |
| FR-026 | 確定でディスク上の名前とツリーの表示が新しい名前になる | T39b、T40a | T42c | T43c | — |
| FR-027 | Finder でコピーしたファイルをツリーにペーストできる | T39b、T40d | T42c | T43c | (4) |
| FR-027 | ツリーでコピーしたファイルを Finder にペーストできる | T40d | T42c | T43c | (4) |
| FR-027 | フォルダ上でペーストするとその中に置かれる | T39b | T42c | T41b、T43c | — |
| FR-027 | 切り取り後にペーストすると元から無くなり、ペースト先にある | T39b | T42c | T43c | — |
| FR-028 | ⌘⌫ でゴミ箱へ移動し、ツリーから消える(AC-T16) | T40c | T42c | T43c | (1) |
| FR-028 | Finder のゴミ箱の「戻す」で復元できる(AC-T16) | T40c(T37 の (a)) | — | — | (1) |
| FR-028 | ゴミ箱を経由しない削除が存在しない | T38a(ポートに削除が無い)、T39b | T42c | — | — |
| FR-029 | 別のフォルダへドロップすると移動する | T39b | T42c | T41b、T43d | (3) |
| FR-029 | ⌥ を押しながらでコピー | T39b | T42c | T41b、T43d | (3) |
| FR-029 | 移動の前に確認が出て、中止で何も変わらない | — | — | T43a、T43d | (3) |
| FR-029 | 設定で確認を省略できる(C-05) | M6(T56。設定ファイル)【本人の判断】 | | | |
| FR-030 | `docs/a.md` があるとき別の `a.md` をドロップすると 3 択が出て、中止でどちらも変わらない(AC-T18) | T39a、T39b、T40a | T42c | T43c、T43d | — |
| FR-030 | 確認なしに上書きされる経路が無い | T38a、T39b、T40a、T44a | T42c | — | — |
| FR-035 | ルートを選ぶと名称変更・切り取り・ゴミ箱が無効(AC-T23) | T39a | T42c | T41b、T43c | — |
| FR-035 | ルート配下以外を変更しない | T38a、T39a | T42a、T42c、T42d | — | — |
| FR-036 | メニューを開いたあとに外から消された対象の名称変更で、何も変わらず「見つかりません」と出て行が消える(AC-T24) | T39b、T44a | T42c | T43a、T43c | — |
| FR-036 | メニューを開いたあとに外で同名ができたら FR-030 の確認が出る | T39b、T44a | T42c | T43a、T43c | — |
| FR-037 | ドラッグ中に 100 ファイル追加されても行が動かず、ドロップ後に反映される(AC-T25) | — | — | T41a、T43d、T44b | — |
| FR-037 | インライン入力中・メニュー表示中も同様 | — | — | T41a、T43c | — |
| FR-002(M2 から) | 切替でツリーのルートが B になる | — | T42b | T43b | — |
| FR-007(M2 から) | ルートを改名してもツリーが表示され続ける(AC-P07) | — | T42b | T43b | — |
| FR-060 | ツリーのヘッダの着色(M2 で済み)を壊さない | — | — | T43b | (9) |
| NFR-005 | 10 万ファイルで展開を計測して記録する(合否にしない) | T41a(窓)、T44a | — | T44b | (8) |
| 【要件の追加】 | ショートカットのキーを 1 つの表から引く(M4.5 の土台) | T38d | T42e | T41b、T43c | (5) |

#### M2 の残りの問題の割当(`output/reports/test/M2_acceptance.md` の「判断が要る点・残っている問題」から、ツリーに関わるもの)

| 項目 | 扱うタスク | 扱い |
| ---- | ---------- | ---- |
| 見つからないプロジェクトは、同じパスにフォルダを作り直すと起動中は自動でつながる(FR-007 の仕様の確認が要る) | T42b、T45(本人) | T42b はつながったら監視を張り直す(いまの振る舞いに合わせる)。仕様として良いかは本人が T45 で判断する |
| 状態の写しが作った順と違う順で届くことがあり得る | 扱わない | ツリーが写しから使うのは選択と隠しファイル表示だけで、中身は別のコマンドで取る。M6(T55)の再分解で扱いを決める |
| 応答しないネットワークのボリュームで、終了時に見回りのスレッドを待って止まる恐れ | 扱わない | 監視(T40b)も同じ恐れがある。M3 ではネットワークのボリュームを確かめない。ドッグフーディングで出たら不具合として扱う |
| それ以外(Dock の終了、書き込みが止まったタブ、ゾンビ、読み取りスレッド、使われない型) | 扱わない | ツリーに関わらない。M6(T55)の再分解で扱いを決める |

### M4 タイル表示(T46 の着手時に 1 セッション粒度に分解し直した。2026-11-15)

完了条件: FR-048、051〜055、FR-060・061 のタイル部分の AC 合格と、9 枚同時表示の体感の判定(人間)(対応表は節の末尾)。

#### M4 の前提

設計の決定は ADR-011(`output/design/ADR_011_tile-view.md`。Proposed)にまとめた。【仮定】として扱い、異論があれば T50a までに直す。

- (a) **表示の状態の持ち主は Rust**(設計書 §3.4「タイルのグリッドとページ、タイルとカードの連動」は `arin-core`)。`Workspace` が表示モード(タブ/タイル)・表示中のページ・拡大中か(真偽)を持ち、グリッドの寸法・ページ数・表示中のページのタイルの並びを計算して写しの `display` に載せる。画面は写しを映すだけ(グリッドの規則を TypeScript に書かない)。アテンションの判定(設計書 §7.1「アテンション」の 2。タイル表示=フォーカス中のタイル。【仮定】14)も同じ情報を使う
- (b) **保存するのは表示モードだけ**(`state.json` の `ui.displayMode`。設計書 §6.2)。ページと拡大中は保存しない(設計書 §6.1 の `PaneSizes` の行)。再起動するとタイル表示の 1 ページ目(フォーカスのあるタイルのページ)で開く
- (c) **フォーカス中のタイル=選択中のプロジェクトの表示中のタブ**。タイルをクリックすると、そのタイルのプロジェクトが選択され、そのタブが表示中になる(FR-053。新しいコマンド `focus_terminal`)。タイル表示では「フォーカス中のタイルは必ず表示中のページにある」を保つ(ページはフォーカスに付いていく)。この形にすると、M3 の「パスをターミナルに挿入」の宛先(選択中のプロジェクトの表示中のタブ。M3 の【仮定】16)が、そのままフォーカス中のタイルになる(M3 の再分解の申し送り)
- (d) **拡大**(FR-054)は表示モードをタイルのまま「拡大中」を真にすること。拡大中の右ペインはタブ表示と同じ形(フォーカス中のタイルのプロジェクトのタブバーと、そのターミナル 1 枚)に、元に戻すボタンを足したもの。同じ操作(ショートカット、または元に戻すボタン)でタイル表示に戻る
- (e) **DOM の持ち方**: M2 の仕組み(xterm.js の要素は React の外。`TerminalHost` が置き場所を描き、`TerminalManager.attach` が要素を置き場所の間で移す。隠すものは `setVisible(false)` で描画の資源を手放し、出力の取り込みは続ける)をそのまま使う。`TerminalManager.present` の「表示中の 1 枚」を「表示中の並び」に広げる。表示中のページのタイルだけを置き場所に入れ、ほかのページは隠す(NFR-009、設計書 §1.3 #4)
- (f) **ショートカット**(【要件の追加(本人。2026-11-15)】の決まり): M4 で足すキーは、ショートカットの表(`crates/arin-core/src/shortcuts/table.rs`)に 4 行を足して入れる(T47a)。受け口はどれも「メニューの項目」。メニューバーに「表示」を足し(T47c)、項目のアクセラレータは表から作る。部品・メニューにキーの文字列を直書きしない(ボタンのツールチップのキーの表示も、画面が受け取った表から引く)
- (g) **アテンションの前倒し**(T48。§リスク #3 は 2026-11-14 に本人が A で確定と書かれているが、親タスクの行は【要確認】のままなので、本節では【仮定】として扱う): BEL 単独と OSC 9・OSC 777 の走査器、表示中かの判定、ターミナルごとのアテンションの印(写し)を M4 で作る。M4 の画面が出すのはタイルのタイトルの印と、ページ表示の印だけ(FR-055)。カードの印・タブのバッジ・macOS 通知は M6(T57)に残す
- (h) **検証用の指定の片付け**(ADR-001): 9 枚の検証画面(`ARIN_VERIFY_GRID`、`VerificationGrid`)は T49c・T49d で撤去する。描画方式(`ARIN_RENDERER`)・全角幅の方式(`ARIN_UNICODE`)・`RendererLabel` は、T49b で複数枚を実測して案を ADR-001 に書き、本人が T50a で体感を比べてから、T50b で外す【仮定】(体感を比べる前に外すと、本人が実機で比べられないため)

本計画が置いた【仮定】(M4 の分。ADR-011 にも書いた):

1. タイルの並びは写しの `terminals[]` の順(プロジェクトの並び順 → タブの順。FR-051・C-10)。見つからないプロジェクトに残っているタブ(C-06)もタイルに並べる
2. 最終ページのグリッドは、そのページの枚数で決める(10 枚なら 2 ページ目は 1×1。設計書 §15.2 #4 の A)。0 枚のタイル表示は「ターミナルがありません」の案内だけ(タブ 0 枚と同じ文言)
3. ページ表示(`1/2` と前後のボタン)は、ページが 2 つ以上のときだけ出す。前後のボタンは端で無効にし、循環しない(ショートカットも端では何もしない)
4. ページを送ると、新しいページの先頭のタイルにフォーカスが移る(そのプロジェクトが選択され、ツリーが切り替わる)。前提 (c) の「フォーカス中のタイルは表示中のページにある」を保つため
5. タイル表示でカードを選ぶと、そのプロジェクトの最初のタイルがあるページへ移る(FR-053 の 2 つ目の AC)。そのプロジェクトの表示中のタブがそのページに無ければ、最初のタイルを表示中のタブにする。タブが 0 枚のプロジェクトならページは変えない
6. タイル表示での ⌘T・⌘W・⌘⇧[・⌘⇧] は、タブ表示と同じく選択中のプロジェクトのタブを操作する。操作のあとのフォーカス中のタイルのページへ移る(ページの数が減ったら丸める)
   - タブが閉じて(⌘W・シェルの終了・プロジェクトの削除)選択中のプロジェクトのタブが 0 枚になり、ほかにタイルが残っていれば、閉じたタイルがあった位置のタイル(末尾なら 1 つ前)にフォーカスを移す(拡大中なら拡大は解ける)。カードで 0 枚のプロジェクトを選んだとき・タブ表示では移さない(2026-11-16、M4 のレビューの S4。ADR-011 の 3)
7. 拡大中にカードを選ぶ・⌘⇧[ などでタブを替えると、拡大したまま、そのタブを出す(拡大中は「タブ表示と同じ形」なので)。表示モードをタブに切り替えると、拡大中は解ける
8. タイルのダブルクリックで拡大するのは、タイルのタイトルバーのダブルクリック(ターミナルの本文のダブルクリックは xterm.js の単語の選択に使われるため)。元に戻すのは、拡大中のタイトル帯の元に戻すボタン、同じショートカット、同じタイトル帯のダブルクリック
9. M4 で表に足すキー(PRD に無いので仮の値。本人が T50a で実機で確かめ、M4.5 で設定ファイルから変えられる):
   - `view.toggle-display-mode`(タイル表示とタブ表示の切替): `Cmd+Shift+G`(⇧⌘G。Safari の「すべてのタブを表示」と同じキー)
   - `view.previous-page`・`view.next-page`(前・次のページ): `Cmd+Alt+ArrowLeft`・`Cmd+Alt+ArrowRight`(⌥⌘←・⌥⌘→)
   - `view.toggle-maximize`(タイルの拡大・元に戻す): `Cmd+Shift+Enter`(⇧⌘↩。iTerm2 のペインの拡大と同じキー)
   - タブ表示では、ページ送りと拡大の項目は何もしない(メニューの項目は無効にせず並べる。開くたびに状態を問い合わせないため。M3 の【仮定】8 と同じ考え方)
10. メニューバーの「表示」の項目の文言は固定: 「タイル表示とタブ表示を切り替える」「前のページ」「次のページ」「タイルを拡大/元に戻す」。置き場所は「編集」の右(macOS の並び)
11. 表示モードの切替のボタンは、上部の帯の右側に置く 2 つのアイコンの切替(タブとタイル。Finder の表示の切替と同じ形)。ページ表示はタイル表示のときにタブバーの位置に出す帯の右端
12. タイルのタイトルバーは「プロジェクト名: タブ名」(FR-061)と、その前の小さな丸い色の印(M3 の締めでカード・タブ・ツリーのヘッダを色の印にしたのにそろえる。FR-060・AC-C02 は印の色相で確かめる。M3 の合否一覧の FR-060 と同じ判定)。フォーカス中のタイルは、タイトルの文字を濃くし、細い強調の線で示す。タイル同士は細い区切り線で分け、ターミナルの本文は不透明(ADR-010)
13. アテンション(FR-055): 印は、タイトルバーの右端の小さな丸(色はシステムの強調色。プロジェクト色ではない)。ページ表示の印は、アテンションのあるページの方向の前後のボタンに付く小さな丸。どちらもそのターミナルがフォーカス中になったら消える(設計書 §7.1「アテンション」の 5。【仮定】14)。OSC の本文は M4 では使わない(保持しない。macOS 通知は M6)
14. アテンションの判定は §15.2 #10 の A(アプリが背面でも、いま見ているターミナルのベルはアテンションにしない)。いま見ている=どの表示モードでもフォーカス中のターミナル 1 枚(タブ表示では選択中のプロジェクトの表示中のタブ、タイル表示ではフォーカス中のタイル、拡大中は拡大したタイル)。同じページに見えているだけのタイルには印が立ち、ページを送って見えただけでは消えない。2026-11-15 に「タイル表示=表示中のページのタイル」から改めた(表示中のページの全タイルを見ている扱いにすると、FR-055 の 1 つ目の受け入れ基準「アテンションのあるタイルのタイトルにバッジが付く」を満たせないため。ADR-011 の 7)
15. OSC 777 は `777;notify;…` の形だけをアテンションにする(ほかの副コマンドは無視)。OSC 9 は `9;4;…`(進捗)を除く
16. 状態ファイルの `ui.displayMode` が無い・知らない値のときはタブ表示で始め、状態ファイルの退避はしない(1 つの値のためにプロジェクトの一覧を退避しない)
17. 隠したまま作るターミナルの初めの寸法は、M2 のまま「最後に表示していたターミナルの寸法」(タイル表示ではタイル 1 枚の寸法になる)。表示に変わったときに置き場所に合わせ直す
18. 9 枚の検証画面の代わり: スモークと計測の 9 枚は、一時の状態フォルダに 9 プロジェクトと `ui.displayMode: "tiles"` の `state.json` を置いて起動する(起動時に各プロジェクトに 1 枚ずつタブができる。T49c)

#### 現状のコードから分かった、M4 で足すところ・変えるところ

- `arin-core/layout` は `panes` だけ(`layout/mod.rs` は `pub mod panes;` の 1 行)。`DisplayMode`・`TileLayout` が無い → T46a
- `Workspace`(`workspace/state.rs`)は `layout: PaneSizes` だけを持ち、表示モード・ページ・拡大中を持たない。写し(`workspace/snapshot.rs`)にも無い。`ordered_tabs()`(プロジェクト順 → タブ順。持ち主の無いものは最後)はタイルの並びにそのまま使える → T46b
- `select_project` は選択を変えるだけ、`activate_terminal` はそのタブのプロジェクトの表示中を変えるだけで、選択中のプロジェクトは変えない(`workspace/terminals.rs`)。「タイルをクリックするとプロジェクトも選ぶ」口が無い → T46b(`focus_terminal`)
- 状態ファイルの `PersistedLayout`(`persistence/codec.rs`)は幅・折りたたみ・高さだけ → T46c
- `OscScanner`(`terminal/osc.rs`)は OSC の番号と終わり方を見分け、BEL が OSC の終端かを区別済み。BEL 単独と OSC の先頭の引数(`9;4`、`777;notify`)は見ていない。読み取りスレッドは、検証用の記録があるときだけ走査する(`sessions.rs` の `read_loop`・`ClipboardCounter`)→ T48a(走査器)、T48c(常に走査する)
- `TerminalStore` と写しの `TerminalTabSnapshot` はアテンションを持たない → T48b
- `src-tauri/src/commands/layout.rs` は `set_pane_sizes`・`set_sidebar_collapsed` だけ → T47b。`menu.rs` は「Arin」「ターミナル」「編集」だけで、`on_menu_event` がタブの操作を `SessionRegistry` へ渡す形 → T47c で「表示」を同じ形で足す
- ショートカットの表は 18 行(`ShortcutCommandId::ALL`)。表示の操作の ID が無い → T47a
- 画面: `selectors.ts` の `presentation` は「表示中の 1 枚と残り」、`terminalSync.ts` は `present(shown, background)`、`TerminalManager.present` も 1 枚だけを表示に戻す。`TerminalArea.tsx` はタブバーと置き場所 1 つ → T47d(複数枚の表示)、T47e(操作)、T47f・T47g(部品)
- `AppShell.tsx` は `verifyGrid` のときに `VerificationGrid` だけを描き、上部の帯の右端に `RendererLabel` を出す。`src/main.tsx` は起動時の指定(`get_launch_options`)で検証画面と描画方式を決める → T49c(検証画面)、T50b(描画方式の表示)
- 9 枚の検証画面は Rust でも広く使われている: `Workspace::for_verify_grid`・`ExitPolicy::KeepTab`・`launch_options` の `ENV_VERIFY_GRID`・`VERIFY_GRID_TERMINAL_COUNT`・`LaunchOptions::terminal_count()`(M2 の残りの「使われない型」)、`launch.rs`・`lib.rs`・`persistence.rs`(読まず書かない)・`tree_watch.rs`(張らない)、`src-tauri/tests/` の多く(`sessions.rs`・`session_pty.rs`・`app_quit.rs`・`terminal_commands.rs`・`app_persistence.rs`・`tree_watch.rs`・`perf_sessions.rs`・`support/mod.rs` など)が持ち主の無い 9 枚を足場にしている。スモークは `--env=ARIN_VERIFY_GRID=1` を持つ → T49c(画面とスモーク)、T49d(Rust と結合テストの足場の置き換え)
- 画面の自動テストの偽物(`e2e/fixtures/tauriMock.ts`)は、写しの形とプロジェクト・ツリーのコマンドを持つ。表示のコマンドと、グリッドとページの計算(Rust の規則の写し。テスト用)が無い → T47f
- `tree_watch.rs` は写しのたびに選択中のプロジェクトとルートを比べて張り替える作り。タイルのクリックでプロジェクトが頻繁に替わっても、同じプロジェクトの中を移るだけなら張り替えないことを T47b で確かめる

#### 親タスクと子タスク

T46〜T50 は親として残し、作業は子タスクで行う。親は子がすべて済んだら済みにする。T47 は Rust のコマンド・メニューと画面に分けて番号を振った(T47a〜T47g)。T48 は走査器・判定・配線に分けた。

| 親 | 子 |
| -- | -- |
| T46 — タイルの規則(`arin-core/layout`、`arin-core/workspace`) | T46a(グリッドとページの規則、表示モードの型)、T46b(`Workspace` の表示の状態・フォーカス・ページ・拡大と写し)、T46c(表示モードの保存と復元) |
| T47 — 画面(タイルのグリッド、ページ送り、タイトルバー、拡大、モード切替) | T47a(ショートカットの表に 4 行)、T47b(表示のコマンド)、T47c(メニューバーの「表示」)、T47d(画面: 複数枚の表示と隠す)、T47e(画面: 表示の操作とタイルの写しの読み取り)、T47f(タイルのグリッド・タイトルバー・ページ送り・モード切替の部品)、T47g(拡大・アテンションの印・フォーカスと挿入の宛先) |
| T48 — アテンションの走査を前倒しで作る(【仮定】。前提 (g)) | T48a(走査器)、T48b(表示中かの判定とアテンションの印の写し)、T48c(読み取りスレッドの走査と写しの送り) |
| T49 — 画面の自動テスト、NFR-009 の計測、M1 の検証用の画面の撤去 | T49a(M4 のシナリオの画面の自動テスト)、T49b(NFR-009 の計測と描画方式・全角幅の方式の案)、T49c(検証画面の撤去: 画面とスモーク)、T49d(検証画面の撤去: Rust と結合テスト) |
| T50 — M4 の完了確認 | T50a(合否一覧と人間の確認=体感の判定と描画方式の選択)、T50b(描画方式の決定の反映、`docs/` の同期、最終の検証) |

粗い分解のときの親の行(内容は子に引き継いだ。消さずに残す):

- [ ] T46 — タイルの規則(`arin-core/layout`、`arin-core/workspace`)(変更ファイル: crates/arin-core/src/layout/, crates/arin-core/src/workspace/ | 依存: T45)
  - 対応: FR-048、051、052、053、054。先に書くテスト: グリッド(1→1×1、2→2×1、3〜4→2×2、5〜6→3×2、7〜9→3×3)、10 枚で `1/2`、並び順(プロジェクト順 → タブ順)、タイルとカードの連動、拡大と復帰、表示モードの保存。検証: `npm test` → **T46a〜T46c**
- [ ] T48 — アテンションの走査を前倒しで作る【要確認】(変更ファイル: crates/arin-core/src/terminal/, src-tauri/src/sessions.rs | 依存: T45)
  - 対応: FR-055。FR-055(M4)は、FR-045(M6)が作るはずの BEL と OSC 9/777 の検出に依存している(§リスク #3)。本計画では走査器と判定を M4 に前倒しし、カードの印・タブのバッジ・macOS 通知は M6(T57)に残す案で書いた
  - 先に書くテスト: BEL 単独、OSC 9、OSC 777 の検出。OSC の終端としての BEL と OSC 9;4 を除く。チャンクの切れ目をまたぐ列。表示中かどうかの判定(タイル表示=表示中ページ)。検証: `npm test` → **T48a〜T48c**(本節では【仮定】として扱う。前提 (g))
- [ ] T47 — 画面(タイルのグリッド、ページ送り、タイトルバー、拡大、モード切替)(変更ファイル: src/ui/terminals/, src/state/, src/logic/keys/, src-tauri/src/commands/, src-tauri/src/menu.rs, src/ipc/ | 依存: T46, T48)
  - 対応: FR-048、051〜055、060、061。表示中ページのタイルだけを画面に出す(NFR-009)。検証: `npm test && npm run e2e` → **T47a〜T47g**
- [ ] T49 — 画面の自動テスト、NFR-009 の計測、M1 の検証用の画面の撤去(変更ファイル: e2e/, scripts/perf/, src/ui/terminals/VerificationGrid.tsx, src/ui/shell/AppShell.tsx | 依存: T47)
  - 対応: AC-M14、AC-M15、AC-C02、NFR-009(9 枚で同時に `yes` を流して記録。合否にしない)。T22 の検証用の画面を削除する。検証: `npm test && npm run e2e && npm run perf` → **T49a〜T49d**
- [ ] T50 — M4 の完了確認(変更ファイル: output/tasks/PROGRESS.md, docs/ | 依存: T49)
  - AC の合否一覧を出す。**人間**: 9 枚同時表示の体感を判定する → **T50a・T50b**

#### 共有のファイルの持ち回り

同じ Phase で 2 つのタスクが同じファイルを触らないように割り当てた。下のファイルは表の順に逐次で触る。同時に動かすタスクは、どの Phase も 2 件まで(このマシンは並行のビルドとテストでメモリが足りなくなったため)。

| ファイル | 触るタスク(この順) |
| -------- | ------------------ |
| `crates/arin-core/src/layout/mod.rs`、`crates/arin-core/src/layout/tiles.rs` | T46a だけ |
| `crates/arin-core/src/terminal/osc.rs`、`crates/arin-core/src/terminal/attention.rs`、`crates/arin-core/src/terminal/mod.rs` | T48a → T48b(`mod.rs` の公開だけ) |
| `crates/arin-core/src/terminal/store.rs` | T48b → T49d(`ExitPolicy::KeepTab` を外す) |
| `crates/arin-core/src/workspace/`(`state.rs`・`snapshot.rs`・`mod.rs`・`terminals.rs`・`display.rs`) | T46b → T46c → T48b → T49d |
| `crates/arin-core/src/persistence/codec.rs` | T46c だけ |
| `crates/arin-core/src/shortcuts/table.rs`、`crates/arin-core/tests/shortcuts.rs` | T47a だけ |
| `crates/arin-core/src/launch_options.rs` | T49d → T50b |
| `crates/arin-core/tests/generated_types.rs`、`src/ipc/generated/` | T46a → T46b → T47a → T48b → T49d → T50b |
| `src-tauri/src/sessions.rs` | T47b → T48c → T49d |
| `src-tauri/src/lib.rs`、`src-tauri/src/commands/mod.rs`、`src-tauri/src/commands/layout.rs`、`src-tauri/Cargo.toml`(`[[test]]`) | T47b → T48c(`Cargo.toml` だけ)→ T49d → T50b |
| `src-tauri/src/menu.rs`、`src-tauri/tests/app_menu.rs` | T47c だけ |
| `src-tauri/src/launch.rs`、`src-tauri/src/persistence.rs`、`src-tauri/src/tree_watch.rs`、`src-tauri/src/commands/app.rs` | T49d → T50b(`launch.rs`・`commands/app.rs` だけ) |
| `src-tauri/tests/`(`support/mod.rs` と、検証画面を足場にしている既存のテスト) | T47b(`display_commands.rs`・`app_persistence.rs` を足す・直す)→ T48c(`session_attention.rs`・`session_pty.rs`)→ T49b(`perf_sessions.rs`)→ T49d(足場の置き換え) |
| `src/ipc/client.ts`、`src/ipc/client.test.ts` | T47b → T49c(`get_launch_options` の `verifyGrid` を読まない)→ T50b |
| `src/terminal/TerminalManager.ts`、`src/terminal/TerminalManager.test.ts` | T47d → T50b(描画方式の表示の口) |
| `src/state/selectors.ts`、`src/state/terminalSync.ts` | T47d → T47e(`selectors.ts` だけ)→ T49c(`terminalSync.ts` の `verifyGrid`) |
| `src/state/actions.ts`、`src/state/uiStore.ts`、`src/state/treeStore.ts`、`src/state/startApp.ts` | T47e だけ |
| `src/state/launchOptions.ts`、`src/main.tsx` | T49c → T50b |
| `src/test/fakeIpc.ts` | T47d(写しの形)→ T47e → T49c |
| `src/logic/keys/`(テストだけ) | T47a だけ |
| `src/logic/verifyGrid.ts`、`src/logic/verifyGrid.test.ts` | T49c だけ(削除) |
| `src/ui/terminals/`(`TerminalArea.tsx`・`TerminalArea.css`・`TileGrid.tsx`・`TileGrid.css`・`TileTitleBar.tsx`・`PageControl.tsx`・`TerminalHost.tsx`) | T47f → T47g → T49c(`VerificationGrid.tsx`・`VerificationGrid.css` の削除) |
| `src/ui/shell/AppShell.tsx`、`src/ui/shell/AppShell.css` | T47f → T49c → T50b(`RendererLabel` を外す) |
| `src/ui/shell/RendererLabel.tsx`、`src/terminal/rendererControl.ts` | T50b だけ |
| `src/ui/tree/icons.tsx`、`src/ui/design/tokens.css` | T47f → T47g |
| `e2e/fixtures/tauriMock.ts` | T47f → T47g → T49a → T49c |
| `e2e/tiles.spec.ts` | T47f → T47g |
| `e2e/m4-scenario.spec.ts` | T49a だけ |
| `e2e/verify-grid.spec.ts`、`e2e/terminal-webgl.spec.ts`、`e2e/app-shell.spec.ts` | T49c(検証画面)→ T50b(描画方式) |
| `scripts/smoke.mjs`、`scripts/smoke.test.mjs` | T49c → T50b |
| `scripts/perf/`、`scripts/perf-run.test.mjs`、`e2e/perf-tiles.spec.ts`、`playwright.config.ts` | T49b だけ |
| `package.json`、`package-lock.json` | T49b(計測の入口を足すときだけ)→ T50b(選ばなかった方式のアドオンを外すとき) |
| `scripts/check-boundaries.mjs` | M4 では触らない見込み |
| `output/design/ADR_011_tile-view.md` | T46a(着手時に読むだけ)→ T50a → T50b(状態を本人の了承に合わせる) |
| `output/design/ADR_001_terminal-engine.md` | T49b(実測と案)→ T50b(決定) |
| `output/design/ARCH_multi-terminal.md`、`docs/`、`project-config.md` | T50b だけ |

#### Phase 29a(並行可能。M3 完了後)

- [x] T46a — タイルのグリッドとページの規則、表示モードの型(`arin-core/layout`)(変更ファイル: crates/arin-core/src/layout/mod.rs, crates/arin-core/src/layout/tiles.rs, crates/arin-core/tests/layout_tiles.rs, crates/arin-core/tests/generated_types.rs, src/ipc/generated/ | 依存: T45)
  - 対応: FR-051(グリッド)、FR-052(1 ページ最大 9 枚)、FR-048(表示モードの型)、C-10・C-11・C-23、設計書 §5.1 の `DisplayMode`・`TileLayout`、§15.2 #4、ADR-011、【仮定】2・3
  - 置くもの: `DisplayMode`(`tabs` / `tiles`。ts-rs で生成。JSON は `"tabs"`・`"tiles"`)。`TileGrid { cols, rows }`(列×行)。純粋な関数: 枚数 → グリッド、総数 → ページ数(0 枚は 0)、ページ番号 → 並びの範囲、n 番目のページ、範囲外のページ番号の丸め。入出力・時計を持たない
  - 並行: 可(T48a とファイルが重ならない。生成物を触るのは Phase 29a ではこのタスクだけ)
  - 先に書くテスト: `1_枚は_1×1`、`2_枚は_2×1`、`3_枚と_4_枚は_2×2`、`5_枚と_6_枚は_3×2`、`7_枚から_9_枚は_3×3`(表で、FR-051 の AC)、`グリッドは列×行で_3×2_は_3_列_2_行`(C-23)、`10_枚は_2_ページで_1_ページ目は_9_枚_3×3_で_2_ページ目は_1_枚_1×1`(AC-M14 の規則。§15.2 #4 の A)、`19_枚は_3_ページで最後は_1_枚`、`0_枚はページが無い`、`ページ番号から並びの範囲が決まる`、`n_番目のターミナルのページ`、`範囲外のページ番号は最後のページに丸める`、`表示モードの_json_は_tabs_と_tiles`。生成した型が最新
  - 検証: `cargo test -p arin-core --test layout_tiles --test generated_types`
- [x] T48a — アテンションの走査器(`arin-core/terminal`)(変更ファイル: crates/arin-core/src/terminal/osc.rs, crates/arin-core/src/terminal/attention.rs, crates/arin-core/src/terminal/mod.rs, crates/arin-core/tests/attention_scanner.rs | 依存: T45)
  - 対応: FR-055 の検出の部分(FR-045 の走査を前倒し。前提 (g))、設計書 §1.3 #5、§7.1「アテンション」の 1、§7.2「OSC 9/777 の本文」、【仮定】13・15
  - 置くもの: `AttentionScanner`(`OscScanner` の上に足す。1 バイトずつの状態機械のまま、チャンクの切れ目をまたいでも同じ結果。状態の大きさは一定)。返すのは「このチャンクでアテンションの契機があったか」だけ(本文は保持しない)。`OscScanner` に「BEL 単独」と「先頭の引数の判定」(`9;4` を除く、`777;notify` だけ)を足す。OSC 52 の数え方(`count_clipboard`)は変えない
  - 並行: 可(`terminal/` の走査器の 3 ファイルとテストだけ)
  - 先に書くテスト: `bel_単独はアテンションの契機`、`osc_の終端の_bel_は契機にしない`、`osc_9_は契機`、`st_で終わる_osc_9_も契機`、`osc_9;4_の進捗は契機にしない`、`osc_777;notify_は契機`、`osc_777_のほかの副コマンドは契機にしない`、`チャンクの切れ目をまたぐ_osc_9_も_1_回だけ`(全ての切れ目で同じ結果)、`切れ目が_esc_と_]_の間でも同じ`、`1_つのチャンクに何度あっても契機があったとだけ返す`、`本文が長くても状態の大きさが変わらない`、`can_と_sub_で打ち切った_osc_は契機にしない`。既存の OSC 52 のテストがそのまま通る
  - 検証: `cargo test -p arin-core --test attention_scanner && cargo test -p arin-core --lib terminal`

#### Phase 29b(逐次。T46a 完了後)

- [x] T46b — `Workspace` の表示の状態(表示モード・ページ・拡大・タイルへのフォーカス)と写し(変更ファイル: crates/arin-core/src/workspace/display.rs, crates/arin-core/src/workspace/mod.rs, crates/arin-core/src/workspace/state.rs, crates/arin-core/src/workspace/snapshot.rs, crates/arin-core/src/workspace/terminals.rs, crates/arin-core/tests/workspace_display.rs, crates/arin-core/tests/app_snapshot.rs, crates/arin-core/tests/generated_types.rs, src/ipc/generated/ | 依存: T46a)
  - 対応: FR-048(切替)、FR-051(並び)、FR-052(ページ)、FR-053(タイルとカードの連動)、FR-054(拡大と復帰)、NFR-009(表示中のページだけ)、設計書 §3.4、§6.1(`PaneSizes` の行の「表示モード、タイルのページ、拡大中か」)、§8「タイル表示」「タイルからの拡大」、ADR-011、前提 (a)(c)(d)、【仮定】1〜7
  - 置くもの: `Workspace` に表示の状態(モード・ページ・拡大中)を足す。口: `set_display_mode`・`toggle_display_mode`(変わったら `true`)、`focus_terminal`(そのタブのプロジェクトを選び、タブを表示中にする。持ち主の無いもの・無い ID は失敗)、`next_page`・`previous_page`(端では何もしない。先頭のタイルにフォーカス)、`toggle_maximize`(タイル表示のときだけ)、`visible_terminals`(表示中のターミナルの並び。T48b の判定と画面の表示に使う)。選択・タブの追加・閉じる・前後・プロジェクトの削除のあとに「ページはフォーカス中のタイルに付いていく」「ページ数が減ったら丸める」を当てる(既存の口の中から 1 つの関数を呼ぶ)。写しに `display`(モード、拡大中、ページ、ページ数、グリッド、表示中のページのタイルの ID の並び)を足す。写しの版は上げない(項目の追加)
  - 安全: 規則は `arin-core` の中だけ。画面からはモードと ID しか受けない
  - 並行: 不可(`workspace/` と生成物を触る)
  - 先に書くテスト: `タイルの並びはプロジェクト順でその中はタブ順`(FR-051)、`10_枚でタイル表示にすると_1_ページ目に_9_枚_ページ数は_2`(AC-M14 の Rust 側)、`タイルにフォーカスすると持ち主のプロジェクトが選択されそのタブが表示中になる`(AC-M15 の Rust 側)、`同じプロジェクトのタイルへのフォーカスは選択を変えない`、`カードを選ぶとそのプロジェクトの最初のタイルのページへ移る`(FR-053)、`表示中のタブがそのページに無ければ最初のタイルを表示中にする`、`タブが_0_枚のプロジェクトを選んでもページは変わらない`、`次のページで先頭のタイルにフォーカスが移る`、`最後のページの次と最初のページの前は何もしない`、`新しいタブを開くとそのタイルのページへ移る`、`タブを閉じてページが減るとページを丸める`、`プロジェクトを消してもページが範囲に収まる`、`拡大すると表示モードはタイルのまま拡大中になり見えるのはフォーカス中の_1_枚`(FR-054)、`もう一度で拡大が解け拡大前のページに戻る`、`拡大中にタブを替えると拡大したままそのタブを出す`、`タブ表示に切り替えると拡大が解ける`、`タブ表示ではページ送りと拡大は何もしない`、`タブ表示で見えるのは選択中のプロジェクトの表示中のタブだけ`、`タイル表示で見えるのは表示中のページのタイルだけ`(NFR-009)、`表示モードを変えると変わったと返し同じなら返さない`、`写しに表示の状態が載る`(`app_snapshot.rs`)。既存の `workspace_*` のテストがそのまま通る
  - 検証: `cargo test -p arin-core`

#### Phase 29c(並行可能。T46b 完了後)

- [x] T46c — 表示モードの保存と復元(変更ファイル: crates/arin-core/src/persistence/codec.rs, crates/arin-core/src/workspace/state.rs, crates/arin-core/tests/persistence_codec.rs, crates/arin-core/tests/workspace_display.rs | 依存: T46b)
  - 対応: FR-048(再起動後に終了時の表示モードで開く)、FR-056、設計書 §6.2 の `ui.displayMode`、前提 (b)、【仮定】16
  - 置くもの: `PersistedLayout` に表示モード。`state.json` の `ui.displayMode`(`"tabs"` / `"tiles"`)。無い・知らない値はタブ表示(退避しない)。`persisted_state` と `restore` でモードだけを往復させる(ページと拡大中は保存しない)
  - 並行: 可(T47a とファイルが重ならない。生成物は触らない)
  - 先に書くテスト: `表示モードを書いて読むと同じモードに戻る`、`display_mode_が無い状態ファイルはタブ表示で始まる`(M3 までの `state.json` を読める)、`知らない値はタブ表示にし退避しない`、`タブ表示のときも_display_mode_を書く`【仮定】、`ページと拡大中は保存する内容に入らない`、`タイル表示で保存して復元するとタイル表示の_1_ページ目でフォーカス中のタイルのページを出す`
  - 検証: `cargo test -p arin-core --test persistence_codec --test workspace_display`
- [x] T47a — ショートカットの表に表示の 4 行を足す(変更ファイル: crates/arin-core/src/shortcuts/table.rs, crates/arin-core/tests/shortcuts.rs, crates/arin-core/tests/generated_types.rs, src/ipc/generated/, src/logic/keys/keyOwner.test.ts, src/logic/keys/shortcutTable.test.ts | 依存: T46b)
  - 対応: FR-048・FR-052・FR-054(ショートカット)、【要件の追加】(表に行を足す)、前提 (f)、【仮定】9
  - 足す行(受け口はどれも「メニューの項目」): `view.toggle-display-mode` = `Cmd+Shift+G`、`view.previous-page` = `Cmd+Alt+ArrowLeft`、`view.next-page` = `Cmd+Alt+ArrowRight`、`view.toggle-maximize` = `Cmd+Shift+Enter`。`ShortcutCommandId::ALL` は 22 行になる。生成物(`ShortcutCommandId.ts`・`defaultShortcuts.json`)を作り直す
  - 画面: `terminalKeyOwner` は受け口がメニューの項目のキーを表から集めて `app` にする作り(T41b)なので、コードは変えずに新しい 4 つのキーがターミナルに渡らなくなることをテストで確かめる
  - 並行: 可(T46c とファイルが重ならない。生成物を触るのは Phase 29c ではこのタスクだけ)
  - 先に書くテスト: 既存の `すべてのコマンドの_id_が既定のキーを持つ`・`同じフォーカスで受けるキーに重なりが無い` が 22 行で通る、`表示の_4_行の既定のキーと受け口`(表で)、`表示の_4_行の_id_の文字列`、`表の_json_の形`(生成物が最新)。Vitest: `表示の操作のキーはターミナルに渡さない`(`keyOwner.test.ts`)、`既定の表に表示の_4_行がある`(`shortcutTable.test.ts`)
  - 検証: `cargo test -p arin-core --test shortcuts --test generated_types && npx vitest run src/logic/keys && node --test scripts/default-shortcuts.test.mjs`

#### Phase 29d(逐次。T46c・T48a 完了後)

- [x] T48b — 表示中かの判定とアテンションの印(`Workspace` と写し)(変更ファイル: crates/arin-core/src/terminal/attention.rs, crates/arin-core/src/terminal/mod.rs, crates/arin-core/src/terminal/store.rs, crates/arin-core/src/workspace/display.rs, crates/arin-core/src/workspace/state.rs, crates/arin-core/src/workspace/snapshot.rs, crates/arin-core/tests/workspace_attention.rs, crates/arin-core/tests/app_snapshot.rs, crates/arin-core/tests/generated_types.rs, src/ipc/generated/ | 依存: T46c, T48a)
  - 対応: FR-055、設計書 §7.1「アテンション」の 2・3・5、§15.2 #10 の A、【仮定】13・14
  - 置くもの: `AttentionPolicy`(ターミナルをいま見ているか。`Workspace::attended_terminals`=フォーカス中のターミナルを使う。画面が描く並びの `visible_terminals` とは別。【仮定】14)。`Workspace::note_attention(id)`(フォーカス中でなければ印を立て、立ったら `true`。既に立っていれば `false`)。表示が変わる口(モード・ページ・拡大・選択・フォーカス・タブの切替)のあとに、フォーカス中になったターミナルの印を消す。写しの `terminals[]` に `attention`、`display` にアテンションのあるページの番号の並びを足す(画面のページ表示の印に使う)
  - 並行: 不可(`workspace/` と生成物を触る)
  - 先に書くテスト: `表示していないターミナルの契機で印が立つ`、`表示中のターミナルの契機では立たない`(§15.2 #10 の A)、`タイル表示ではフォーカス中のタイルに立たない`、`タイル表示で同じページのフォーカスの無いタイルに印が立つ`、`別のページのタイルに立つ`、`拡大中は拡大したタイルのほかに立つ`、`タイル表示でフォーカスすると消える`、`ページを送って先頭のタイルにフォーカスが移ると消える`、`ページを送っただけではフォーカスの無いタイルの印は消えない`、`タブ表示で選んで表示すると消える`、`タイル表示に切り替えて見えただけのタイルの印は残る`、`立っているところにもう一度来ても変わったと返さない`、`閉じたタブの印は残らない`、`写しのターミナルに印が載る`、`写しにアテンションのあるページの番号が載る`
  - 検証: `cargo test -p arin-core`

#### Phase 30a(並行可能。T47a・T48b 完了後)

- [x] T47b — 表示のコマンド(変更ファイル: src-tauri/src/commands/layout.rs, src-tauri/src/sessions.rs, src-tauri/src/lib.rs, src-tauri/Cargo.toml, src-tauri/tests/display_commands.rs, src-tauri/tests/app_persistence.rs, src/ipc/client.ts, src/ipc/client.test.ts | 依存: T46c, T47a, T48b)
  - 対応: FR-048・FR-052〜054 のコマンド、FR-002・FR-053(ツリーの監視の張り替え)、設計書 §5.2「表示」の案、ADR-011、前提 (a)(c)
  - コマンド: `set_display_mode`(モード)、`focus_terminal`(ターミナル ID)、`next_tile_page`・`previous_tile_page`、`toggle_tile_maximize`。`SessionRegistry` に同名の口を足し、変わったら写しを送る(保存はモードが変わったときだけ予約される。写しの中身から保存の内容を作る既存の作りのまま)
  - 監視: タイルのクリックでプロジェクトが替わると `tree_watch.rs` が写しで張り替える(作りは変えない)。同じプロジェクトの中を移るだけなら張り替えないことを確かめる
  - 並行: 可(T47d とファイルが重ならない)
  - 先に書くテスト(模擬ランタイム。`harness = false` の `[[test]]` を足す): `表示モードの切替で写しが変わり状態ファイルに保存される`、`再起動すると前の表示モードで開く`(FR-048 の AC。`app_persistence.rs`)、`タイルへのフォーカスで写しの選択が替わりツリーの監視が新しいルートに張り替わる`(AC-M15 の Rust 側。偽の監視のポート)、`同じプロジェクトのタイルの間を移っても監視を張り替えない`、`ページ送りと拡大のコマンドで写しの表示が変わる`、`知らないターミナルへのフォーカスは失敗を返し何も変えない`。Vitest: 新しいコマンドの包みが正しい名前と引数で届く
  - 検証: `cargo test -p arin --test display_commands --test app_persistence --test tree_watch && npx vitest run src/ipc/client.test.ts`
- [x] T47d — 画面: 表示中のターミナルの並び(複数枚の表示と隠す)(変更ファイル: src/terminal/TerminalManager.ts, src/terminal/TerminalManager.test.ts, src/state/terminalSync.ts, src/state/terminalSync.test.ts, src/state/selectors.ts, src/state/selectors.test.ts, src/test/fakeIpc.ts | 依存: T48b)
  - 対応: NFR-009(表示中のページだけを描く)、FR-052(表示していないページのプロセスと出力が続く)、FR-042(隠したものの取り込み)、設計書 §1.3 #4、§7.3、前提 (e)、【仮定】17
  - 変えるもの: `presentation`(写し → 表示中の並びと隠すもの。タブ表示と拡大中は 1 枚、タイル表示は写しの `display` の表示中のページのタイル)。`TerminalManager.present(shown[], background)`: 表示中の並びをすべて表示に戻し、ほかを `setVisible(false)`(描画の資源を手放す。WebGL のときはコンテキストを返す)。置き場所への出し入れは `TerminalHost` の `attach` のまま。`terminalSync.ts` を並びで伝える形に替える(検証画面の分岐は T49c まで残す)
  - 並行: 可(T47b とファイルが重ならない)
  - 先に書くテスト(Vitest。偽のエンジン): `タブ表示では表示中のタブ_1_枚だけを表示にする`(M2 の既存のテストが通る)、`タイル表示では表示中のページのタイルを表示にしほかを隠す`、`ページが替わると前のページのタイルを隠し新しいページのタイルを表示に戻す`、`隠したターミナルにも出力が届き処理済みを返し続ける`(FR-052)、`隠すと描画の資源を手放す`、`拡大中は拡大したタイル_1_枚だけを表示にする`、`隠したまま作るターミナルは最後に表示していたタイルの寸法で接続する`、`表示中の並びが同じなら何もしない`
  - 検証: `npx vitest run src/terminal src/state/terminalSync.test.ts src/state/selectors.test.ts`

#### Phase 30b(並行可能。T47b 完了後)

- [x] T48c — 読み取りスレッドのアテンションの走査と写しの送り(変更ファイル: src-tauri/src/sessions.rs, src-tauri/Cargo.toml, src-tauri/tests/session_attention.rs, src-tauri/tests/session_pty.rs | 依存: T47b)
  - 対応: FR-055、設計書 §7.1「ターミナルの入出力」の 2・「アテンション」、§7.1「スレッドの分担」(PTY の読み取りと走査はターミナルごとの読み取りスレッド)
  - 変えるもの: 読み取りスレッドが、検証用の記録の有無によらず `AttentionScanner` で走査する(走査器はそのスレッドだけが持つ。ロックを取らない)。契機があったチャンクのあとにだけ `Workspace` のロックを短く取って `note_attention` を呼び、印が立ったときだけ写しを送る。OSC 52 の数え方(検証用の記録)は変えない
  - 安全: 走査は出力の中身を保存しない・画面へ本文を送らない。ロックの順序(接続の直列化 → Workspace → 一覧)を守り、送信のあいだはロックを持たない
  - 並行: 可(T47c とファイルが重ならない)
  - 先に書くテスト(偽の PTY。`harness = false` の `[[test]]` を足す): `表示していないタブで_bel_が出ると写しにアテンションが載る`、`表示中のタブの_bel_では写しを送らない`、`bel_が続いても写しは_1_回だけ送られる`、`osc_9;4_の進捗では写しを送らない`、`検証用の記録の_osc_52_の数え方は変わらない`。実 PTY(`session_pty.rs`。`/bin/zsh` 固定、`HOME` は一時フォルダ、目印、上限時間): `隠れたタブで_printf_'\a'_を実行すると印が立つ`
  - 検証: `cargo test -p arin --test session_attention --test session_pty`
- [x] T47c — メニューバーの「表示」(変更ファイル: src-tauri/src/menu.rs, src-tauri/tests/app_menu.rs | 依存: T47b)
  - 対応: FR-048・FR-052・FR-054(ショートカット)、【要件の追加】(アクセラレータは表から)、前提 (f)、【仮定】9・10
  - 変えるもの: 「表示」の見出しと 4 つの自前の項目(文言は `menu.rs`、ID とキーは表)。並びは「編集」の右。`on_menu_event` で `SessionRegistry` の表示の口を呼ぶ(タブの操作と同じ形。画面を経由しない)
  - 並行: 可(T48c とファイルが重ならない)
  - 先に書くテスト(模擬ランタイム): `表示に切替_前のページ_次のページ_拡大がこの順である`、`表示の項目のアクセラレータが表の既定のキーと同じ`、`見出しの並びは_arin_ターミナル_編集_表示`、`表示の項目を選ぶと写しの表示が変わる`、`タブ表示でページ送りと拡大の項目を選んでも何も変わらない`。既存の `app_menu.rs` のテストがそのまま通る
  - 検証: `cargo test -p arin --test app_menu`

#### Phase 30c(逐次。T47b・T47d 完了後)

- [x] T47e — 画面: 表示の操作とタイルの写しの読み取り(変更ファイル: src/state/actions.ts, src/state/actions.test.ts, src/state/selectors.ts, src/state/selectors.test.ts, src/state/uiStore.ts, src/state/uiStore.test.ts, src/state/treeStore.ts, src/state/treeStore.test.ts, src/state/startApp.ts, src/state/startApp.test.ts, src/test/fakeIpc.ts | 依存: T47b, T47d)
  - 対応: FR-048・FR-052〜055 の画面の操作、FR-061(タイトルの文字列)、設計書 §6.1、M3 の残りの問題「切り替えたあとに届いた前のプロジェクトの応答で、衝突の確認が開きうる」(タイルのクリックでプロジェクトの切替が増えるため M4 で扱う)
  - 置くもの: `actions` に `setDisplayMode`・`focusTile`(フォーカス中なら呼ばない)・`nextTilePage`・`previousTilePage`・`toggleTileMaximize`。セレクタ `tileView`(表示中のページのタイル=ID・「プロジェクト名: タブ名」・色・フォーカス中か・アテンション・状態、グリッド、ページの文字列 `1/2`、前後に進めるか、前後のページのアテンション)。ツリー: 応答が届いたときに、そのプロジェクトがもう表示中でなければ、確認や知らせを開かずに捨てる(読み込み済みの中身の更新は続ける)
  - 並行: 不可(`src/state/` をまとめて触る)
  - 先に書くテスト(Vitest。偽の通信): `表示モードの切替でコマンドを呼ぶ`、`フォーカス中のタイルをクリックしてもコマンドを呼ばない`、`別のタイルへのフォーカスでコマンドを呼ぶ`、`ページ送りと拡大のコマンドを呼ぶ`、`タイルの見出しはプロジェクト名とタブ名をつなぐ`(FR-061)、`タイルの色はそのプロジェクトの色`、`ページの文字列は_1/2`(AC-M14)、`最初のページでは前へ進めない`、`前後のページのアテンションを返す`(FR-055)、`ページが_1_つならページ表示を出さない`(【仮定】3)、`プロジェクトを替えたあとに前のプロジェクトの衝突の応答が届いても確認を開かない`(M3 の残り)
  - 検証: `npx vitest run src/state`

#### Phase 30d(逐次。T47c・T47e 完了後)

- [x] T47f — タイルのグリッド、タイトルバー、ページ送り、表示モードの切替の部品(変更ファイル: src/ui/terminals/TileGrid.tsx, src/ui/terminals/TileGrid.css, src/ui/terminals/TileTitleBar.tsx, src/ui/terminals/PageControl.tsx, src/ui/terminals/TerminalArea.tsx, src/ui/terminals/TerminalArea.css, src/ui/shell/AppShell.tsx, src/ui/shell/AppShell.css, src/ui/tree/icons.tsx, src/ui/design/tokens.css, e2e/fixtures/tauriMock.ts, e2e/tiles.spec.ts | 依存: T47c, T47e)
  - 対応: FR-048(ボタン)、FR-051、FR-052(前後のボタン、表示していないページの継続)、FR-053、FR-060(タイルのタイトルバー)、FR-061、NFR-009(表示中のページだけを描く)、PRD §6.3、設計書 §8・§9.1・§9.2、ADR-010、前提 (e)、【仮定】2・3・11・12
  - 作るもの: `TileGrid`(Container。写しのグリッドの列×行で、表示中のページのタイルだけを描く。各タイルはタイトルバーと `TerminalHost`)、`TileTitleBar`(色の印・「プロジェクト名: タブ名」。名前は文字として差し込む)、`PageControl`(`1/2` と前後のアイコンのボタン。ツールチップのキーは受け取った表から引く)。`TerminalArea` は表示モードでタブ表示とタイル表示を出し分ける。上部の帯に表示モードの切替(アイコン 2 つ)。タイルのクリックでフォーカスのコマンド。アイコンは自作の少数の SVG(設計書 §15.2 #12)。偽の Rust 側に表示のコマンドと、グリッド・ページの計算(テスト用に Rust の規則を写したもの)を足す
  - **デザイン**(本人の指示): 「AI にありがちなデザインを避け、Apple のデザインらしく、ハイセンスに」。macOS 標準の作法に寄せる(システムフォント 13px、控えめな色、細い区切り線、角丸の淡い選択、アイコンのボタン。パステルの塗り・太い枠・文字だけの大きなボタンを避ける)。プロジェクト色は小さな丸い印で示す。ウィンドウは半透過(すりガラス)、ターミナルの置き場所は不透明。**Playwright の画像(ライトとダーク、1・4・9・10 枚)を撮り、自分の目で確かめてから終える**(画像は `testreport/` に置き、報告に添える)
  - 安全: プロジェクト名・タブ名は信頼しない文字列として文字で差し込む(設計書 §12)
  - 並行: 不可
  - 先に書くテスト(Playwright。偽の Rust 側): `切替のボタンでタイル表示になり全プロジェクトのターミナルが並ぶ`(FR-048・FR-051)、`もう一度でタブ表示に戻り表示中のタブ_1_枚だけが描かれる`、`枚数に応じたグリッド`(1→1×1、2→2×1、4→2×2、6→3×2、9→3×3 を表で。FR-051)、`10_枚で_1_ページ目に_9_枚_2_ページ目に_1_枚_表示は_1/2`(AC-M14)、`前後のボタンでページが替わり端のボタンは無効`(FR-052)、`表示中のページのタイルの置き場所だけがある`(NFR-009。ターミナルの要素の数)、`表示していないページのターミナルにも出力が届き戻ると表示されている`(FR-052)、`タイルのタイトルはプロジェクト名:_タブ名`(FR-061)、`a_の全タイルとカードの色の印は同じ色相で_b_のタイルとは違う`(AC-C02。計算済みのスタイル)、`別のプロジェクトのタイルをクリックするとカードが選ばれツリーのルートが替わる`(AC-M15)、`カードを選ぶとそのプロジェクトの最初のタイルのページへ移る`(FR-053)、`ウィンドウの大きさを変えると各タイルの寸法が送られる`(FR-051 の 3 つ目の AC)、`html_のような名前もそのまま文字で出る`。既存の `e2e/tabs.spec.ts`・`e2e/projects.spec.ts`・`e2e/terminal.spec.ts` が通る
  - 人間が見る範囲(T50a): 実機での見た目(すりガラスの上の帯とタイトルバー、ライトとダーク)
  - 検証: `npx tsc -b && npx playwright test e2e/tiles.spec.ts e2e/tabs.spec.ts e2e/projects.spec.ts e2e/terminal.spec.ts`

#### Phase 30e(逐次。T47f 完了後。T47 の節目)

- [x] T47g — 拡大、アテンションの印、フォーカスとパスの挿入の宛先(変更ファイル: src/ui/terminals/TileGrid.tsx, src/ui/terminals/TileTitleBar.tsx, src/ui/terminals/PageControl.tsx, src/ui/terminals/TerminalArea.tsx, src/ui/terminals/TerminalArea.css, src/ui/terminals/TerminalHost.tsx, src/ui/tree/icons.tsx, src/ui/design/tokens.css, e2e/fixtures/tauriMock.ts, e2e/tiles.spec.ts | 依存: T47f)
  - 対応: FR-054、FR-055、FR-016・FR-024(パスの挿入の宛先。M3 の申し送り)、設計書 §8「タイルからの拡大」、§15.2 #6 の A(別のプロジェクトのターミナルへは絶対パス。T38b で入れた規則)、前提 (c)(d)、【仮定】7・8・13
  - 作るもの: タイトルバーのダブルクリックで拡大。拡大中はタブ表示の形に、タイトル帯(同じ見た目)と元に戻すアイコンのボタンを足す。タイトルバーのアテンションの印、ページ表示の前後のボタンの印。フォーカス中のタイルが替わったら、そのターミナルに入力のフォーカスを移す(M2 の `TerminalArea` の「次の描画のフレームで移す」と同じ扱い)。ツリーのドラッグの落とし先(`TerminalHost` の目印)はタイルでもそのまま使う
  - **デザイン**: T47f と同じ方針。印は小さな丸(システムの強調色)で、塗りの大きなバッジにしない。Playwright の画像で自分の目で確かめる
  - 並行: 不可
  - 先に書くテスト(Playwright): `タイトルバーをダブルクリックするとそのターミナルがタブ表示の形で出て表示モードはタイルのまま`(FR-054)、`元に戻すボタンでタイル表示に戻り前のページが出る`、`拡大中のタイトル帯のダブルクリックでも戻る`、`ターミナルの本文のダブルクリックでは拡大しない`、`アテンションのあるタイルのタイトルに印が付く`(FR-055 の 1 つ目)、`別のページのアテンションはページ表示の前後のボタンに印が付く`(FR-055 の 2 つ目)、`フォーカスすると印が消え_ページを送って見えただけのタイルの印は残る`(【仮定】14)、`タイルをクリックするとそのターミナルに入力のフォーカスが移る`、`ツリーのパスをターミナルに挿入の宛先はフォーカス中のタイル`(M3 の申し送り)、`ツリーの項目を別のプロジェクトのタイルへ落とすとそのターミナルへの挿入が呼ばれる`(宛先の ID。絶対パスにするのは Rust の T42d の既存のテスト。挿入のあと落とした先のタイルを `focusTile` でフォーカス中にし、入力のフォーカスも明示的に移す)、`ツリーの項目をフォーカス中のタイルへ落としてもフォーカスのコマンドを呼ばない`、`ドラッグ中にタイルの上を通っても行が動かない`(AC-T25 の既存のテストが通る)
  - 人間が見る範囲(T50a): 実機でのダブルクリックの手触り、印の見え方
  - 検証: `npx playwright test e2e/tiles.spec.ts e2e/tree-dnd.spec.ts e2e/tree-ops.spec.ts`。節目として続けて `npm run lint && npm test && npm run e2e`(並行のタスクを動かさずに 1 件で回す)

#### Phase 31a(並行可能。T47g・T48c 完了後)

- [x] T49a — M4 のシナリオの画面の自動テスト(変更ファイル: e2e/m4-scenario.spec.ts, e2e/fixtures/tauriMock.ts | 依存: T47g, T48c)
  - 対応: PRD §8「UI」(タイル表示)、AC-M14、AC-M15、AC-C02 の通し、FR-048・FR-052〜055
  - 先に書くテスト(このタスク自体がテスト): `タブ表示からタイル表示に切り替え_ページを送り_別のプロジェクトのタイルをクリックし_拡大して戻し_タブ表示に戻すを通す`、`10_枚で_1/2_を出し_2_ページ目の_1_枚に出力が続いている`(AC-M14・FR-052)、`a_のタイルにフォーカス中に_b_のタイルをクリックするとカードが_b_になりツリーのルートが_b_になる`(AC-M15)、`緑の_a_のカードとタイルの色の印が同じ色相`(AC-C02)、`別のページで鳴ったベルの印がページ表示に出て_送ると消える`(FR-055)、`再読み込みしても写しの表示モードで開く`(FR-048 の画面側)
  - 並行: 可(T49b とファイルが重ならない)
  - 検証: `npx playwright test e2e/m4-scenario.spec.ts`(3 回続けて合格)
- [x] T49b — NFR-009 の計測と、描画方式・全角幅の方式の案(変更ファイル: src-tauri/tests/perf_sessions.rs, scripts/perf/run.mjs, scripts/perf/app-memory.mjs, scripts/perf-run.test.mjs, e2e/perf-tiles.spec.ts, playwright.config.ts, package.json(入口を足すときだけ), output/design/ADR_001_terminal-engine.md | 依存: T47g, T48c)
  - 対応: NFR-009(9 枚で同時に `yes` を流して記録。合否にしない)、ADR-001(描画方式と全角幅の方式は複数枚の実測で決める)、ADR-001 の「流量制御のしきい値(複数枚のときの値は M4 で実測して見直す)」、設計書 §7.3、前提 (h)
  - 測るもの(記録だけ。数値で失敗させない。結果は `testreport/perf/`): (1) Rust 側: 9 つのセッションで同時に `yes` を流したときの、送り先までの量と遅れ・未処理の量の頭打ち(`perf_sessions.rs`。`#[ignore]`)。(2) 画面側: WebKit で 9 枚のタイルに偽の Rust 側から同時に大量の出力を流しているあいだの、ページ送り・タイルのクリック・キー入力の反応までの時間とフレームの間隔を、DOM と WebGL、`unicode11` と `graphemes` の組で記録する(`e2e/perf-tiles.spec.ts`。`npm run e2e` には入れず `npm run perf` から動かす。WebGL のコンテキストの数も記録する)。(3) 実アプリ: 9 プロジェクトのタイル表示の `state.json` で起動したときのメモリ(`app-memory.mjs`。【仮定】18)
  - 書くもの: ADR-001 に「M4 の実測」の節(上の表と、描画方式・全角幅の方式・しきい値の**案**。決定は T50a の本人の体感のあと)
  - 並行: 可(T49a とファイルが重ならない。計測は重いので、T49a の自動テストが終わってから計測を回すこと)
  - 先に書くテスト: `scripts/perf-run.test.mjs` に `画面側の計測の結果を表にまとめる`・`9_枚の計測の行を読める`・`計測が動かせないときだけ終了コードが_0_でない`
  - 検証: `node --test scripts/perf-run.test.mjs && npm run app && npm run perf`

#### Phase 31b(逐次。T49a・T49b 完了後)

- [x] T49c — M1 の検証画面の撤去(画面とスモーク)(変更ファイル: src/ui/terminals/VerificationGrid.tsx(削除), src/ui/terminals/VerificationGrid.css(削除), src/logic/verifyGrid.ts(削除), src/logic/verifyGrid.test.ts(削除), src/ui/shell/AppShell.tsx, src/main.tsx, src/state/terminalSync.ts, src/state/terminalSync.test.ts, src/state/launchOptions.ts, src/state/launchOptions.test.ts, src/ipc/client.ts, src/ipc/client.test.ts, src/test/fakeIpc.ts, e2e/verify-grid.spec.ts(削除), e2e/fixtures/tauriMock.ts, scripts/smoke.mjs, scripts/smoke.test.mjs | 依存: T49a, T49b)
  - 対応: §リスク #10(検証用の仕組みをリリースビルドに残さない)、ADR-001「起動時の指定の扱い」、前提 (h)、【仮定】18
  - 変えるもの: 画面は `ARIN_VERIFY_GRID` を読まない(起動時の指定の `verifyGrid` を使わない。型の項目は T49d まで残る)。検証画面の部品・純粋ロジック・画面の自動テストを消す。スモークの `--env=ARIN_VERIFY_GRID=1` の代わりに、9 プロジェクトとタイル表示の `state.json` を置いて起動し、9 枚の準備完了と出力を確かめる `--tiles`(シェルがアプリの終了後に残らないことの確認を引き継ぐ)
  - 並行: 不可
  - 先に書くテスト: `scripts/smoke.test.mjs` に `タイルの指定で_9_プロジェクトとタイル表示の状態ファイルを置く`・`9_枚の印が揃うまで待つ`。Vitest: `起動時の指定に検証画面の値があっても通常の画面を出す`(`launchOptions.test.ts`)。既存の `e2e/tiles.spec.ts`・`e2e/m4-scenario.spec.ts` が通る
  - 検証: `npx vitest run src/state src/ipc && npx playwright test e2e/tiles.spec.ts e2e/app-shell.spec.ts && node --test scripts/smoke.test.mjs && npm run app && node scripts/smoke.mjs --tiles`

#### Phase 31c(逐次。T49c 完了後)

- [x] T49d — M1 の検証画面の撤去(Rust と結合テストの足場)(変更ファイル: crates/arin-core/src/launch_options.rs, crates/arin-core/src/workspace/state.rs, crates/arin-core/src/terminal/store.rs, crates/arin-core/tests/workspace_terminal.rs, crates/arin-core/tests/workspace_projects.rs, crates/arin-core/tests/app_snapshot.rs, crates/arin-core/tests/generated_types.rs, src/ipc/generated/, src-tauri/src/launch.rs, src-tauri/src/lib.rs, src-tauri/src/sessions.rs, src-tauri/src/persistence.rs, src-tauri/src/tree_watch.rs, src-tauri/tests/(検証画面を足場にしているもの。support/mod.rs を含む) | 依存: T49c)
  - 対応: §リスク #10、ADR-001、M2 の残りの問題「使われなくなった `terminal::TerminalSnapshot` と `LaunchOptions::terminal_count()`」
  - 変えるもの: `ENV_VERIFY_GRID`・`VERIFY_GRID_TERMINAL_COUNT`・`LaunchOptions` の検証画面の項目・`terminal_count()`・`Workspace::for_verify_grid`・持ち主の無いタブ(`TerminalStore` の `None` のプロジェクト)・`ExitPolicy::KeepTab`・`TerminalSnapshot` を消す。`launch.rs`・`lib.rs`・`persistence.rs`(読まず書かない)・`tree_watch.rs`(張らない)の分岐を消す。結合テストの足場を「一時フォルダのプロジェクト 1〜9 件」に置き換える(共通の補助を `support/mod.rs` に置く)。写しの `projects` の「検証画面では空」・`terminals` の「持ち主の無いもの」の説明を直す
  - 並行: 不可(`workspace/`・`sessions.rs`・`lib.rs`・生成物・多くの結合テストを触る)
  - 先に書くテスト: `起動時の指定に_arin_verify_grid_があっても通常の起動になる`(`launch.rs` の既存のテストを書き換える)、`写しのターミナルはすべて持ち主のプロジェクトを持つ`。既存の結合テストを新しい足場で通す(数が減らないこと。`cargo test -p arin` のテスト数を作業の前後で比べて報告する)
  - 検証: `npm test && npm run lint && npm run app && npm run smoke && node scripts/smoke.mjs --tiles`

#### Phase 31d(逐次。人間ゲート)

- [x] T50a — M4 の合否一覧と人間の確認(変更ファイル: output/reports/test/M4_acceptance.md, output/tasks/PROGRESS.md, output/design/ADR_011_tile-view.md | 依存: T49d)
  - AI がやること: `npm run build && npm test && npm run lint && npm run test:coverage && npm run e2e && npm run app && npm run smoke && node scripts/smoke.mjs --tiles && npm run perf` を通し、下の対応表に沿って AC の合否一覧を `output/reports/test/M4_acceptance.md` と `testreport/` に出す。NFR-009 の記録(T49b)を載せる(合否にしない)。人間に渡す確かめ方(9 プロジェクトの `state.json` の置き方、`ARIN_RENDERER=webgl`・`ARIN_UNICODE=graphemes` での起動の仕方)を書く。fresh context の `/code-review` と `verifier` を 1 回ずつ回す
  - **人間がやること**(`npm run app` で作った `.app` を Finder から開く): (1) **9 枚同時表示の体感**: 9 枚で Claude Code などを動かし、いくつかで `yes` などの大量の出力を流しながら、ページ送り・タイルのクリック・入力が我慢できる速さか(NFR-009。M4 の人間ゲート)。(2) **描画方式と全角幅の方式**: 既定(DOM、`unicode11`)と `ARIN_RENDERER=webgl`・`ARIN_UNICODE=graphemes` で同じことをして比べ、T49b の案を採るか選ぶ(ADR-001)。(3) タイル表示とタブ表示の切替(ボタンと ⇧⌘G)、再起動後に同じ表示で開く(FR-048)。(4) ページ送り(前後のボタンと ⌥⌘← ⌥⌘→)、10 枚で `1/2`(FR-052)。(5) 別のプロジェクトのタイルのクリックでカードとツリーが替わる(AC-M15)、カードを選ぶとそのページへ移る。(6) タイトルバーのダブルクリックと ⇧⌘↩ での拡大と戻し(FR-054)。(7) 隠れたタイル・別のページのタイルで `printf '\a'` を実行すると印が付き、表示すると消える(FR-055)。Claude Code の入力待ちで印が付くか(参考。§15.3 #12 は M6)。(8) ツリーからタイルへのドラッグと ⌘↩ の挿入の宛先(フォーカス中のタイル。別のプロジェクトのタイルへは絶対パス)。(9) タイルのタイトルバー・ページ表示・切替のボタン・印の見た目(ライトとダーク、すりガラスの上。デザインの方針)。(10) M4 で足した 4 つのキーが US と JIS の配列で効き、ターミナルに渡らない(【仮定】9。合わなければ表の既定のキーを替える)
  - 完了条件: AC の合否一覧ができ、本人が (1)〜(10) を確かめ、(2) で描画方式と全角幅の方式を選んだ

#### Phase 31e(逐次。T50a 完了後)

- [ ] T50b — 描画方式の決定の反映、`docs/` の同期、最終の検証(**AI の分は 2026-11-16 に済み**。残りは完了条件のうち本人の T50a の (1)〜(10) と ADR-011 の了承。M4.5 の T64a 以降はファイルの順の上では始めてよい)(変更ファイル: crates/arin-core/src/launch_options.rs, crates/arin-core/tests/generated_types.rs, src/ipc/generated/, src-tauri/src/launch.rs, src-tauri/src/lib.rs, src-tauri/src/commands/app.rs, src/ipc/client.ts, src/ipc/client.test.ts, src/state/launchOptions.ts, src/main.tsx, src/terminal/TerminalManager.ts, src/terminal/rendererControl.ts, src/ui/shell/RendererLabel.tsx(削除), src/ui/shell/AppShell.tsx, src/terminal/xterm/, e2e/terminal-webgl.spec.ts, e2e/app-shell.spec.ts, scripts/smoke.mjs, package.json, package-lock.json(選ばなかった方式のアドオンを外すときだけ), output/design/ADR_001_terminal-engine.md, output/design/ADR_011_tile-view.md, output/design/ARCH_multi-terminal.md, output/reports/test/M4_acceptance.md, output/tasks/PROGRESS.md, docs/, project-config.md | 依存: T50a)
  - 変えるもの: T50a の (2) の決定に従い、`ARIN_RENDERER`・`ARIN_UNICODE`・`get_launch_options`・`RendererLabel` を外し、選んだ方式に固定する。選ばなかった方式のアドオンを依存から外すか、M4.5 の設定ファイルの項目に回すかは、本人の判断に従う(本人の判断が要る点)。ADR-001 を決定で直し、ADR-011 の状態を本人の了承に合わせる。`docs/`(`state.json` の `ui.displayMode`、写しの `display`・`attention`、コマンドとメニュー、ショートカットの表の 22 行、`TerminalManager.present`、検証画面と起動時の指定を外したこと、新しい落とし穴)と `project-config.md` §2・§3 を同期する。設計書を直す: §5.2 の通信の一覧(表示)、§6.1(`PaneSizes` の行)、§6.2、§8、§9.1(メニューバーの「表示」)、§15.3 #1(描画方式の決定)
  - 先に書くテスト: `起動時の指定の環境変数があっても無視して既定の方式で起動する`(スモーク)。描画方式の切替のテスト(`e2e/terminal-webgl.spec.ts` など)は選んだ方式に合わせて直すか消す
  - 検証: `npm run build && npm test && npm run lint && npm run test:coverage && npm run e2e && npm run app && npm run smoke && node scripts/smoke.mjs --tiles`
  - 完了条件: 下の「M4 の完了条件」をすべて満たす

#### M4 の完了条件

- 下の対応表の AC が、自動テストと T50a の人間の確認で全件合格している
- `npm run build`・`npm test`・`npm run lint`・`npm run test:coverage`(`arin-core` と `src/logic` のそれぞれで行 80%)・`npm run e2e`(`e2e/m4-scenario.spec.ts` は 3 回続けて合格)・`npm run smoke`・`node scripts/smoke.mjs --tiles` が終了コード 0。`npm run perf` が NFR-009 を記録している(合否にしない)
- テストとスモークが、本物の `~/Library/Application Support/Arin/` と一時フォルダの外のファイルを作っていない・変えていない
- `cargo tree -i reqwest --target aarch64-apple-darwin` が空(通信のクレートが増えていない)
- 9 枚の検証画面(`ARIN_VERIFY_GRID`・`VerificationGrid`)と、起動時の指定 `ARIN_RENDERER`・`ARIN_UNICODE`・`RendererLabel` がソースに残っていない。ADR-001 に描画方式と全角幅の方式の決定がある
- `src-tauri/src/menu.rs` と `src/ui/` に、M4 で足したキーの文字列の直書きが無い(表の 22 行から引く。【要件の追加】)
- 本人が T50a の (1)〜(10) を確かめた。ADR-011 が本人の了承に合わせて直っている

#### FR の受け入れ基準とタスクの対応(漏れの確認用)

| FR | 受け入れ基準 | 規則 | コマンド・メニュー | 画面 | 人間(T50a) |
| -- | ------------ | ---- | ------------------ | ---- | ---------- |
| FR-048 | ボタンとショートカットのどちらでもタブ表示とタイル表示が切り替わる | T46a、T46b、T47a | T47b、T47c | T47e、T47f、T49a | (3)、(10) |
| FR-048 | 再起動後、終了時の表示モードで開く | T46c | T47b(`app_persistence.rs`) | T49a | (3) |
| FR-051 | 1→1×1、2→2×1、3〜4→2×2、5〜6→3×2、7〜9→3×3(列×行) | T46a | — | T47f | (1) |
| FR-051 | 全プロジェクトの全ターミナルがプロジェクト順→タブ順に並ぶ | T46b | — | T47f | — |
| FR-051 | タイル表示中もウィンドウのリサイズに追従し、各タイルの行・桁が PTY に通知される | — | 既存の `resize_terminal` | T47d、T47f | (1) |
| FR-052 | 10 枚で 1 ページ目に 9 枚(3×3)、2 ページ目に 1 枚、ページ表示が `1/2`(AC-M14) | T46a、T46b | — | T47e、T47f、T49a | (4) |
| FR-052 | 前後ボタンとショートカットでページが切り替わる | T46b、T47a | T47b、T47c | T47e、T47f | (4)、(10) |
| FR-052 | 表示していないページのプロセスと出力が継続する | — | — | T47d、T47f、T49a | (1) |
| FR-053 | A のタイルにフォーカス中に B のタイルをクリックすると、カードは B が選択状態、ツリーのルートが B(AC-M15) | T46b | T47b(監視の張り替え) | T47e、T47f、T49a | (5) |
| FR-053 | カードを選ぶと、そのプロジェクトの最初のタイルがあるページへ移る | T46b | — | T47f | (5) |
| FR-054 | タイルをダブルクリック(またはショートカット)すると、そのターミナルがタブ表示で拡大される | T46b、T47a | T47b、T47c | T47g、T49a | (6) |
| FR-054 | 同じ操作でタイル表示に戻る | T46b | T47b、T47c | T47g、T49a | (6) |
| FR-055 | アテンションのあるタイルのタイトルにバッジ(印)が付く | T48a、T48b | T48c | T47e、T47g、T49a | (7) |
| FR-055 | そのタイルが別ページにあるとき、ページ表示にも印が付く | T48b | T48c | T47e、T47g、T49a | (7) |
| FR-060 | A の色が緑のとき、A のカードのタイトル部と A の全タイルのタイトルバーが同じ色相で、他プロジェクトのタイルと違う(AC-C02) | — | — | T47f、T49a | (9) |
| FR-060 | タブ表示の着色(M2・M3 で済み)を壊さない | — | — | T47f(既存の `tabs.spec.ts`・`projects.spec.ts`) | (9) |
| FR-061 | タイルのタイトルに「プロジェクト名: タブ名」が必ず出る | — | — | T47e、T47f | (9) |
| NFR-009 | 9 枚同時に出力が流れていても操作が 100ms 以内に反応する(目安。記録だけ)。描画するのは表示中ページのタイルだけ | T46b(表示中の並び) | — | T47d、T47f、T49b | (1)、(2) |
| M3 の申し送り | 「パスをターミナルに挿入」の宛先をタイルのフォーカスにする | T46b(フォーカス=表示中のタブ) | 既存の `insert_entry_path` | T47g | (8) |
| M3 の残り | 切り替えたあとに届いた前のプロジェクトの応答で衝突の確認が開かない | — | — | T47e | — |
| ADR-001 | 描画方式と全角幅の方式を複数枚で実測して決め、起動時の指定と `RendererLabel` を外す | — | — | T49b、T50b | (2) |
| §リスク #10 | 9 枚の検証画面を撤去する | — | — | T49c、T49d | — |
| 【要件の追加】 | M4 のショートカットを 1 つの表から引く(M4.5 の土台) | T47a | T47c | T47f(ツールチップ) | (10) |

#### M3 までの残りの問題の割当(`M2_acceptance.md`・`M3_acceptance.md` の「判断が要る点・残っている問題」から、タイル表示に関わるもの)

| 項目 | 扱うタスク | 扱い |
| ---- | ---------- | ---- |
| 切り替えたあとに届いた前のプロジェクトの応答で、衝突の確認が開きうる(M3) | T47e | タイルのクリックでプロジェクトの切替が増えるので、M4 で直す |
| 使われなくなった `terminal::TerminalSnapshot` と `LaunchOptions::terminal_count()`(M2) | T49d | 検証画面の撤去と一緒に消す |
| 状態の写しが作った順と違う順で届くことがあり得る(M2) | 扱わない(注意だけ) | M4 で写しを送る回数が増える(フォーカス・ページ・アテンション)。表示が前の状態に戻る症状が T49a・T50a で出たら、写しに通し番号を足すタスクを起こす。出なければ M6(T55)の再分解のまま |
| タブの「+」の読み上げ用のキーの表示(`Meta+T`)が直書き(M3) | 扱わない | M4.5 のまま。M4 で足すツールチップのキーの表示は表から引く(T47f) |
| それ以外(ゴミ箱の目視、競合の隙間、「開く」の範囲、NFR-005、読み込み中が残る、Dock の終了など) | 扱わない | タイル表示に関わらない |

### M4.5 ショートカットの設定(T63 の着手時に 1 セッション粒度に分解し直した。2026-11-16)

**【要件の追加(本人。2026-11-15)】** 「ショートカットのキーを設定ファイル(`settings.toml`)で変えられるようにする」(本人は「ショートカットの設定もしくは説明はあるの?」に対し、一覧の画面・⌘1〜⌘9 の切替・キーを変えられる設定のうち「キーを変えられる設定」を選んだ)。PRD に無い要件なので、PRD への反映(FR の追加と AC)は本人の判断を待つ。仮の受け入れ基準は ADR-012 の AC-K01〜K10。M3・M4 で、キーを「コマンドの ID → 既定のキーと受け口」の 1 つの表(`arin-core/shortcuts`)から引く形にしてある(T38d・T41b・T42e・T47a)。設定ファイルの土台(M6 の T56 の一部)を前倒しする。

完了条件: ADR-012 の AC-K01〜K10(【要確認】の回答で外れたものを除く)が自動テストと T66c の本人の確認で合格し、下の「M4.5 の完了条件」をすべて満たす(対応表は節の末尾)。

#### M4.5 の前提

設計の決定は ADR-012(`output/design/ADR_012_keybindings.md`。Proposed)にまとめた。【要確認】の 4 点は**推奨案で計画を組んだ**。本人の回答が推奨と違えば、下の「回答が推奨と違うとき」のとおりにタスクを直す。

- (a) **置き場所**: `<状態フォルダ>/settings.toml`(既定は `~/Library/Application Support/Arin/settings.toml`)。`ARIN_STATE_DIR` で差し替えたら差し替え先の `settings.toml` だけを読む。パスは `AppPaths::settings_file()`(ADR-012 の 1)
- (b) **書式**: `[shortcuts]` の下に `"コマンドの ID" = "キーの文字列"`(`KeyChord` の書式)。変えたい行だけを書き、書かなかった行は既定。引用符の無い点つきの ID は 1 段だけつないで読む。M4.5 は `[shortcuts]` 以外の節を黙って無視する(ADR-012 の 2)
- (c) **検査**: 問題のある行はその行だけ既定に戻し、残りは使い、画面に 1 行で知らせる。TOML として読めないときは、起動時は既定、動作中は直前の表のまま。受け口は変えさせない。予約のキー・修飾キーの条件・重なりの規則は ADR-012 の 3 の表(規則は `arin-core/shortcuts` の純粋な関数)
- (d) **安全**: 設定ファイルは読むだけで、書き換えない・退避しない・消さない(例外は「設定ファイルを開く…」が**無いときだけ**見本を作ること。上書きしない)。知らせと標準エラー(`[arin:settings]`)に出すのはコマンドの ID・キーの文字列・行と列・理由の種類だけで、設定ファイルの行の全文・TOML のクレートの失敗の文言・絶対パスは出さない。テストとスモークは一時の状態フォルダ(`ARIN_STATE_DIR`)だけを使い、本物の設定ファイルを読み書きしない
- (e) **TOML の解析**は `arin-platform` に置き、`arin-core` には文字列の対とファイル全体の結果だけを渡す(`arin-core` の依存を増やさない)。クレートは `toml`(`Cargo.lock` に `toml 1.2.1+spec-1.1.0` が `tauri-utils 2.10.2` の依存として載っている)。版・公開日・ライセンス・macOS の依存の木に入るかは T65a で確かめる(ADR-012 の 4)
- (f) **メニューの差し替え**は、項目ごとの `set_accelerator` ではなく、`build_with_table` で作り直して `AppHandle::set_menu` で丸ごと替える(`set_accelerator` は読めない文字列を黙って捨て、macOS では `None` でキーを消さない。ADR-012 の 5 の「確かめたこと」)
- (g) **画面への知らせ**: 新しい型を足さない。Rust は「設定が変わった」イベント(`settings-changed`。中身なし)を送り、画面は `get_shortcuts` と、新しいコマンド `get_settings_notice`(1 行の文字列か null)を呼び直す。設定の知らせは状態ファイルの知らせ(写しの `notice`)と別に出す【仮定】
- (h) **T50b との順**: T50b(M4 の最後)は `src-tauri/src/lib.rs`・`src-tauri/src/commands/app.rs`・`src/ipc/client.ts`・`crates/arin-core/tests/generated_types.rs`・`src/ipc/generated/` を触る。M4.5 の実装(T64a 以降)は T50b の完了後に始める

【要確認】の回答が推奨と違うとき:

| 【要確認】 | 推奨(計画の前提) | 違う回答のときのタスクの直し |
| ---------- | ------------------ | ---------------------------- |
| 1. 割り当てを外す書き方 | A: 入れない(`""` は読めないキーとして既定に戻す) | B・C: 表の行のキーを「無し」を許す形にする。T64a(規則)・T64c(表と生成する型)・T65b(メニュー)・T65e(画面の振り分けとラベル)が広がる。T64a の前に型を変える子タスクを 1 つ足す |
| 2. ⌘C・⌘X・⌘V(「編集」の標準の項目) | 変更の対象から外し、書かれたら無視して知らせる | 変えられるようにする: 標準の項目を自前の項目に替え、画面のコピー・切り取り・ペーストの振り分けを作り直すタスクを T65b のあとに足す(ターミナルとツリーのコピーの既存のテストを通し直す) |
| 3. 反映のタイミング | B: 保存を検知して即時(T65c) | C: T65c を「メニューの『設定を読み直す』で読み直す」に替える(監視は作らない)。A: T65c を外す(AC-K07 も外す) |
| 4. 説明の置き場所 | A: メニューに「設定ファイルを開く…」(⌘,)。無ければ見本を作って開く | B: T64c(表の 23 行目と見本)と T65d を外す。説明は T66b の `docs/` だけ。AC-K08 を外す |

#### 現状のコードから分かった、M4.5 で足すところ・変えるところ

- `arin-core/shortcuts` は、表の検査(`from_bindings`)まで。設定の値を当てる口・予約のキー・修飾キーの条件・問題の一覧が無い → T64a。表は 22 行 → 「設定ファイルを開く」で 23 行(T64c)
- `arin-core` に `settings` が無い(`lib.rs` に `pub mod settings;` が無い)。`ports` に `SettingsFilePort` が無い。`AppPaths` は状態フォルダだけを返す → T64b
- `arin-platform/src/storage/` は `ready_marker.rs`・`state_file.rs` だけ(`mod.rs` の説明は「設定ファイルの読み書きは M5 で足す」と古い)。TOML のクレートは `arin-platform` の依存に無い → T65a
- `menu.rs` の `Shortcuts` は表を値で持ち、`lib.rs` が `.manage(Shortcuts::default())` で登録する。差し替える口が無い。メニューはアプリ名・ターミナル・編集・表示で、アプリ名のメニューは標準の「隠す」「ほかを隠す」「すべてを表示」と自前の「終了」 → T65b(読み込みと差し替えの口)、T65d(「設定ファイルを開く…」)
- `get_shortcuts` は起動時に画面が 1 回呼ぶだけ(`src/state/startApp.ts`)。表が変わったことを画面に伝えるイベントが無い。`keyOwner` は `main.tsx` で `ui.getState().shortcuts` を毎回読むので、`uiStore` の表を替えれば追従する → T65e
- 知らせは写しの `notice`(`Workspace` が持つ 1 つ。状態ファイルの退避用)だけで、`NoticeBar` は 1 行。設定の知らせを出す口が無い → T65b(Rust)、T65e(画面)
- `src/ui/terminals/TabBar.tsx` の「+」の `aria-keyshortcuts="Meta+T"` と `title="新規ターミナル(⌘T)"` が直書き(M3 の残り。M4 の割当で M4.5 に回した)。表から読み上げ用の書式(`Meta+Shift+T`)を作る関数が無い → T65e
- ツリーの監視(`tree_watch.rs`)は選択中のプロジェクトのルートに 1 本。状態フォルダの監視は無い。`WatcherPort`(`FsEventsWatcher`)はフォルダに再帰で張る口で、そのまま使える → T65c
- 「既定のアプリで開く」「Finder で表示」は M3 の `DesktopPort`(`open_with_default_app`・`reveal_in_finder`)にある → T65d
- スモーク(`scripts/smoke.mjs`)は一時の状態フォルダに `state.json` を置ける(`--tiles`)が、`settings.toml` を置く指定が無い → T66a

#### 親タスクと子タスク

T64〜T66 は親として残し、作業は子タスクで行う。親は子がすべて済んだら済みにする。

| 親 | 子 |
| -- | -- |
| T64 — 規則(`arin-core`) | T64a(設定の値を表に当てる規則と問題の一覧)、T64b(設定ファイルの結果の型・ポート・置き場所)、T64c(表に「設定ファイルを開く」の行と見本の文) |
| T65 — 読み込みとメニュー・画面への反映 | T65a(設定ファイルの読み込みと TOML の解析。`arin-platform`)、T65b(起動時に読んでメニューと `get_shortcuts` に反映し知らせる)、T65c(保存の検知と即時の反映)、T65d(「設定ファイルを開く…」)、T65e(画面: 届いた表と知らせへの追従、`TabBar` の直書き) |
| T66 — 自動テストと M4.5 の完了確認 | T66a(シナリオの画面の自動テストと、設定ファイルを置いたスモーク)、T66b(文書の同期と設定の書き方の説明)、T66c(合否一覧と本人の確認)、T66d(本人の確認の反映と最終の検証) |

粗い分解のときの親の行(内容は子に引き継いだ。消さずに残す):

- [ ] T64 — 規則(`arin-core/settings` の最小と `arin-core/shortcuts` のマージ)(変更ファイル: crates/arin-core/src/settings/, crates/arin-core/src/shortcuts/, crates/arin-core/tests/ | 依存: T63)
  - 対応: 【要件の追加】、FR-057 の土台(T56 から前倒し)、設計書 §7.2「設定ファイル」。検証: `npm test && npm run test:coverage` → **T64a〜T64c**
- [ ] T65 — 読み込みとメニュー・画面への反映(変更ファイル: crates/arin-platform/src/storage/, src-tauri/src/menu.rs, src-tauri/src/lib.rs, src-tauri/src/commands/app.rs, src/ipc/client.ts, src/state/, src/ui/ | 依存: T64)
  - 対応: 【要件の追加】、FR-057 の土台、設計書 §1.3 #10。安全: 設定ファイルは読むだけ、テストは一時の状態フォルダの中だけ。検証: `npm test && npm run lint && npm run app && npm run smoke` → **T65a〜T65e**
- [ ] T66 — 自動テストと M4.5 の完了確認(変更ファイル: e2e/, scripts/smoke.mjs, output/tasks/PROGRESS.md, output/reports/test/, docs/, project-config.md | 依存: T65)
  - **人間**: `settings.toml` でキーを変え、メニューバーとツリーの両方で効くこと、不正な値で理由が出て既定に戻ること、行を消すと既定に戻ることを確かめる → **T66a〜T66d**

#### 共有のファイルの持ち回り

同じ Phase で 2 つのタスクが同じファイルを触らないように割り当てた。下のファイルは表の順に逐次で触る。同時に動かすタスクは、どの Phase も 2 件まで(このマシンは並行のビルドとテストでメモリが足りなくなったため)。`src-tauri` を触るタスク(T65b〜T65d)は同時に動かさない(`cargo` のビルドが重いため)。

| ファイル | 触るタスク(この順) |
| -------- | ------------------ |
| `crates/arin-core/src/shortcuts/mod.rs`、`crates/arin-core/src/shortcuts/overrides.rs`、`crates/arin-core/src/shortcuts/reserved.rs`、`crates/arin-core/tests/shortcut_overrides.rs` | T64a → T64c(`mod.rs` の公開と、見本の検査のテストを足すだけ) |
| `crates/arin-core/src/shortcuts/table.rs`、`crates/arin-core/src/shortcuts/sample.rs`、`crates/arin-core/tests/shortcuts.rs` | T64c だけ |
| `crates/arin-core/src/lib.rs`、`crates/arin-core/src/settings/`、`crates/arin-core/src/ports/mod.rs`、`crates/arin-core/src/ports/settings_file.rs`、`crates/arin-core/tests/settings_load.rs` | T64b だけ |
| `crates/arin-core/src/paths/app_paths.rs`、`crates/arin-core/tests/app_paths.rs` | T64b だけ |
| `crates/arin-core/tests/generated_types.rs`、`src/ipc/generated/` | T50b(M4)→ T64c |
| `Cargo.toml`(ワークスペースの依存)、`Cargo.lock`、`crates/arin-platform/Cargo.toml` | T65a だけ |
| `crates/arin-platform/src/storage/`(`mod.rs`・`settings_file.rs`)、`crates/arin-platform/tests/settings_file.rs` | T65a だけ |
| `src-tauri/src/settings.rs` | T65b → T65c → T65d |
| `src-tauri/src/settings_watch.rs`、`src-tauri/tests/settings_watch.rs`、`src-tauri/src/lifecycle.rs` | T65c だけ |
| `src-tauri/src/lib.rs`、`src-tauri/src/events.rs`、`src-tauri/src/commands/app.rs`、`src-tauri/src/commands/mod.rs`、`src-tauri/Cargo.toml`(`[[test]]`) | T50b(M4)→ T65b → T65c(`lib.rs`・`Cargo.toml` だけ) |
| `src-tauri/src/menu.rs`、`src-tauri/tests/app_menu.rs` | T65b → T65d |
| `src-tauri/tests/app_settings.rs` | T65b → T65d |
| `src/logic/keys/`(`defaultShortcuts.ts`・`keyOwner.test.ts`・`shortcutTable.test.ts`) | T64c → T65e(`shortcutTable.ts`・`shortcutTable.test.ts`・`keyOwner.test.ts`) |
| `src/ipc/client.ts`、`src/ipc/client.test.ts` | T50b(M4)→ T65e |
| `src/state/`(`startApp.ts`・`uiStore.ts` とテスト) | T65e だけ |
| `src/ui/terminals/TabBar.tsx`、`src/ui/shell/AppShell.tsx`、`src/ui/shell/NoticeBar.tsx`、`src/ui/shell/AppShell.css` | T50b(M4。`AppShell.tsx`)→ T65e |
| `src/test/fakeIpc.ts` | T65e だけ |
| `e2e/fixtures/tauriMock.ts` | T65e → T66a |
| `e2e/settings.spec.ts` | T65e だけ |
| `e2e/m45-scenario.spec.ts` | T66a だけ |
| `scripts/smoke.mjs`、`scripts/smoke.test.mjs` | T50b(M4)→ T66a |
| `scripts/check-boundaries.mjs` | M4.5 では触らない見込み(`arin-core` の依存は増やさない) |
| `output/design/ADR_012_keybindings.md` | T63 → T65a(クレートの版と確かめた結果の追記)→ T66b → T66d(状態を本人の了承に合わせる) |
| `output/design/ARCH_multi-terminal.md`、`output/design/ADR_005_settings-format.md`、`docs/`、`project-config.md` | T66b → T66d(本人の確認で変わった点だけ) |

#### Phase 31.5a(逐次。文書だけなので T50b を待たずに行った)

- [x] T63 — M4.5 の着手時の確認と再分解、ADR(変更ファイル: output/tasks/PLAN_multi-terminal.md, output/design/ADR_012_keybindings.md, output/design/ADR_INDEX.md | 依存: T50)
  - 決めたこと(ADR-012): (1) 置き場所と名前(状態フォルダの `settings.toml`、`ARIN_STATE_DIR` の差し替えに付いていく、ほかの設定と同じファイルに節で分ける)。(2) TOML のクレートと置き場所(`arin-platform` で解析し `arin-core` には文字列の対を渡す。`toml` を推奨。版は T65a で確かめる)。(3) 書式(`[shortcuts]` の下に ID = キーの文字列。外す書き方は【要確認】)。(4) 検査と失敗の扱い(行ごとに既定に戻す、予約のキー、修飾キーの条件、重なり、⌘C・⌘X・⌘V は【要確認】、ファイル全体の問題、退避しない)。(5) 反映のタイミング(【要確認】。推奨は保存を検知して即時。メニューは `set_menu` で作り直す)。(6) 既定に戻す方法(行を消す)。(7) 受け口は設定で変えさせない。(8) キーを知る手段と説明の置き場所(【要確認】。推奨は「設定ファイルを開く…」と見本)。(9) T56 との分担。仮の受け入れ基準 AC-K01〜K10
  - PRD への反映が要る点(本人の判断): FR の追加(例: FR-065「ショートカットのキーを設定ファイルで変えられる」)と AC-K01〜K10 の採否、M4.5 という区切りを PRD §9 に足すか
  - 設計書(`ARCH_multi-terminal.md`)は T66b でまとめて直す(本人の回答を待ってから直すため)
  - 検証: 文書だけ。`ls output/design/ADR_012_*.md` が 1 件(済み)

#### Phase 31.5b(並行可能。T63・T50b 完了後)

- [x] T64a — 設定の値を表に当てる規則と問題の一覧(`arin-core/shortcuts`)(変更ファイル: crates/arin-core/src/shortcuts/mod.rs, crates/arin-core/src/shortcuts/overrides.rs, crates/arin-core/src/shortcuts/reserved.rs, crates/arin-core/tests/shortcut_overrides.rs | 依存: T63, T50b)
  - 対応: 【要件の追加】、ADR-012 の 2・3、AC-K03・AC-K04・AC-K09 の規則の部分、設計書 §7.2「設定ファイル」
  - 置くもの: 入力 `ShortcutOverride { id: String, value: OverrideValue }`(`OverrideValue` は文字列か「文字列でない」の印。`settings` の型に依存しない=機能モジュールどうしを直接つながない。変換は T65b)。`apply_overrides(既定の表, &[ShortcutOverride]) -> (ShortcutTable, Vec<ShortcutSettingProblem>)`: ADR-012 の 3 の表の #1〜#10 を当てる。重なりは「関わる設定の行をすべて既定に戻す」を無くなるまで繰り返す。結果の表は必ず `from_bindings` を通る。問題は表の順(知らない ID は書いた順で最後)に並べる。`reserved.rs` に予約のキーの一覧(ADR-012 の 3)。知らせの 1 行と、標準エラー用の問題ごとの 1 行を作る関数(ID とキーの文字列は制御文字を除き 64 文字で切る)。入出力・時計を持たない
  - 安全: 知らせに出すのはコマンドの ID・キーの文字列・理由の種類だけ
  - 並行: 可(T64b とファイルが重ならない)
  - 先に書くテスト: `設定が空なら既定の表と同じで問題が無い`、`書いた行だけキーが替わり書かなかった行は既定のまま`(AC-K01・K03)、`正規の書式でない文字列も同じキーとして読む`(`shift+cmd+t`)、`読めないキーの行は既定に戻り理由の種類が残る`、`空の文字列は読めないキーとして既定に戻す`(【要確認】1 の A)、`文字列でない値の行は既定に戻す`、`知らないコマンドの_id_は無視し問題に残す`、`id_は大文字と小文字を区別する`、`切り取り_コピー_ペーストは変えずに問題に残す`(【要確認】2)、`予約のキーの行は既定に戻す`(一覧の全件を表で)、`既定の表は予約のキーを使わない`、`メニューの項目の行で_cmd_を含まないキーは既定に戻す`(`Ctrl+C`・`Alt+N`・`Shift+T`・`T` を表で)、`画面の行は修飾キーの無い_enter_space_backspace_delete_を許す`、`画面の行で修飾キーの無い文字_矢印_tab_escape_は既定に戻す`、`既定の行と重なる設定の行は既定に戻し相手のコマンドを問題に残す`、`設定の行どうしが重なると両方を既定に戻す`、`既定に戻したことで生じた重なりも解ける`、`両方を書けばキーを入れ替えられる`、`同じコマンドを_2_度書くと両方を既定に戻す`、`結果は書いた順によらない`(並べ替えのすべてで同じ)、`結果の表は必ず表の検査を通る`、`終了のキーは_cmd_を含めば変えられる`、`知らせは_1_行で_id_とキーの文字列と理由だけ`(AC-K09)、`知らせの_id_とキーは制御文字を除き_64_文字で切る`、`問題が無ければ知らせは無い`
  - 検証: `cargo test -p arin-core --test shortcut_overrides --test shortcuts`
- [x] T64b — 設定ファイルの結果の型・ポート・置き場所(`arin-core/settings`・`ports`・`paths`)(変更ファイル: crates/arin-core/src/lib.rs, crates/arin-core/src/settings/mod.rs, crates/arin-core/src/settings/load.rs, crates/arin-core/src/ports/mod.rs, crates/arin-core/src/ports/settings_file.rs, crates/arin-core/src/paths/app_paths.rs, crates/arin-core/tests/settings_load.rs, crates/arin-core/tests/app_paths.rs | 依存: T63, T50b)
  - 対応: FR-057 の土台(T56 から前倒し)、ADR-005、ADR-012 の 1・3(ファイル全体の問題)・7、設計書 §5.1(`SettingsFilePort`)、§6.1(`SettingsStore`)、§7.2、AC-K03・AC-K05・AC-K10 の規則の部分
  - 置くもの: `SETTINGS_FILE_NAME = "settings.toml"`、`AppPaths::settings_file()`(状態フォルダの直下。差し替えたら差し替え先)。`SettingsFilePort`(読む口: 無い / 中身のバイト列 / 読めない理由。見本を**無いときだけ**作る口: 作った / もうある。上書きする口・改名する口・消す口は**作らない**)。読んだ結果の型 `SettingsLoad`(無い / 読めた=節ごとの「キーと値」の並び / 読めない理由=UTF-8 でない・大きすぎる・ファイルでない・入出力 / 構文エラー=行と列)。値は文字列か「文字列でない」の印。`SettingsStore` の最小(使っている内容と、いまの問題。起動時の構文エラーは空、動作中の構文エラーは直前の内容を保つ。ファイルが消えたら空)。ファイル全体の問題の知らせの 1 行(行と列・理由の種類だけ)。M4.5 で読む節は `shortcuts` だけで、ほかの節は持たない(黙って無視)
  - 安全: 入出力を持たない。知らせに中身を出さない
  - 並行: 可(T64a とファイルが重ならない。`lib.rs` を触るのは Phase 31.5b ではこのタスクだけ)
  - 先に書くテスト: `状態フォルダの直下の_settings_toml_を指す`、`差し替えた状態フォルダでは差し替え先の_settings_toml_を指す`(AC-K10)、`ファイルが無いときは空で知らせが無い`(AC-K03)、`構文エラーの起動時は空で行と列を知らせる`(AC-K05)、`構文エラーの読み直しでは直前の内容を保つ`(設計書 §7.2)、`構文エラーを直して読み直すと新しい内容になり知らせが消える`、`ファイルが消えたら空に戻る`、`読めない理由ごとの知らせ`(表で)、`shortcuts_が表でないときはその節だけ空にして知らせる`、`知らない節は黙って無視する`、`知らせに中身の文字列が入らない`
  - 検証: `cargo test -p arin-core --test settings_load --test app_paths`

#### Phase 31.5c(並行可能。T64a・T64b 完了後)

- [x] T64c — 表に「設定ファイルを開く」の行と見本の文(変更ファイル: crates/arin-core/src/shortcuts/table.rs, crates/arin-core/src/shortcuts/sample.rs, crates/arin-core/src/shortcuts/mod.rs, crates/arin-core/tests/shortcuts.rs, crates/arin-core/tests/shortcut_overrides.rs, crates/arin-core/tests/generated_types.rs, src/ipc/generated/, src/logic/keys/defaultShortcuts.ts, src/logic/keys/keyOwner.test.ts, src/logic/keys/shortcutTable.test.ts | 依存: T64a)
  - 対応: ADR-012 の 6(【要確認】4 の A)、AC-K08 の規則の部分
  - 足す行: `app.open-settings` = `Cmd+,`(受け口は「メニューの項目」)。`ShortcutCommandId::ALL` は 23 行になる。生成物(`ShortcutCommandId.ts`・`defaultShortcuts.json`)と画面の見本(`defaultShortcuts.ts`)を作り直す
  - 置くもの: `sample_settings_toml(既定の表) -> String`(見本の文。先頭に書き方の決まり・変えられないコマンド・予約のキー・効かないときの確かめ方・リンクの先の変更は拾えないことをコメントで書き、`[shortcuts]` の下に全コマンドの `# "ID" = "既定のキー"` をコメントで並べる)
  - 並行: 可(T65a とファイルが重ならない。生成物を触るのは Phase 31.5c ではこのタスクだけ)
  - 先に書くテスト: 既存の `すべてのコマンドの_id_が既定のキーを持つ`・`同じフォーカスで受けるキーに重なりが無い` が 23 行で通る、`設定ファイルを開くの既定は_cmd_カンマで受け口はメニューの項目`、`見本はすべてのコマンドの_id_と既定のキーをコメントで並べる`、`見本の行はコメントか_shortcuts_の見出しだけ`、`見本に変えられないコマンドと予約のキーが書いてある`、`見本の行のコメントを外しても検査で問題が出ない`(各行を `apply_overrides` に通す。`shortcut_overrides.rs`)、`表の_json_の形`(生成物が最新)。Vitest: `設定ファイルを開くキーはターミナルに渡さない`(`keyOwner.test.ts`)、`既定の表に_23_行がある`(`shortcutTable.test.ts`)
  - 検証: `cargo test -p arin-core --test shortcuts --test shortcut_overrides --test generated_types && npx vitest run src/logic/keys && node --test scripts/default-shortcuts.test.mjs`
- [x] T65a — 設定ファイルの読み込みと TOML の解析(`arin-platform`)(変更ファイル: Cargo.toml, Cargo.lock, crates/arin-platform/Cargo.toml, crates/arin-platform/src/storage/mod.rs, crates/arin-platform/src/storage/settings_file.rs, crates/arin-platform/tests/settings_file.rs, output/design/ADR_012_keybindings.md | 依存: T64b)
  - 対応: FR-057 の土台、ADR-005 の「未定の点」、ADR-012 の 1・2・4、AC-K05・AC-K09・AC-K10 の読み込みの部分
  - 着手時に確かめる(ADR-012 の 4。結果を ADR-012 の末尾に追記する): `cargo tree -i toml --target aarch64-apple-darwin` で `toml 1.2.1+spec-1.1.0` が macOS の依存の木に入っているか、`cargo info toml` と crates.io でその版の公開日(公開から 2 週間未満を避ける)とライセンス、使う機能が `tauri-utils` の有効にしている範囲に収まるか。版は `=` で固定する(ワークスペースの依存に足す)。入っていない・2 週間未満なら、止めて報告する(別の版を推測で選ばない)
  - 置くもの: `FsSettingsFile`(`SettingsFilePort` の実装。読む: シンボリックリンクの先を読む、ファイルでなければ「ファイルでない」、1 MiB を超えれば中身を読まずに「大きすぎる」、UTF-8 でなければ「UTF-8 でない」。見本を作る: `create_new`(同じ名前があれば作らない。シンボリックリンクがあっても作らない)・0600・`fsync`。改名・削除・上書きの口は持たない)。TOML の文字列を `SettingsLoad` にする関数: `[shortcuts]` の文字列の値はそのまま、入れ子の表は 1 段だけ `.` でつなぐ(`[shortcuts.terminal]` も同じ)、2 段以上と文字列でない値は「文字列でない」の印、ほかの節は持たない。構文エラーはクレートが返す位置から行と列を数え、クレートの失敗の文言は持たない。`storage/mod.rs` の古い説明(「設定ファイルの読み書きは M5 で足す」)を直す
  - 安全: 読むだけ(見本は無いときだけ作る)。テストは一時フォルダの中だけで、本物の `~/Library/Application Support/Arin/` を読み書きしない
  - 並行: 可(T64c とファイルが重ならない。`Cargo.lock` を触るのはこのタスクだけ)
  - 先に書くテスト(一時フォルダ): `無いときは無いと返す`、`読んでも中身と更新日時が変わらない`(AC-K05)、`シンボリックリンクの先を読む`、`フォルダならファイルでないと返す`、`1_MiB_を超えると中身を読まずに大きすぎると返す`、`utf_8_でないと返す`、`読む権限が無いと入出力の失敗を返す`、`shortcuts_の文字列の行を書いた順に返す`、`引用符の無い点つきの_id_を_1_段つないで読む`、`shortcuts_terminal_の節の行も同じ_id_で読む`、`2_段以上の入れ子と文字列でない値は文字列でないの印になる`、`ほかの節は返さない`、`構文エラーは行と列だけを返し本文を含まない`(結果の `Debug` の文字列に該当行の文字列が入らない。AC-K09)、`同じキーを_2_度書くと構文エラー`、`無いときだけ見本を作り_0600`、`あるときは見本を作らず中身と更新日時を変えない`、`シンボリックリンクがあると見本を作らない`
  - 検証: `cargo test -p arin-platform --test settings_file && cargo clippy -p arin-platform -- -D warnings && cargo tree -i reqwest --target aarch64-apple-darwin`(最後は空であること)

#### Phase 31.5d(逐次。T64c・T65a 完了後。`src-tauri/src/lib.rs` と `menu.rs` を触る)

- [x] T65b — 起動時に読んでメニューと `get_shortcuts` に反映し、知らせる(変更ファイル: src-tauri/src/settings.rs, src-tauri/src/menu.rs, src-tauri/src/lib.rs, src-tauri/src/events.rs, src-tauri/src/commands/app.rs, src-tauri/src/commands/mod.rs, src-tauri/Cargo.toml, src-tauri/tests/app_settings.rs, src-tauri/tests/app_menu.rs | 依存: T64a, T64b, T64c, T65a)
  - 対応: 【要件の追加】、AC-K01・K02・K03・K04・K05・K09・K10 の Rust 側、ADR-012 の 3・5(差し替えの口)、前提 (f)(g)
  - 置くもの: `settings.rs`(`FsSettingsFile` で読み → `SettingsStore` → `[shortcuts]` の行を `ShortcutOverride` に直して `apply_overrides` → 表と知らせ。知らせはファイル全体の問題と行の問題をまとめて 1 行。標準エラーには `[arin:settings]` で問題ごとに 1 行)。起動時はメニューを作る前に読む(`lib.rs`。`.manage(Shortcuts::default())` を読んだ表に替える)。`Shortcuts` は表を差し替えられる形にする(`RwLock` など。`table()` は写しを返す)。`menu::rebuild(app)`(`build_with_table` で作り直して `set_menu`。メインスレッドで作る。T65c・T65d が使う)。コマンド `get_settings_notice`(1 行か null)、イベント名 `settings-changed`(中身なし。`events.rs`)
  - 安全: 設定ファイルは読むだけ。知らせと標準エラーに中身・絶対パスを出さない。状態ファイルの退避の仕組み(`persistence.rs`)に設定ファイルを渡さない
  - 並行: 不可(`lib.rs`・`menu.rs`)
  - 先に書くテスト(模擬ランタイム。`harness = false` の `[[test]]` を足す。一時の状態フォルダ): `設定ファイルが無いと既定の表でメニューを作り知らせが無い`(AC-K03)、`設定で替えたキーがメニューの項目のアクセラレータと_get_shortcuts_の両方に出る`(AC-K01 の Rust 側。muda の項目のキーを読む既存の補助を使う)、`画面で受けるコマンドのキーも_get_shortcuts_に出る`(AC-K02 の Rust 側)、`問題のある行は既定に戻り_get_settings_notice_に_1_行出る`(AC-K04)、`構文エラーなら既定の表で行と列を知らせる`(AC-K05)、`起動しても設定ファイルの中身と更新日時が変わらず退避のファイルもできない`(AC-K05)、`差し替えた状態フォルダの設定だけを読む`(AC-K10)、`標準エラーの行は_arin_settings_で始まり設定の行の全文を含まない`(行を作る関数で。AC-K09)、`表を替えて作り直すとメニューのアクセラレータが替わり項目の_id_は変わらない`(`menu::rebuild`)。既存の `app_menu.rs`・`app_commands.rs` のテストがそのまま通る
  - 検証: `cargo test -p arin --test app_settings --test app_menu --test app_commands`

#### Phase 31.5e(並行可能。T65b 完了後。Rust と画面で 1 件ずつ)

- [x] T65c — 保存の検知と即時の反映(【要確認】3 の B)(変更ファイル: src-tauri/src/settings_watch.rs, src-tauri/src/settings.rs, src-tauri/src/lib.rs, src-tauri/src/lifecycle.rs, src-tauri/Cargo.toml, src-tauri/tests/settings_watch.rs | 依存: T65b)
  - 対応: AC-K05(動作中の構文エラー)・AC-K07、設計書 §1.3 #10、ADR-012 の 5
  - 置くもの: 専用のスレッド 1 本(`arin-settings-watch`)。`WatcherPort` で状態フォルダに張り(無ければ 0700 で作る。設定ファイルは作らない)、届いたパスの名前が `settings.toml` のときだけ読み直す(`state.json` などは捨てる)。結果の表と知らせが今と同じなら何もしない。違えば `Shortcuts` を差し替え、`menu::rebuild`、`settings-changed` を送る。構文エラーは直前の表のまま知らせだけ替える(`SettingsStore`)。終了のとき(`lifecycle.rs`)にスレッドを止めて待つ。`Workspace` のロックは取らない
  - 安全: 読むだけ。張れなかったときは落とさず、標準エラーに `[arin:settings]` で 1 行(パスを出さない)
  - 並行: 可(T65e とファイルが重ならない)
  - 先に書くテスト(偽の監視のポート + 一時フォルダ。`harness = false`): `settings_toml_の変更でキーが替わり_settings_changed_が届く`(AC-K07 の Rust 側)、`state_json_の変更では読み直さない`、`同じ内容で保存し直しても送らない`、`構文エラーに直すと直前の表のまま知らせだけが替わる`(AC-K05)、`ファイルを消すと既定の表に戻る`(AC-K03)、`状態フォルダが無ければ_0700_で作ってから張り設定ファイルは作らない`、`終了のときにスレッドが抜ける`。実 FSEvents(上限時間を付ける): `一時ファイルに書いて改名して保存しても読み直す`(エディタの保存の形)
  - 検証: `cargo test -p arin --test settings_watch --test app_settings`
- [x] T65e — 画面: 届いた表と知らせへの追従、`TabBar` の直書き(変更ファイル: src/ipc/client.ts, src/ipc/client.test.ts, src/state/startApp.ts, src/state/startApp.test.ts, src/state/uiStore.ts, src/state/uiStore.test.ts, src/logic/keys/shortcutTable.ts, src/logic/keys/shortcutTable.test.ts, src/logic/keys/keyOwner.test.ts, src/ui/terminals/TabBar.tsx, src/ui/shell/AppShell.tsx, src/ui/shell/AppShell.css, src/ui/shell/NoticeBar.tsx, src/test/fakeIpc.ts, e2e/fixtures/tauriMock.ts, e2e/settings.spec.ts | 依存: T65b)
  - 対応: AC-K01・K02(画面の表示)・K04(知らせ)・K06・K07(画面側)、M3 の残り「タブの『+』の読み上げ用のキーの表示(`Meta+T`)が直書き」
  - 置くもの: 包み `getSettingsNotice`・`onSettingsChanged`。起動時に表と設定の知らせを受け取り、`settings-changed` が届いたら両方を取り直して `uiStore` を置き換える(取り直せなければ今の表のまま)。`ariaKeyShortcuts(表, コマンド)`(`Cmd+Shift+T` → `Meta+Shift+T`。`Alt` → `Alt`、`Ctrl` → `Control`)。`TabBar` の「+」の `aria-keyshortcuts` と `title` を表から引く(`shortcutLabel`)。設定の知らせを状態の知らせとは別の 1 行で出す(閉じるのも別)。偽の Rust 側に `get_settings_notice` と `settings-changed` を足す
  - **デザイン**(本人の指示): 「AI にありがちなデザインを避け、Apple のデザインらしく、ハイセンスに」。知らせの 2 行目は今の `NoticeBar` と同じ控えめな形(システムフォント 13px、細い区切り線、閉じるのはアイコンのボタン)。Playwright の画像(ライトとダーク、知らせ 1 行と 2 行)を撮り、自分の目で確かめてから終える(画像は `testreport/`)
  - 安全: 知らせの文は文字として差し込む
  - 並行: 可(T65c とファイルが重ならない)
  - 先に書くテスト: Vitest: `設定が変わったと届くと表と知らせを取り直して置き換える`、`取り直せないときは今の表のまま`、`読み上げ用のキーの表示は_meta_の形`(表で)、`設定の知らせは状態の知らせと別に持ち別に閉じられる`、`表を替えると新しいメニューのキーはターミナルに渡さず古いキーは渡す`(AC-K06。`keyOwner.test.ts`)。Playwright(`e2e/settings.spec.ts`): `タブバーの新規のボタンのツールチップと読み上げのキーが表から出る`(AC-K02)、`表が替わったと届くとツールチップが替わる`(AC-K07 の画面側)、`状態の知らせと設定の知らせが_2_行出て別々に閉じられる`、`設定で替えたツリーのキーで新規ファイルの入力が開き古いキーでは開かない`(AC-K02)。既存の `e2e/tabs.spec.ts`・`e2e/tree-ops.spec.ts` が通る
  - 検証: `npx tsc -b && npx vitest run src/ipc src/state src/logic/keys && npx playwright test e2e/settings.spec.ts e2e/tabs.spec.ts e2e/tree-ops.spec.ts`

#### Phase 31.5f(逐次。T65c・T65e 完了後。T65 の節目)

- [x] T65d — 「設定ファイルを開く…」(【要確認】4 の A)(変更ファイル: src-tauri/src/menu.rs, src-tauri/src/settings.rs, src-tauri/tests/app_menu.rs, src-tauri/tests/app_settings.rs | 依存: T65c, T65e)
  - 対応: AC-K08、ADR-012 の 6
  - 置くもの: アプリ名のメニューの先頭に「設定ファイルを開く…」(項目の ID は `app.open-settings`、キーは表から)と区切り線。選ばれたら待ってよいスレッドで、`settings.toml` が無ければ見本(`sample_settings_toml`)を作り(無いときだけ)、`DesktopPort::open_with_default_app` で開く。開けなければ `reveal_in_finder` で Finder に出し、設定の知らせで 1 行知らせる(「設定ファイルを開けるアプリが無いため、Finder で表示しました」【仮定】)。見本を作れなければ開かずに知らせる
  - 安全: 既にある設定ファイルの中身を変えない。見本は上書きしない
  - 並行: 不可
  - 先に書くテスト(模擬ランタイム、偽の `DesktopPort`、一時の状態フォルダ): `アプリ名のメニューの先頭に設定ファイルを開くがあり表のキーが付く`、`見出しの並びと既存の項目は変わらない`、`無いときは見本を作って開く`(AC-K08)、`あるときは中身と更新日時を変えずに開く`(AC-K08)、`開けないと_finder_で表示し知らせる`、`見本を作れないときは開かず知らせる`
  - 検証: `cargo test -p arin --test app_menu --test app_settings`。節目として続けて `npm run lint && npm test`(並行のタスクを動かさずに 1 件で回す)

#### Phase 31.5g(並行可能。T65d 完了後)

- [x] T66a — M4.5 のシナリオの画面の自動テストと、設定ファイルを置いたスモーク(変更ファイル: e2e/m45-scenario.spec.ts, e2e/fixtures/tauriMock.ts, scripts/smoke.mjs, scripts/smoke.test.mjs | 依存: T65d)
  - 対応: AC-K01〜K07・K09 の通し、【要件の追加】
  - 置くもの: スモークに `--settings`(一時の状態フォルダに、正しい変更の行と問題のある行を 1 つずつ含む `settings.toml` を置いて起動する)。準備完了とターミナルの出力を確かめ、標準エラーの `[arin:settings]` の行がコマンドの ID とキーの文字列だけであること、`[arin:frontend-error]` が無いこと、終わったあと設定ファイルの中身(ハッシュ)と更新日時が同じで `settings.toml` の名前の付いた別のファイルができていないことを確かめる
  - 先に書くテスト(このタスク自体がテスト): Playwright: `替えた表で起動し_メニューのキーの表示とツリーのキーとタブのツールチップがそろう`、`設定が変わったと届くと全部が新しい表に追従する`、`問題のある行の知らせが_1_行出て閉じられる`。`scripts/smoke.test.mjs`: `設定の指定で一時の状態フォルダに_settings_toml_を置く`、`終わったあと設定ファイルの中身と更新日時が同じか確かめる`、`設定の診断の行に設定の行の全文が無いか確かめる`
  - 並行: 可(T66b とファイルが重ならない)
  - 検証: `npx playwright test e2e/m45-scenario.spec.ts`(3 回続けて合格)`&& node --test scripts/smoke.test.mjs && npm run app && node scripts/smoke.mjs --settings && npm run smoke`
- [x] T66b — 文書の同期と設定の書き方の説明(変更ファイル: docs/data-model.md, docs/development-patterns.md, docs/project.md, docs/architecture.md, project-config.md, output/design/ARCH_multi-terminal.md, output/design/ADR_005_settings-format.md, output/design/ADR_012_keybindings.md | 依存: T65d)
  - 対応: ADR-012 の 6(開発者向けの仕様の置き場所)、ドキュメント更新計画
  - 書くもの: `docs/data-model.md` に `settings.toml` の節(置き場所、`[shortcuts]` の書式、コマンドの ID の一覧は生成物 `defaultShortcuts.json` を正とする旨、検査の規則、ファイル全体の問題の扱い、読むだけ・見本は無いときだけ)。`docs/development-patterns.md` に標準エラーの頭 `[arin:settings]`、落とし穴(`set_accelerator(None)` は macOS でキーを消さない、引用符の無い点つきの ID は TOML では入れ子の表になる、リンクの先の変更は監視で拾えない)。`docs/project.md`・`project-config.md` §2 に TOML のクレートの版、`docs/architecture.md` に新しいファイル。設計書: §5.1(`SettingsFileStore` → `FsSettingsFile`、`SettingsStore` は M4.5)、§5.2(`get_settings_notice`・`settings-changed`)、§6.1、§6.2(設定ファイルのキーの表に `[shortcuts]`)、§7.1「起動」の 2(設定ファイルは M4.5)、§7.2、§9.1(アプリ名のメニューの「設定ファイルを開く…」)。ADR-005 の「未定の点」に ADR-012 で決めたと追記
  - 並行: 可(T66a とファイルが重ならない)
  - 検証: 文書だけ。`git diff --stat` が上の変更ファイルだけであること

#### Phase 31.5h(逐次。人間ゲート)

- [ ] T66c — M4.5 の合否一覧と本人の確認(**AI の分は 2026-11-16 に済み**。残りは本人の (1)〜(10))(変更ファイル: output/reports/test/M4.5_acceptance.md, output/tasks/PROGRESS.md | 依存: T66a, T66b)
  - AI がやること: `npm run build && npm test && npm run lint && npm run test:coverage && npm run e2e && npm run app && npm run smoke && node scripts/smoke.mjs --tiles && node scripts/smoke.mjs --settings` を 1 件で通し、下の対応表に沿って AC-K の合否一覧を `output/reports/test/M4.5_acceptance.md` と `testreport/` に出す。本人に渡す確かめ方(設定ファイルの置き場所、見本の開き方、書き換えの例 3 つ)を書く。fresh context の `/code-review` と `verifier` を 1 回ずつ回す
  - **人間がやること**(`npm run app` で作った `.app` を Finder から開く): (1) ⌘, で見本が開き、書き方が読んで分かる(AC-K08)。(2) `"terminal.new-tab" = "Cmd+Shift+T"` を書いて保存すると、メニューバーの表示とキーが替わり、⌘T では開かない(AC-K01・K07)。(3) ツリーのキー(例 `"tree.new-file" = "Cmd+Alt+N"`)を替えると、ツリーのメニューの表示とキーが替わる(AC-K02)。(4) 保存しても動いているシェル(Claude Code など)が続いている(AC-K07)。(5) 読めない行・予約のキー(⌘H など)・重なりを書くと、その行だけ既定に戻り、知らせが 1 行で読める(AC-K04)。(6) 構文を壊して保存すると直前のキーのまま、行と列が出る(AC-K05)。(7) 行を消すと既定に戻る(AC-K03)。(8) 替えたキーが US と JIS の配列で効き、ターミナルに渡らない。替える前のキーはターミナルに渡る(AC-K06)。(9) 知らせの見た目(ライトとダーク、すりガラスの上。デザインの方針)。(10) ADR-012 の【要確認】の回答と、PRD への反映(FR の追加・AC-K の採否・M4.5 の区切り)の判断
  - 完了条件: AC-K の合否一覧ができ、本人が (1)〜(10) を確かめた
- [ ] T66d — 本人の確認の反映と最終の検証(変更ファイル: output/design/ADR_012_keybindings.md, output/design/ADR_INDEX.md, output/prd/PRD_multi-terminal.md(本人が了承した場合だけ), output/reports/test/M4.5_acceptance.md, output/tasks/PROGRESS.md, docs/(変わった点だけ) | 依存: T66c)
  - 変えるもの: ADR-012 の状態を本人の了承に合わせる。T66c で直すと決めた点(既定のキー・予約のキーの一覧・知らせの文言)を直す(ソースを直すときは、そのタスクの先に書くテストを足してから)。PRD への反映は本人が了承した内容だけを書く
  - 検証: `npm run build && npm test && npm run lint && npm run e2e && npm run app && npm run smoke && node scripts/smoke.mjs --settings`
  - 完了条件: 下の「M4.5 の完了条件」をすべて満たす

#### M4.5 の完了条件

- 下の対応表の AC-K が、自動テストと T66c の本人の確認で全件合格している(【要確認】の回答で外したものを除く)
- `npm run build`・`npm test`・`npm run lint`・`npm run test:coverage`(`arin-core` と `src/logic` のそれぞれで行 80%)・`npm run e2e`(`e2e/m45-scenario.spec.ts` は 3 回続けて合格)・`npm run smoke`・`node scripts/smoke.mjs --tiles`・`node scripts/smoke.mjs --settings` が終了コード 0
- テストとスモークが、本物の `~/Library/Application Support/Arin/`(`settings.toml` を含む)と一時フォルダの外のファイルを作っていない・変えていない。どのテストでも、既にある設定ファイルの中身と更新日時が変わっていない
- `arin-core` の依存が `serde`・`serde_json`・`thiserror`・`ts-rs` のまま(`npm run lint` の境界検査)。`cargo tree -i reqwest --target aarch64-apple-darwin` が空。TOML のクレートの版と確かめた結果が ADR-012 にある
- `src-tauri/src/menu.rs` と `src/ui/` にキーの文字列の直書きが無い(`TabBar` の `Meta+T`・`⌘T` を含む。表の 23 行から引く)
- 本人が T66c の (1)〜(10) を確かめた。ADR-012 が本人の了承に合わせて直っている

#### 要件と受け入れ基準の対応(漏れの確認用。PRD に FR が無いので、ADR-012 の仮の受け入れ基準 AC-K で対応を取る。PRD への反映は本人の了承が要る)

| AC | 受け入れ基準(要約。全文は ADR-012) | 規則(`arin-core`) | 読み込み・メニュー(Rust) | 画面 | 人間(T66c) |
| -- | ---------------------------------- | ------------------ | ------------------------ | ---- | ---------- |
| AC-K01 | `terminal.new-tab` を替えるとメニューの表示とキーが替わり、元のキーでは開かない | T64a | T65a、T65b | T65e、T66a | (2) |
| AC-K02 | 画面で受けるコマンドを替えるとツリーで効き、メニューの表示・タブの「+」のツールチップと読み上げのキーも表から出る | T64a | T65b | T65e、T66a | (3) |
| AC-K03 | 書かなかった行は既定。行を消すと既定。ファイルが無ければ既定で知らせなし | T64a、T64b | T65b、T65c | T66a | (7) |
| AC-K04 | 問題のある行はその行だけ既定に戻り、残りは効き、ID と理由が 1 行で出る | T64a | T65b | T65e、T66a | (5) |
| AC-K05 | TOML として読めないと起動時は既定、動作中は直前のまま、行と列が出る。設定ファイルは変わらず退避もされない | T64b | T65a、T65b、T65c | T66a(スモーク) | (6) |
| AC-K06 | 替えたあとのメニューのキーはターミナルに渡らず、替える前のキーは渡る | T64c(既存の `keyOwner`) | — | T65e、T66a | (8) |
| AC-K07 | 動作中に保存すると再起動せずに替わり、シェルが続く(【要確認】3 の B) | — | T65c | T65e、T66a | (2)、(4) |
| AC-K08 | 「設定ファイルを開く…」で、無ければ見本を作って開き、あれば変えずに開く(【要確認】4 の A) | T64c | T65a、T65d | — | (1) |
| AC-K09 | 知らせと標準エラーに ID・キーの文字列・行と列・理由だけが出る | T64a、T64b | T65a、T65b | T66a(スモーク) | (5)、(6) |
| AC-K10 | `ARIN_STATE_DIR` の差し替え先の `settings.toml` だけを読む | T64b | T65b | T66a(スモーク) | — |
| M3 の残り | タブの「+」の `Meta+T` の直書きを表から引く | — | — | T65e | (3) |

### M4.6 フォントサイズの設定画面(T67 で分解した。2026-11-16)

**【要件の追加(本人。2026-11-16)】** 「ターミナルのフォントサイズと、UI のフォントサイズの変更を、設定画面でできるようにする」。本人が決めたこと: (1) 値は `state.json` に保存する。`settings.toml` は人だけが書き、フォントサイズの項目を作らない。(2) ⌘, は設定画面を開く。「設定ファイルを開く…」は設定画面の中のボタンとメニューの項目に移す。(3) ⌘+・⌘− は今回入れない(あとから足せる形)。フォントの種類・配色も入れない。PRD に無い要件なので、PRD への反映(FR の追加、FR-049 の分割、N5 との線引き)は本人の判断を待つ。仮の受け入れ基準は ADR-013 の AC-F01〜F14。

完了条件: ADR-013 の AC-F01〜F14(【要確認】の回答で外れたものを除く)が自動テストと T73a の本人の確認で合格し、下の「M4.6 の完了条件」をすべて満たす。

#### M4.6 の前提

設計の決定は ADR-013(`output/design/ADR_013_font-size-settings.md`。Proposed)。【要確認】の 5 点は**推奨案で計画を組んだ**。

- (a) **T56 からの前倒し**: フォントサイズは M6 の T56(FR-049。`settings.toml` の `[terminal]` の `font_size`)から M4.6 に前倒しし、置き場所を設定ファイルから `state.json` に替えた。T56 の項は書き換えない(フォントの種類・配色などの残りは T56 のまま。ADR-013 の 8)
- (b) **保存**: `state.json` の `ui.terminalFontSize`(既定 15 = いまの xterm.js の既定)と `ui.interfaceFontSize`(既定 13 = いまの `--font-size-l`)。整数の CSS ピクセル。範囲はターミナル 9〜28・画面 11〜18、1 刻み(【要確認】1)。既定のあいだは書かない。無い・数でないは既定、整数でないは四捨五入、範囲の外は端に留める。退避しない。版は 1 のまま(ADR-013 の 1)
- (c) **流れ**: 正本は `Workspace`(規則は `crates/arin-core/src/layout/font_sizes.rs`)。コマンド `step_font_size(target, delta)`・`reset_font_sizes()` → 変わったら `notify` → 写しの `fontSizes`(値・最小・最大・既定)→ 画面。保存は既存の保存のスレッド(ADR-013 の 2)
- (d) **ターミナル**: `TerminalSurface.setFontSize`。表示中は当ててから `fit` し直す(`ResizeObserver` は鳴らない)、隠しているものは表示に戻すときに当てる。変更は 1 フレームにまとめる。PTY への寸法は今の経路(ADR-013 の 3)
- (e) **画面**: 純粋な関数 `interfaceMetrics(size)`(`src/logic/appearance/`)が整数の px を返し、`:root` の変数を上書きする。追従は文字の 4 トークンと行の高さ 9 トークンと新しい `--tree-input-height`(【要確認】5)。ツリーの行の高さは CSS から読まず同じ関数から受け取る(ADR-013 の 4)
- (f) **設定画面**: 画面内のシート(`<dialog>`、背景を暗くしない=【要確認】4)、ステッパー(`role="spinbutton"`)、即時に反映、開閉は `uiStore`(ADR-013 の 5)
- (g) **⌘,**: 表の行 `app.open-settings` の意味を「設定…(設定画面)」に替え、Rust は `menu-command` の `"open-settings"` を送る。「設定ファイルを開く…」は表の外のキーなしの項目(ID `app.open-settings-file`)と、コマンド `open_settings_file`。表は 23 行のまま、生成物の表の中身は変わらない(【要確認】2。ADR-013 の 6)
- (h) **M4.5 との順**: M4.5 は本人の確認待ち(T66c・T66d)。T66d は `menu.rs`・`settings.rs`・ADR-012・`docs/` を直しうるので、それらを触る T68d・T72 は T66d の完了後に行う。それ以外(T68a〜T68c・T69a〜T69b)は T66d を待たずに始めてよい
- (i) **M5 の順**: M5 は M4.6 のあとに置く(T51・T52 の依存は T66 のまま書いてある。書き換えは本人の了承のあと T73b で行う)
- (j) **重い処理は 1 件ずつ**: 下の「並行: 可」はファイルが重ならないという意味で、**同じ時に走らせるのは 1 件だけ**にする(このマシンは並行のビルドとテストで固まった)。順は Phase の番号と、Phase の中の書いた順

【要確認】の回答が推奨と違うとき:

| 【要確認】 | 推奨(計画の前提) | 違う回答のときのタスクの直し |
| ---------- | ------------------ | ---------------------------- |
| 1. 範囲と刻み | 9〜28・11〜18、1 刻み | T68a の定数と表のテストだけ |
| 2. ⌘, と表の行 | A(意味を替える。設定ファイルはキーなしの表の外の項目) | B: T68d で表に 24 行目と新しい既定のキー、生成物と `defaultShortcuts.ts` を作り直す(JIS で打てるかも確かめる)。C: T68d の前に表の型を替える子タスクを 1 つ足す |
| 3. 設定画面の ⌘W | 取り消して設定画面を閉じる(T70a の着手時に効くか確かめる) | B: T70b の ⌘W の扱いとテストを外す |
| 4. 背景を暗くするか | 暗くしない | T70b の CSS 1 行と画像 |
| 5. 画面で追従させる範囲 | 文字と行の高さだけ | アイコンも: T69b にアイコンの px のトークン化を足す(CSS の直書き 10 か所ほど) |

#### 現状のコードから分かった、M4.6 で足すところ・変えるところ

- `XtermEngine.ts:57-63` は `fontSize` を渡していない(xterm.js 6.0.0 の既定 15)。`TerminalSurface` に文字の大きさを変える口が無い → T69a
- 文字の大きさだけを変えても置き場所の大きさは変わらず、`TerminalHost` の `ResizeObserver` が鳴らない。`fit` は隠しているあいだ何もしない(`XtermEngine.ts:107-110`)→ T69a で明示して呼び、隠しているものは表示に戻すときに当てる
- `tokens.css` が文字の 4 トークン(11・12・13・15)と行の高さのトークンを持ち、部品の `font-size` はすべてトークン。部品の CSS の px の直書きは 9 ファイルで約 100 か所(`tokens.css` の 42 を除く)で、大半は線・輪・アイコン・余白・幅の上下限。文字の入れ物の直書きは `TreeView.css:178`・`188`(名前の入力欄の 18px)だけ → T69b
- `FileTreeView.tsx:68-72`・`214-217` は行の高さを付いたときに 1 回だけ CSS から読む(`calc()` にすると読めず 22 に落ちる)→ T69b で関数から受け取る形に替える
- `PersistedLayout`・`UiSection`(`persistence/codec.rs`)、`Workspace`(`restore`・`persisted_state`・`snapshot`)、`AppSnapshot` に文字の大きさが無い。`PersistedLayout { … }` の構造体の書き方が `codec.rs`・`workspace/state.rs`・`tests/persistence_codec.rs`・`tests/workspace_projects.rs` に計 10 か所 → T68a・T68b
- 写しの形を足すと、画面の写しの見本(`src/test/fakeIpc.ts`、`src/state/actions.test.ts`、`src/state/selectors.test.ts`、`e2e/fixtures/tauriMock.ts` の `sampleSnapshot`)が型検査で落ちる → T68a で一緒に直す
- `menu.rs` の自前の項目は表の行からしか作らない。`on_menu_event` は表に無い ID を捨てる。`MenuCommand` は `"select-all"` だけ。画面の `onMenuCommand` は `TerminalManager.handleMenuCommand` 直結(`main.tsx:72`)→ T68d・T70a
- 確認ダイアログ(`ConfirmDialog`)はモーダルの `<dialog>` と自前の Tab の回し方を持つ。設定画面は確認ではないので別の部品にし、Tab の回し方は同じ考えで書く → T70b
- ⌘W は確認なしでタブを閉じシェルを終わらせる(`sessions.rs:528-531`)。keydown を取り消したときメニューに回るかは、リポジトリの中の記述が食い違う(`menu.rs:8` と `XtermEngine.ts:67-69`)→ T70a の着手時に確かめる
- スモーク(`scripts/smoke.mjs`)は `state.json` を置ける(`--tiles`)が、文字の大きさを入れた状態の指定が無い → T71

#### 親タスクと子タスク

| 親 | 子 |
| -- | -- |
| T68 — Rust(規則・保存・コマンド・⌘,) | T68a(規則と写しの型)、T68b(保存と `Workspace` の口)、T68c(コマンドと保存の結合)、T68d(⌘, の付け替えとメニュー) |
| T69 — 画面への反映 | T69a(ターミナル)、T69b(画面の CSS の基準値化とツリー) |
| T70 — 設定画面 | T70a(開閉と配線)、T70b(部品とデザイン) |
| T71〜T73 — 通しの確認と文書 | T71(E2E とスモーク)、T72(文書の同期)、T73a(合否一覧と本人の確認)、T73b(反映と最終の検証) |

#### 共有のファイルの持ち回り(表の順に逐次で触る)

| ファイル | 触るタスク(この順) |
| -------- | ------------------ |
| `crates/arin-core/src/layout/font_sizes.rs`、`crates/arin-core/src/layout/mod.rs`、`crates/arin-core/tests/font_sizes.rs` | T68a だけ |
| `crates/arin-core/src/workspace/snapshot.rs`、`crates/arin-core/tests/generated_types.rs`、`src/ipc/generated/` | T68a → T68d(`MenuCommand.ts`) |
| `crates/arin-core/src/persistence/codec.rs`、`crates/arin-core/tests/persistence_codec.rs`、`crates/arin-core/src/workspace/state.rs`、`crates/arin-core/src/workspace/font_sizes.rs`(新)、`crates/arin-core/tests/workspace_font_sizes.rs`、`crates/arin-core/tests/workspace_projects.rs` | T68a(`state.rs` の写しだけ)→ T68b |
| `src-tauri/src/sessions.rs`、`src-tauri/src/commands/layout.rs`、`src-tauri/src/lib.rs`、`src-tauri/Cargo.toml`、`src-tauri/tests/font_commands.rs`、`src-tauri/tests/app_persistence.rs` | T68c → T68d(`lib.rs`・`commands/app.rs` だけ) |
| `crates/arin-core/src/workspace/events.rs`、`crates/arin-core/src/shortcuts/table.rs`(説明の文)、`crates/arin-core/src/shortcuts/sample.rs`、`crates/arin-core/tests/shortcuts.rs`、`src-tauri/src/menu.rs`、`src-tauri/src/settings.rs`、`src-tauri/src/commands/app.rs`、`src-tauri/tests/app_menu.rs`、`src-tauri/tests/app_settings.rs` | T68d だけ |
| `src/test/fakeIpc.ts`、`e2e/fixtures/tauriMock.ts` | T68a(写しの見本)→ T70a(コマンドの応答)→ T71 |
| `src/state/actions.test.ts`、`src/state/selectors.test.ts` | T68a → T70a(`actions.test.ts`) |
| `src/terminal/engine.ts`、`src/terminal/xterm/XtermEngine.ts`、`src/terminal/TerminalManager.ts`(とテスト)、`src/test/fakeTerminalEngine.ts`、`src/state/terminalSync.ts`(とテスト) | T69a だけ |
| `src/logic/appearance/`、`src/ui/design/tokens.css`、`src/ui/design/applyInterfaceMetrics.ts`、`src/ui/tree/TreeView.css`、`src/ui/tree/FileTreeView.tsx` | T69b だけ |
| `src/ui/shell/AppShell.tsx` | T69b(基準値を当てる)→ T70b(設定画面を置く) |
| `src/ipc/client.ts`、`src/ipc/client.test.ts`、`src/state/uiStore.ts`、`src/state/actions.ts`、`src/logic/keys/menuCommand.ts`(とテスト)、`src/main.tsx` | T70a だけ |
| `src/ui/settings/`(新)、`e2e/font-settings.spec.ts`(新) | T70b だけ |
| `e2e/m46-scenario.spec.ts`(新)、`scripts/smoke.mjs`、`scripts/smoke.test.mjs` | T71 だけ |
| `docs/`、`output/design/ARCH_multi-terminal.md`、`output/design/ADR_012_keybindings.md`、`output/design/ADR_013_font-size-settings.md` | T72 → T73b(本人の確認で変わった点だけ) |

#### Phase 31.6a(逐次。文書だけ)

- [x] T67 — M4.6 の分解と ADR(変更ファイル: output/tasks/PLAN_multi-terminal.md, output/design/ADR_013_font-size-settings.md, output/design/ADR_INDEX.md | 依存: なし)
  - 決めたこと(ADR-013): (1) 保存の形と既定・範囲・読むときの扱い。(2) 流れとコマンド(`delta` で動かす)。(3) ターミナルへの当て方。(4) 画面への当て方と追従の範囲。(5) 設定画面の形。(6) ⌘, と表の行。(7) ⌘+・⌘− の足し方。(8) T56 との分担。仮の受け入れ基準 AC-F01〜F14。【要確認】5 点
  - 検証: 文書だけ。`ls output/design/ADR_013_*.md` が 1 件(済み)

#### Phase 31.6b(逐次。Rust。T67 完了後)

- [x] T68a — 文字の大きさの規則と写しの型(変更ファイル: crates/arin-core/src/layout/font_sizes.rs, crates/arin-core/src/layout/mod.rs, crates/arin-core/src/workspace/snapshot.rs, crates/arin-core/src/workspace/state.rs, crates/arin-core/tests/font_sizes.rs, crates/arin-core/tests/app_snapshot.rs, crates/arin-core/tests/generated_types.rs, src/ipc/generated/, src/test/fakeIpc.ts, src/state/actions.test.ts, src/state/selectors.test.ts, e2e/fixtures/tauriMock.ts | 依存: T67)
  - 対応: ADR-013 の 1・2、AC-F06・F08・F09 の規則の部分
  - 置くもの: `FontSizes { terminal, interface }`(既定 15・13)、`FontSizeTarget`(`terminal`/`interface`)、範囲と既定の定数、`stepped(target, delta)`(端で留める)、`reset()`、`from_persisted(raw)`(JSON の値 → 既定・四捨五入・端に留める)。写しの `FontSizesSnapshot`(それぞれ `value`・`min`・`max`・`defaultValue`)を `AppSnapshot.fontSizes` に足す(版は 1 のまま)。`Workspace::snapshot` は当面 `FontSizes::default()` を載せる(T68b で持たせる)。生成物を作り直し、画面の写しの見本 4 か所に既定の `fontSizes` を足す
  - 安全: 入出力なし
  - 並行: 不可(生成物)
  - 先に書くテスト: `既定はターミナル_15_画面_13`、`刻みは範囲の端で留まる`(表で)、`既定に戻すと両方が既定`、`保存値が無い_null_数でないときは既定`、`整数でない数は四捨五入する`、`範囲の外は端に留める`、`写しに値と範囲と既定が載る`、`写しの_json_の形`(生成物が最新)
  - 検証: `cargo test -p arin-core --test font_sizes --test app_snapshot --test generated_types && npx tsc -b`
- [x] T68b — 保存と `Workspace` の口(変更ファイル: crates/arin-core/src/persistence/codec.rs, crates/arin-core/src/workspace/state.rs, crates/arin-core/src/workspace/font_sizes.rs, crates/arin-core/src/workspace/mod.rs, crates/arin-core/tests/persistence_codec.rs, crates/arin-core/tests/workspace_font_sizes.rs, crates/arin-core/tests/workspace_projects.rs | 依存: T68a)
  - 対応: AC-F06・F07・F08
  - 置くもの: `PersistedLayout.font_sizes`。`UiSection` に `terminalFontSize`・`interfaceFontSize`(読みは `lenient_display_mode` と同じく退避しない形、既定なら書かない)。`Workspace` が `FontSizes` を持ち、`restore` で読み、`persisted_state`・`snapshot` に出す。`step_font_size(target, delta) -> bool`・`reset_font_sizes() -> bool`(変わったときだけ真)
  - 安全: 1 つの値のために `state.json` を退避しない
  - 並行: 不可(T68a に続く)
  - 先に書くテスト: `文字の大きさが無い版_1_の_state_json_をそのまま読む`、`既定の大きさは書かない`、`変えた大きさを書いて読み戻せる`、`数でない文字の大きさは退避せず既定`、`範囲の外の文字の大きさは端に留める`、`文字の大きさを足しても_schema_version_は_1`、`刻むと写しの値が変わり真を返す`、`端で刻んでも変わらず偽を返す`、`既定に戻すと両方が既定になり既定のときは偽`
  - 検証: `cargo test -p arin-core --test persistence_codec --test workspace_font_sizes --test workspace_projects`
- [x] T68c — コマンドと保存の結合(変更ファイル: src-tauri/src/sessions.rs, src-tauri/src/commands/layout.rs, src-tauri/src/lib.rs, src-tauri/Cargo.toml, src-tauri/tests/font_commands.rs, src-tauri/tests/app_persistence.rs | 依存: T68b)
  - 対応: AC-F02(Rust 側)・F06・F07・F08、ADR-013 の 2・7
  - 置くもの: `SessionRegistry::step_font_size`・`reset_font_sizes`(変わったら `notify`)。コマンド `step_font_size(target, delta)`・`reset_font_sizes()`(`commands/layout.rs`。`generate_handler!` に足す)。`[[test]]`(`harness = false`)
  - 安全: 一時の状態フォルダだけ
  - 並行: 不可(`lib.rs`)
  - 先に書くテスト(模擬ランタイム): `刻むと写しが届き保存される`、`続けて_3_回刻むと_3_上がる`、`端では写しを送らない`、`既定に戻すと両方が既定の写しが届く`、`保存した大きさで起動し直すと写しに出る`、`文字の大きさを変えてもシェルは続く`(セッションの状態が `running` のまま)
  - 検証: `cargo test -p arin --test font_commands --test app_persistence`
- [x] T68d — ⌘, の付け替えとメニュー(【要確認】2 の A)(変更ファイル: crates/arin-core/src/workspace/events.rs, crates/arin-core/src/shortcuts/table.rs, crates/arin-core/src/shortcuts/sample.rs, crates/arin-core/tests/shortcuts.rs, crates/arin-core/tests/generated_types.rs, src/ipc/generated/MenuCommand.ts, src-tauri/src/menu.rs, src-tauri/src/settings.rs, src-tauri/src/commands/app.rs, src-tauri/src/lib.rs, src-tauri/tests/app_menu.rs, src-tauri/tests/app_settings.rs | 依存: T68c, T66d)
  - 対応: AC-F01・F10・F11(Rust 側)、ADR-013 の 6
  - 置くもの: `MenuCommand::OpenSettings`(`"open-settings"`)。`menu::label(AppOpenSettings)` を「設定…」に、`on_menu_event` は `menu-command` を送る。表の外の項目「設定ファイルを開く…」(定数 `OPEN_SETTINGS_FILE_ID = "app.open-settings-file"`、キーなし。「設定…」の下)で `settings::open_from_menu` を呼ぶ(名前は `open_settings_file` に替えてよい)。コマンド `open_settings_file`(引数なし)。見本の先頭に「文字の大きさはアプリの「設定…」(⌘,)で変えます」の 1 行。表は 23 行のまま(`table.rs` は説明の文だけ)
  - 安全: 設定ファイルは無いときだけ見本を作る(ADR-012 の 6 のまま)
  - 並行: 不可(`lib.rs`・`menu.rs`)
  - 先に書くテスト: `設定の既定は_cmd_カンマで受け口はメニューの項目`(既存を「設定画面」の意味に読み替え)、`アプリ名のメニューの先頭は設定と設定ファイルを開くで_設定ファイルを開くにはキーが無い`、`設定を選ぶと_open_settings_のメニューの操作が届く`、`設定ファイルを開くを選ぶと無いときは見本を作って開く`(既存の AC-K08 のテストを新しい ID で)、`open_settings_file_のコマンドも同じく開く`、`設定で替えたキーが設定のメニューの項目に付く`(AC-F11)、`見本に文字の大きさは設定画面で変える旨がある`、`表の行は_23_のまま`。既存の `app_menu.rs` の「自前の項目は設定ファイルを開くと終了だけ」は新しい並びに直す(直す理由をテストの説明に書く)
  - 検証: `cargo test -p arin-core --test shortcuts --test generated_types && cargo test -p arin --test app_menu --test app_settings && node --test scripts/default-shortcuts.test.mjs`。節目として続けて `npm run lint && npm test`(1 件で回す)

#### Phase 31.6c(画面への反映。T68a 完了後。T68b〜T68d と並べられるが同じ時に走らせない)

- [x] T69a — ターミナルへの反映(変更ファイル: src/terminal/engine.ts, src/terminal/xterm/XtermEngine.ts, src/terminal/TerminalManager.ts, src/terminal/TerminalManager.test.ts, src/test/fakeTerminalEngine.ts, src/state/terminalSync.ts, src/state/terminalSync.test.ts | 依存: T68a)
  - 対応: AC-F02・F03・F09、ADR-013 の 3
  - 置くもの: `TerminalSurface.setFontSize(px)`、`createSurface(id, { fontSize })`。`XtermSurface` は表示中なら `options.fontSize` を替えて `fit`、隠していれば覚えて `setVisible(true)` で当ててから `fit`。`TerminalManager.setFontSize(px)`(全サーフェスへ。1 フレームにまとめ最後の値だけ。`#lastShownSize` を更新)。`terminalSync` が写しの `fontSizes.terminal.value` の変化で呼ぶ(`present` より前)
  - 安全: 接続を切らない・作り直さない
  - 並行: 可(T68b〜T68d・T69b とファイルが重ならない)
  - 先に書くテスト(Vitest): `大きさを変えると表示中のサーフェスに当てて寸法を合わせ直す`、`隠しているサーフェスは表示に戻すときに当てる`、`新しく作るサーフェスは今の大きさで作る`、`寸法が変われば_resize_terminal_を送り接続は保つ`、`続けて変えると最後の値だけを_1_回当てる`、`写しの大きさが同じなら何もしない`、`隠したまま作るサーフェスも写しの大きさで作る`(`terminalSync`)
  - 検証: `npx vitest run src/terminal src/state/terminalSync.test.ts`
  - 着手時に確かめる: xterm.js 6.0.0 で `display: none` のまま `fontSize` を替えると文字の寸法が 0 で測られるか(ADR-013 の 3 の【仮定】)。T70b の E2E で「隠していたタブを表示すると、初めから新しい大きさだったものと同じ行・桁」を WebKit で確かめる
- [x] T69b — 画面の CSS の基準値化とツリー(【要確認】5)(変更ファイル: src/logic/appearance/interfaceMetrics.ts, src/logic/appearance/interfaceMetrics.test.ts, src/ui/design/applyInterfaceMetrics.ts, src/ui/design/tokens.css, src/ui/tree/TreeView.css, src/ui/tree/FileTreeView.tsx, src/ui/shell/AppShell.tsx | 依存: T68a)
  - 対応: AC-F04・F05・F09、ADR-013 の 4
  - 置くもの: `interfaceMetrics(size)`(追従の 14 トークンを整数の px で。`round(既定 × size / 13)`)。`applyInterfaceMetrics(root, metrics)`(`style.setProperty`)。`tokens.css` に `--tree-input-height: 18px`、`TreeView.css` の 18px 2 か所を置き換え。`FileTreeView` は行の高さを props か写しから受け取り、変わったら窓を計算し直す(`readRowHeight` と `FALLBACK_ROW_HEIGHT` は外すか既定の値に使う)。`AppShell` が写しの `fontSizes.interface.value` で当てる
  - 安全: HTML の差し込みを使わない(CSSOM の `setProperty` だけ)
  - 並行: 可(T69a とファイルが重ならない)
  - 先に書くテスト: Vitest: `既定の_13_では_tokens_css_の値と同じ`(`tokens.css` を読んで比べる)、`どの大きさでも整数の_px`、`大きさに比例し単調に増える`(表で 11〜18)、`追従させないトークンは返さない`。Playwright(既存の `e2e/tree.spec.ts` に足す): `画面の大きさを変えた写しでツリーの行の高さと仮想スクロールがそろう`、`選択中の行がキーの移動で見える位置に来る`。既存の E2E が既定の大きさで全件通る(AC-F09)
  - 検証: `npx tsc -b && npx vitest run src/logic/appearance && npx playwright test e2e/tree.spec.ts e2e/app-shell.spec.ts e2e/tiles.spec.ts`

#### Phase 31.6d(設定画面。T68c・T69a・T69b 完了後。逐次)

- [x] T70a — 開閉と配線(変更ファイル: src/ipc/client.ts, src/ipc/client.test.ts, src/state/uiStore.ts, src/state/uiStore.test.ts, src/state/actions.ts, src/state/actions.test.ts, src/logic/keys/menuCommand.ts, src/logic/keys/menuCommand.test.ts, src/main.tsx, src/test/fakeIpc.ts, e2e/fixtures/tauriMock.ts | 依存: T68c, T68d)
  - 対応: AC-F01・F07・F10(画面側)、【要確認】3 の確認
  - 置くもの: 包み `stepFontSize`・`resetFontSizes`・`openSettingsFile`。`uiStore` に `settingsOpen`・`openSettings`(ほかの確認ダイアログが開いていれば開かない)・`closeSettings`。`routeMenuCommand` に `"open-settings"`。`main.tsx` の `onMenuCommand` で振り分け。偽の Rust 側に 3 つのコマンドの応答
  - **着手時に確かめる(【要確認】3)**: `npm run app:debug` の実機で、`<dialog>` の keydown で ⌘W を `preventDefault` したときにメニューの「タブを閉じる」が動かないか。結果を ADR-013 の末尾に書く。動いてしまうなら T70b の ⌘W の扱いを外し、本人に報告する
  - 並行: 不可
  - 先に書くテスト(Vitest): `設定を開くのメニューの操作で設定画面が開く`、`開いているときにもう一度届いても_1_つのまま`、`ほかの確認ダイアログが開いていれば開かない`、`刻むと_step_font_size_を差分で呼ぶ`、`既定に戻すで_reset_font_sizes_を呼ぶ`、`設定ファイルを開くで_open_settings_file_を呼ぶ`、`すべてを選択の振り分けは変わらない`
  - 検証: `npx tsc -b && npx vitest run src/ipc src/state src/logic/keys`
- [x] T70b — 設定画面の部品とデザイン(変更ファイル: src/ui/settings/SettingsDialog.tsx, src/ui/settings/SettingsDialog.css, src/ui/settings/FontSizeStepper.tsx, src/ui/shell/AppShell.tsx, e2e/font-settings.spec.ts | 依存: T70a)
  - 対応: AC-F01〜F07・F10・F12・F13、ADR-013 の 5
  - 置くもの: シートの `<dialog>`(`role="dialog"`、名前「設定」、背景は透明=【要確認】4)、2 行のステッパー(`role="spinbutton"`、↑↓・Home・End、▲▼ は `tabindex="-1"`、端で無効、`aria-valuetext` は「N ポイント」)、「設定ファイルを開く…」、「既定に戻す」(両方既定で無効)、「完了」。Esc・Return・⌘W(T70a の確認しだい)で閉じ、前のフォーカスへ戻す。Tab は中を回す
  - **デザイン**(本人の指示): 「AI にありがちなデザインを避け、Apple のデザインらしく、ハイセンスに」。既存のトークンの色だけ、グラデーション・絵文字・大きな角丸のカード・影の重ねを使わない、組は区切り線と余白で分ける。Playwright の画像(ライトとダーク、既定・最小・最大の大きさ)を `testreport/` に撮り、自分の目で確かめてから終える
  - 安全: 文言は文字として描く
  - 並行: 不可
  - 先に書くテスト(Playwright `e2e/font-settings.spec.ts`): `cmd_カンマのメニューの操作で設定画面が開きターミナルのステッパーにフォーカスがある`、`上矢印で_step_font_size_が_1_で呼ばれ届いた写しで値が変わる`、`ターミナルの大きさの写しでタブ表示とタイル_9_枚の行と桁が変わり_resize_terminal_が送られる`(AC-F02)、`隠していたタブを表示すると初めから新しい大きさだったものと同じ行と桁`(AC-F03)、`画面の大きさの写しでサイドバーとタブとツリーの文字と行の高さが変わりターミナルの文字は変わらない`(AC-F04)、`端ではボタンが無効で値が変わらない`(AC-F06)、`既定に戻すが既定のときは無効`(AC-F07)、`設定ファイルを開くで_open_settings_file_が呼ばれる`、`tab_が設定画面の中を回り_esc_で閉じてターミナルにフォーカスが戻る`(AC-F12)、`ダイアログとステッパーの名前と値が読み上げ用に付く`
  - 検証: `npx tsc -b && npx playwright test e2e/font-settings.spec.ts e2e/tabs.spec.ts e2e/tiles.spec.ts e2e/settings.spec.ts`

#### Phase 31.6e(逐次。T70b 完了後)

- [x] T71 — M4.6 のシナリオの E2E と、文字の大きさを入れたスモーク(変更ファイル: e2e/m46-scenario.spec.ts, e2e/fixtures/tauriMock.ts, scripts/smoke.mjs, scripts/smoke.test.mjs | 依存: T70b)
  - 対応: AC-F02・F04・F08・F14 の通し
  - 置くもの: スモークに `--fonts`(一時の状態フォルダの `state.json` に `ui.terminalFontSize: 20`・`ui.interfaceFontSize: 15` を置いて起動)。準備完了とターミナルの出力、`[arin:frontend-error]` が無いこと、終わったあと `state.json` に同じ 2 つの値が残り退避のファイルができていないこと、`settings.toml` ができていないことを確かめる
  - 先に書くテスト: Playwright: `設定画面で両方を変えて閉じると全部が新しい大きさのまま操作できる`、`タイル表示で大きさを続けて変えても行と桁が最後の値に落ち着く`(`expect.poll`)。`scripts/smoke.test.mjs`: `文字の大きさの指定で一時の状態フォルダに_state_json_を置く`、`終わったあと文字の大きさが残り退避が無いか確かめる`
  - 並行: 不可
  - 検証: `npx playwright test e2e/m46-scenario.spec.ts`(3 回続けて合格)`&& node --test scripts/smoke.test.mjs && npm run app && node scripts/smoke.mjs --fonts && npm run smoke`

#### Phase 31.6f(逐次。T71・T66d 完了後)

- [x] T72 — 文書の同期(変更ファイル: docs/data-model.md, docs/architecture.md, docs/development-patterns.md, docs/project.md, output/design/ARCH_multi-terminal.md, output/design/ADR_012_keybindings.md, output/design/ADR_013_font-size-settings.md | 依存: T71, T66d)
  - 書くもの: `docs/data-model.md`(`ui.terminalFontSize`・`ui.interfaceFontSize`、写しの `fontSizes`、コマンド `step_font_size`・`reset_font_sizes`・`open_settings_file`、`menu-command` の `"open-settings"`、`settings.toml` の節の「設定ファイルを開く…」の場所)、`docs/architecture.md`(`src/ui/settings/`・`src/logic/appearance/`・新しいテスト)、`docs/development-patterns.md`(落とし穴: 文字の大きさだけでは `ResizeObserver` が鳴らない、`display: none` のまま xterm.js の文字の大きさを替えない、CSS の変数を `calc()` にすると `getPropertyValue` が計算前の文字列を返す。T70a の ⌘W の結果)、`docs/project.md`(概要に M4.6)。設計書: §6.1・§6.2(`state.json` の表に 2 行、設定ファイルのキーの表の `terminal.font_size` を「M4.6 で `state.json` に移した」に直す)・§6.3 の写しの表・§9.1(アプリ名のメニューの並び、設定画面)。ADR-012 の末尾に「`app.open-settings` の意味を ADR-013 の 6 で『設定…』に替えた」と追記
  - 並行: 不可(文書だけ。T71 と同じ時に走らせない)
  - 検証: 文書だけ。`git diff --stat` が上の変更ファイルだけであること

#### Phase 31.6g(逐次。人間ゲート)

- [ ] T73a — M4.6 の合否一覧と本人の確認(変更ファイル: output/reports/test/M4.6_acceptance.md, output/tasks/PROGRESS.md | 依存: T72)
  - AI がやること: `npm run build && npm test && npm run lint && npm run test:coverage && npm run e2e && npm run app && npm run smoke && node scripts/smoke.mjs --tiles && node scripts/smoke.mjs --settings && node scripts/smoke.mjs --fonts` を 1 件ずつ通し、AC-F の合否一覧を書く。fresh context の `/code-review` と `verifier` を 1 回ずつ回す
  - **人間がやること**(`npm run app` の `.app` を Finder から開く): (1) ⌘, とメニューの「設定…」で設定画面が開く(AC-F01)。(2) タブ表示とタイル 9 枚でターミナルの大きさを変えると、すぐ変わり、Claude Code などが続いて描き直される。続けて押しても固まらない(AC-F02)。(3) ほかのタブ・ほかのプロジェクトに切り替えても新しい大きさ(AC-F03)。(4) 画面の大きさを変えると、サイドバー・タブ・ツリー・右クリックのメニューの文字と行がそろって変わり、線とアイコンはにじまない(AC-F04・F05)。(5) 再起動しても残る。「既定に戻す」で M4.5 と同じ見た目に戻る(AC-F07〜F09)。(6) 「設定ファイルを開く…」(設定画面とメニュー)が M4.5 と同じに開く(AC-F10)。(7) キーボードだけで操作でき、⌘W で設定画面が閉じる(タブは閉じない。【要確認】3 の確認の結果しだい)(AC-F12)。(8) 見た目(ライト・ダーク・すりガラスの上。デザインの方針)(AC-F13)。(9) ADR-013 の【要確認】1〜5 の回答と、PRD への反映(FR の追加・FR-049 の分割・N5 との線引き・M4.6 の区切り)、M5 の依存を T73b に替えるかの判断
  - 完了条件: AC-F の合否一覧ができ、本人が (1)〜(9) を確かめた
- [ ] T73b — 本人の確認の反映と最終の検証(変更ファイル: output/design/ADR_013_font-size-settings.md, output/design/ADR_INDEX.md, output/prd/PRD_multi-terminal.md(本人が了承した場合だけ), output/tasks/PLAN_multi-terminal.md(M5 の依存。本人が了承した場合だけ), output/reports/test/M4.6_acceptance.md, output/tasks/PROGRESS.md, docs/(変わった点だけ) | 依存: T73a)
  - 変えるもの: ADR-013 の状態を本人の了承に合わせる。T73a で直すと決めた点を直す(ソースを直すときは先にテストを足す)
  - 検証: `npm run build && npm test && npm run lint && npm run e2e && npm run app && npm run smoke && node scripts/smoke.mjs --fonts`
  - 完了条件: 下の「M4.6 の完了条件」をすべて満たす

#### M4.6 の完了条件

- 下の対応表の AC-F が、自動テストと T73a の本人の確認で全件合格している(【要確認】の回答で外したものを除く)
- `npm run build`・`npm test`・`npm run lint`・`npm run test:coverage`(`arin-core` と `src/logic` のそれぞれで行 80%)・`npm run e2e`(`e2e/m46-scenario.spec.ts` は 3 回続けて合格)・`npm run smoke`・`node scripts/smoke.mjs --tiles`・`--settings`・`--fonts` が終了コード 0
- 既定の大きさで、既存の E2E が書き換えなしで通る(見た目が M4.5 と同じ。`app_menu.rs` の並びの検査は T68d で理由を付けて直したものだけ)
- `settings.toml` にフォントの項目が無く、設定画面の操作で `settings.toml` が作られず・変わらない。`state.json` の `schemaVersion` が 1 のまま
- テストとスモークが本物の `~/Library/Application Support/Arin/` を読み書きしていない
- `src/ui/` の部品の CSS に、追従させるトークンの値の直書きが無い(`TreeView.css` の 18px を含む)。キーの文字列の直書きが無い
- 本人が T73a の (1)〜(9) を確かめた。ADR-013 が本人の了承に合わせて直っている

#### 要件と受け入れ基準の対応(PRD に FR が無いので ADR-013 の仮の受け入れ基準 AC-F で対応を取る)

| AC | 受け入れ基準(要約。全文は ADR-013) | Rust | 画面 | E2E・スモーク | 人間(T73a) |
| -- | ---------------------------------- | ---- | ---- | ------------- | ---------- |
| AC-F01 | ⌘, と「設定…」で開く。1 つのまま。ターミナルに渡らない | T68d | T70a | T70b | (1) |
| AC-F02 | ターミナルの大きさが表示中の全部(タイル 9 枚)に即時に効き、PTY に寸法が伝わり、シェルが続く | T68c | T69a | T70b、T71 | (2) |
| AC-F03 | 隠していたものも表示に戻すと新しい大きさで同じ行・桁 | — | T69a | T70b | (3) |
| AC-F04 | 画面の大きさで文字と行の高さが変わり、余白・線・アイコン・ターミナルの文字は変わらない | — | T69b | T70b、T71 | (4) |
| AC-F05 | ツリーの仮想スクロールがそろい、選択中の行が見える | — | T69b | T69b | (4) |
| AC-F06 | 端で無効・それ以上変わらない | T68a、T68c | T70b | T70b | — |
| AC-F07 | 「既定に戻す」で両方既定。既定のときは無効 | T68b、T68c | T70a、T70b | T70b | (5) |
| AC-F08 | 再起動で残る。既定は書かない。壊れた値は既定・端。退避しない。版 1 | T68a、T68b、T68c | — | T71(スモーク) | (5) |
| AC-F09 | 既定の大きさで見た目が M4.5 と同じ | T68a | T69a、T69b | 既存の E2E | (5) |
| AC-F10 | 「設定ファイルを開く…」(ボタンとキーなしの項目)が AC-K08 と同じ。設定画面は `settings.toml` を作らない | T68d | T70a、T70b | T71(スモーク) | (6) |
| AC-F11 | `settings.toml` の `app.open-settings` のキーで設定画面が開く。見本に説明 | T68d | — | — | — |
| AC-F12 | キーボードだけで使え、閉じるとフォーカスが戻る。読み上げの名前と値 | — | T70a、T70b | T70b | (7) |
| AC-F13 | 見た目が控えめで、ライト・ダーク・すりガラスで読める | — | T70b | T70b(画像) | (8) |
| AC-F14 | テストとスモークは一時の状態フォルダだけ | T68c | — | T71 | — |

### M5 公証・配布(粗い分解。秘密情報を扱う作業は人間)

完了条件: 別ユーザーアカウントで Gatekeeper の警告なく起動する。PRD の依存は M0 だけなので前倒しもできるが、本計画では M4 の後に置いた。M4.5 を足したので、M4.5 の後に置く(2026-11-15。T51・T52 の依存を T50 から T66 に替えた)。

#### Phase 32(並行可能。M4.5 完了後)

- [ ] T51 — 署名・公証のスクリプトと手順書を書く(AI。実行はしない)(変更ファイル: scripts/package-mac.sh, package.json, src-tauri/tauri.conf.json, docs/release-notarization.md | 依存: T66)
  - 対応: G5、PRD §7 C1・C2。tadcap の `scripts/package-mac.sh` と `docs/release-notarization.md` を下敷きにする(Tauri が `.app` を署名・公証し、スクリプトが `.dmg` を署名・公証・staple して検証する)。`npm run dist:mac` を足す
  - 着手時に決める【要確認】: エンタイトルメント(設計書 §15.3 #2。M3 のゴミ箱で Finder に頼む方式を採った場合は Apple Events のエンタイトルメントが要る。ADR-007 の (a))、配布物の置き場所(同 #11)
  - 秘密情報(証明書、API キー、パスワード)は読まない・書かない。スクリプトは環境変数の名前だけを参照する
  - 検証: `bash -n scripts/package-mac.sh`(構文だけ)。`npm run lint`
- [ ] T52 — tag のときの公証のワークフローを書く(AI)(変更ファイル: .github/workflows/release.yml | 依存: T66)
  - 対応: 設計書 §13.3。ビルドは読み取り権限だけ、使い捨てのキーチェーン、公証後の検証。Secrets は名前だけを参照する
  - 検証: 手元では構文の確認だけ。動作の確認は T54

#### Phase 33(逐次。人間ゲート)

- [ ] T53 — **人間**: 証明書と API キーを用意し、手元で署名・公証する(変更ファイル: なし | 依存: T51)
  - Developer ID の証明書、App Store Connect の API キーを用意する。`npm run dist:mac` を実行する。AI はこのコマンドを実行しない
- [ ] T54 — **人間**: GitHub の Secrets と Environment を登録し、tag を push して確認する(変更ファイル: なし | 依存: T52, T53)
  - 別ユーザーアカウントで `.dmg` から入れ、Gatekeeper の警告なく起動することを見る(M5 の完了条件)。NFR-008 のエンタイトルメントの確認もここで行う
  - AI がやること: 結果を聞いて PROGRESS と `docs/release-notarization.md` を直すだけ

### M6 日常の磨き(粗い分解。T55 で分解し直す)

完了条件: 対象 FR の AC 合格と、ドッグフーディングの不満リストが空になる。

#### Phase 34(逐次。M5 完了後)

- [ ] T55 — M6 の着手時の確認と再分解(変更ファイル: output/tasks/PLAN_multi-terminal.md, output/tasks/PROGRESS.md, output/design/ | 依存: T45, T50, T54)
  - 設計書 §15.3 の #5(Finder へのドラッグの方式)、#6(外からの ⌘ドロップ=移動を許すか)、#8(Quick Look の方式)、#9(macOS 通知の方式)、#10(絞り込みの範囲)、#12(エージェントが出す通知の種類と `TERM_PROGRAM`)を決める・確かめる。`DOGFOODING_NOTES.md` の不満をタスクに取り込む(非ゴールに触れるものは実装せず、提案として記録する)

#### Phase 35(並行可能。T55 完了後。同じクレートを触るものは再分解のときに逐次にする)

- [ ] T56 — 設定ファイルと破損時の回復(変更ファイル: crates/arin-core/src/settings/, crates/arin-core/src/persistence/, crates/arin-platform/src/storage/, src/terminal/xterm/, src/ui/design/ | 依存: T55)
  - 対応: FR-049、057、058、062。`settings.toml` の読み込みと検証、保存の検知と読み直し、`state.json` の退避、ライト・ダークでのコントラスト(人間が両モードを確認)
  - 注記(2026-11-15): 設定ファイルの土台(置き場所と読み込み、`SettingsFilePort`、`SettingsStore` の最小、TOML のクレート、読み直しの契機、エラーの出し方)は M4.5(T63〜T65)に前倒しした。T56 では、その土台に設定の項目(フォント・配色・スクロールバック・目印の時間)を足す。M3 で後回しにした FR-029 の「設定で移動の確認を省略できる」(`tree.confirm_move_on_drop`)もここで足す(本人の判断で M4.5 に寄せることもできる)
  - 注記(2026-11-16、T63): 分担は ADR-012 の 7。M4.5 が作るもの: `AppPaths::settings_file()`、`SettingsFilePort` と `FsSettingsFile`(設計書の `SettingsFileStore` はこの名前にした。読むだけと、見本を無いときだけ作る口)、TOML の解析(`arin-platform`)、`SettingsLoad` と `SettingsStore` の最小(`arin-core/settings`)、`[shortcuts]` の節、状態フォルダの監視と読み直し(`settings-changed`)、設定の知らせ(`get_settings_notice`・`[arin:settings]`)、見本(`sample_settings_toml`)。T56 が足すもの: `[terminal]`・`[tree]`・`[notifications]` の節と項目ごとの検査(`SettingsValidator`)、項目が変わったときの反映(同じイベントに足す)、見本への項目の節。M4.5 の版は `[shortcuts]` 以外の節を黙って無視するので、T56 の節を先に書いた設定ファイルも読める
- [ ] T57 — アテンション、実行中の印、タブ名、ブランチ名(変更ファイル: crates/arin-core/src/terminal/, crates/arin-platform/src/process/, crates/arin-platform/src/git/, crates/arin-platform/src/desktop/, src/ui/projects/, src/ui/terminals/ | 依存: T55)
  - 対応: FR-011、012、045、046
- [ ] T58 — プロジェクトの操作の磨き(変更ファイル: crates/arin-core/src/project/, crates/arin-core/src/workspace/, src/ui/projects/, src/logic/keys/, src-tauri/src/menu.rs | 依存: T55)
  - 対応: FR-005、006、019
- [ ] T59 — ツリーの磨き(変更ファイル: crates/arin-core/src/fileops/, crates/arin-core/src/tree/, src/logic/tree/, src/ui/tree/, crates/arin-platform/src/desktop/ | 依存: T55)
  - 対応: FR-018(Quick Look)、020、021、032、033、034、038
- [ ] T60 — Finder・他アプリとのドラッグ(変更ファイル: crates/arin-platform/src/desktop/, src-tauri/src/commands/, src/logic/dnd/, src/ui/tree/ | 依存: T55)
  - 対応: FR-031
- [ ] T61 — スクロールバック内の検索と URL(変更ファイル: src/terminal/xterm/, src/terminal/engine.ts, package.json | 依存: T55)
  - 対応: FR-047。`@xterm/addon-search` と `@xterm/addon-web-links` を足す。URL は `http` と `https` だけ

#### Phase 36(逐次。Phase 35 完了後)

- [ ] T62 — M6 の完了確認と最終ゲート(変更ファイル: output/tasks/PROGRESS.md, output/reports/, docs/, project-config.md | 依存: T56, T57, T58, T59, T60, T61)
  - AC の合否一覧を出す。NFR-001〜003 を計測して記録する。`/code-review`、`/security-scan`、`/legal-check`、`verifier` を 1 回ずつ回す。**人間**: 不満リストが空になったことを確認する

## 依存関係グラフ

マイルストーンの順(各「人間」は人間ゲート):

```text
M0 ──→ M1 ──(Go)──→ M2 ──→ M3 ──→ M4 ──→ M4.5 ──→ M5 ──→ M6
        │(No-Go)
        └──→ 本人が判断(描画方式の変更 / 実装技術の見直し)。T28 以降は止める
```

M0:

```text
T1 ──┬──→ T2 ───────────────────────────────────────────┐
     ├──→ T3 ──┬──→ T5 ──┐                               │
     └──→ T4 ──┴──→ T6 ──┴──→ T7 ──┬──→ T8 ──┬──→ T10 ──┤
                                    │         └──→ T11 ──┼──→ T12 ──→ T13(人間)
                                    └──→ T9 ─────────────┘
```

M1:

```text
T13 ──┬──→ T14 ──→ T15 ──→ T17 ──┐
      └──→ T16 ──────────────────┴──→ T18 ──┬──→ T19 ───────────────┐
                                             ├──→ T20 ──→ T21 ──→ T22 ┤
                                             ├──→ T23 ───────────────┼──→ T25 ──→ T26(人間) ──→ T27
                                             └──→ T24 ───────────────┘
```

M2(T28 で分解し直した。T29〜T35 は親で、矢印は子タスクの間に引いた):

```text
T27 ──→ T28 ──┬──→ T28a(lint)──────────────────────────────────────→ T36
              ├──→ T28b(設計書)────────────────────────────────────→ T36
              ├──→ T34a(画面の純粋ロジック)───────────────────────→ T34b
              └──→ T29a ──┬──→ T29b ──┐
                          ├──→ T30a ──┤
                          ├──→ T30b ──┼──→ T32b ──┐
                          ├──→ T32a ──┘           │
                          ├──→ T31a ──────────────┤
                          ├──→ T31b ──→ T31c ─────┴──→ T33a ──→ T33b ──→ T33c ──→ T33d ──→ T33e ──→ T34b
                          └──→ T31d ──────────────────────────→ T33b

T34b ──→ T34c ──→ T34d ──┬──→ T35a ──┐
                         └──→ T35b ──┴──→ T36(人間)
```

M3(T37 で分解し直した。T38〜T44 は親で、矢印は子タスクの間に引いた。「(a)」は T37 の (a) のゴミ箱の検証の結果):

```text
T36 ──→ T37 ──┬──→ T41a(ツリーの行)──────────────────────────────────────────────┐
              ├──→ T41b(ドラッグ・メニュー・キー)────────────────────────────────┤
              └──→ T38a ──┬──→ T38b(パス)────────────────────────┐                │
                          ├──→ T38c(除外・変更)──────────────────┤                │
                          ├──→ T38d(ショートカットの表)──→ T38e ─┤                │
                          ├──→ T39a ──→ T39b ─────────────────────┤                │
                          ├──→ T40a ───────────────────┐          │                │
                          └──→ T40b ──→ T40d ──────────┴─(a)─→ T40c ──┬──→ T42a ──→ T42b ──→ T42c ──→ T42d ──→ T42e ──┴──→ T43a
                                                                       └──→ T44a(同時操作・往復・計測)────────────┐
T43a ──→ T43b ──→ T43c ──→ T43d ──→ T44b ──────────────────────────────────────────────────────────────────────────┴──→ T45(人間。1 週間)
```

細かい依存(図は省略して書いた): T42a ← T38c・T38e・T39a・T40a。T42b ← T38c・T40b・T42a。T42c ← T39b・T40c・T40d・T42b。T42d ← T38b・T42c。T42e ← T38d・T42d。T43a ← T41a・T41b・T42e。T44a ← T38b・T39b・T40c。T45 ← T44a・T44b。

M4(T46 で分解し直した。T46〜T50 は親で、矢印は子タスクの間に引いた。同時に動かすのは 2 件まで):

```text
T45 ──┬──→ T46a ──→ T46b ──┬──→ T46c ──┐
      │                    └──→ T47a ──┼──────────────┐
      └──→ T48a ───────────────────────┴──→ T48b ──┬──→ T47b ──┬──→ T48c ──────────────────────┐
                                                   │           ├──→ T47c ──┐                   │
                                                   └──→ T47d ──┴───────────┴──→ T47e ──→ T47f ──→ T47g ──┤
                                                                                                        │
T47g・T48c ──┬──→ T49a(シナリオ)──┐
             └──→ T49b(計測)──────┴──→ T49c(画面とスモーク)──→ T49d(Rust)──→ T50a(人間)──→ T50b
```

細かい依存(図は省略して書いた): T46c ← T46b。T47a ← T46b。T48b ← T46c・T48a。T47b ← T46c・T47a・T48b。T47d ← T48b。T47c ← T47b。T48c ← T47b。T47e ← T47b・T47d。T47f ← T47c・T47e。T47g ← T47f。T49a・T49b ← T47g・T48c。

M4.5(T63 で分解し直した。T64〜T66 は親で、矢印は子タスクの間に引いた。同時に動かすのは 2 件まで。2026-11-15 の要件の追加):

```text
T63(文書。済み)──┐
T50b ──────────────┴──┬──→ T64a(規則)──────→ T64c(23 行目と見本)──┐
                      └──→ T64b(結果の型)──→ T65a(読み込み)───────┴──→ T65b(起動時の反映)──┬──→ T65c(監視)──┐
                                                                                             └──→ T65e(画面)──┴──→ T65d(開く)──┬──→ T66a(シナリオ・スモーク)──┐
                                                                                                                                  └──→ T66b(文書)───────────────┴──→ T66c(人間)──→ T66d
```

細かい依存(図は省略して書いた): T64a・T64b ← T63・T50b。T64c ← T64a。T65a ← T64b。T65b ← T64a・T64b・T64c・T65a。T65c・T65e ← T65b。T65d ← T65c・T65e。T66a・T66b ← T65d。T66c ← T66a・T66b。T66d ← T66c。【要確認】3 が A なら T65c を外し、4 が B なら T64c と T65d を外す(前提の表)。

M5〜M6(粗い):

```text
T66 ──┬──→ T51 ──→ T53(人間) ──┐
      └──→ T52 ────────────────┴──→ T54(人間)

T54 ──→ T55 ──→ T56 / T57 / T58 / T59 / T60 / T61 ──→ T62(人間)
```

## テスト戦略

方針は設計書 §10 のとおり。ここには、どのタスクでどの種類のテストを足すかだけを書く。

### ユニットテスト

- Rust(`cargo test`、`arin-core`): 起動の仕様(T14)、OSC の走査(T23、T48)、プロジェクトと色(T29)、状態の読み書きと保存のまとめ(T30)、ストアをまたぐ操作(T32)、パス整形とシェルエスケープと除外(T38)、ファイル操作の手順(T39。偽のポート)、タイルのグリッドとページ(T46)、ショートカットの設定の規則と設定ファイルの結果(T64a・T64b・T64c)、設定の検証と破損時の回復(T56)
- TypeScript(Vitest、`src/logic` と `src/ipc`): 通信の包みとストアの置き換え(T8)、ターミナルの管理(T16、T18)、キーの振り分け(T20)、ツリーの差分・保留・ドラッグ・メニュー(T41)
- 検査スクリプト(`node --test`): 境界検査(T6)
- カバレッジ: 80% を `arin-core` と `src/logic` のそれぞれで見る(T9 で仕組みを入れる)。`arin-platform`、`src-tauri`、`src/ui` は対象外
- 時間に依存するロジックは時計を注入し、偽の時計で進める

### 結合テスト

- 実 PTY(T15、T35): 直列で実行し、待ち時間に上限を付ける。`which claude` は環境に依存するので自動テストにせず、`PATH` の一致で代える
- 実ファイル(T31、T40、T44): 一時ディレクトリの中だけ。ゴミ箱はテストが入れた項目だけを片付ける
- コマンド(T5、T8、T17、T33、T42): Tauri の模擬ランタイム(使えるかは T5 で確かめる)

### E2Eテスト

- Playwright のブラウザ方式(WebKit)。Rust 側は偽物で、型は Rust から生成したものを使う
- M0: 画面が開き準備完了が届く(T11)
- M1: 出力の表示、キー入力、リサイズ、スクロールバック、コピー・ペースト、9 枚への取り込み(T19、T20、T22)
- M2: 起動 → プロジェクト追加 → 切替 → タブ操作 → 削除の確認 → 終了の確認(T34、T35)
- M3: ツリーの表示、コンテキストメニュー、新規作成、名称変更、衝突の確認、ドラッグ中の更新の保留(T43、T44)
- M4: タイルのグリッド、10 枚で `1/2`、ページ送り、タイルとカードの連動、拡大、アテンションの印(T47f、T47g)、M4 のシナリオ(T49a)。描画方式ごとの 9 枚の反応の記録は `npm run perf` から動かす(T49b。`npm run e2e` には入れない)
- 実アプリの起動スモーク(`npm run smoke`): T10 で作り、各タスクの着手前に通す
- ブラウザ方式では Rust 側の実物と WKWebView 固有の挙動を通らない。そこは結合テストと人間ゲートで補う

### 人間ゲート

| マイルストーン | 人間がやること | タスク |
| ---- | ---- | ---- |
| M0 | `.app` の起動を見る。push して CI の緑を見る | T13 |
| M1 | Claude Code と Codex で Go/No-Go を判定する | T26 |
| M2 | 実機の体感確認 | T36 |
| M3 | ゴミ箱の「戻す」の目視(T37)。1 週間のドッグフーディング(T45) | T37、T45 |
| M4 | 9 枚同時表示の体感の判定、描画方式と全角幅の方式の選択 | T50a |
| M4.5 | 設定ファイルでキーを変え、メニューバーとツリーの両方で効くこと・保存ですぐ替わりシェルが続くことの確認。ADR-012 の【要確認】と PRD への反映の判断 | T66c |
| M5 | 証明書と API キーの用意、署名・公証の実行、別アカウントでの起動確認 | T53、T54 |
| M6 | ライト・ダークのコントラストの確認。不満リストが空になったことの確認 | T56、T62 |

### 性能の計測(記録だけ。合否にしない)

| NFR | 時機 | タスク |
| ---- | ---- | ---- |
| NFR-004 | M1 | T24 |
| NFR-009 | M1(簡易)、M4 | T24、T49b |
| NFR-005 | M3 | T44 |
| NFR-001〜003 | M2 の完了時、M6 の完了時 | T36、T62 |

## ドキュメント更新計画

### project-config.md

- 本計画では変更していない
- §2(技術スタック): T1 で決めた React・zustand などの版、T26 で決めた描画方式とアドオンを、T13 と T27 で反映する
- §3(コマンド): T7 で実装した内容と食い違いがあれば T13 で直す。`dist:mac` は T51
- §11(既知の落とし穴): 実装中に見つけたものを追記する(重複を確認してから)
- 人間が直す必要がある箇所(AI は変更できない節。報告だけ):
  - §1「Node.js要件: なし(Node.js は使用しない)」は Tauri の採用と食い違っている(Node v24 と npm を使う)
  - §1「対応環境」に「(仮定。REQ_001 §11 Q2)」が残っている(PRD の C-02 で確定済み)
  - §6「テストカバレッジ目標」が「ロジック層の SPM ターゲットが対象」「※仮定」のまま(設計書 §15.1 #8 で `arin-core` と `src/logic` に確定済み)
  - §4(アーキテクチャ)と §8(E2E テスト環境)が未記入

### docs/

- 本計画では変更していない。下の同期は `/implementing-features` が行う
- `docs/project.md`: コマンドと技術スタック(T13)、ストア一覧(T27、T36、T45)
- `docs/architecture.md`: ディレクトリ構成とテスト構成(T13 で初版、以降は各マイルストーンの完了時)
- `docs/data-model.md`: `state.json` 版 1(T36)、`settings.toml`(`[shortcuts]` は T66b、ほかの節は T62)
- `docs/development-patterns.md`: 実装中に見つけた規約と落とし穴
- `docs/release-notarization.md`: T51 で新規作成

### output/design/

- `ARCH_multi-terminal.md` §4: T1 で React 前提に直す
- ADR-003(T1)、ADR-002・004・005・006(T2)、ADR-001(T27。M4 の実測は T49b、決定は T50b)、ADR-007(T37)、ADR-011(T46 の再分解。状態は T50b で本人の了承に合わせる)、ADR-012(T63。クレートの版の追記は T65a、状態は T66d で本人の了承に合わせる)

## リスク・懸念事項

設計書と PRD を読んで気づいた点。番号は本文から参照している。

| # | 内容 | 種別 | 対応 |
| - | ---- | ---- | ---- |
| 1 | 設計書 §3.1 は `arin-core` の依存を `serde`・`serde_json`・`thiserror` だけとしているが、§15.1 #7 の `ts-rs`(Rust の型から TypeScript の型を生成)は、型を定義するクレートに依存として入れる必要がある。状態の写しの型は `arin-core` にある | **確定: A**(2026-11-14 本人承認) | A(推奨): `ts-rs` を `arin-core` の許可する依存に加える(型の生成だけで入出力をしないため)/ B: 画面へ渡す型を `src-tauri` に別に定義し、`arin-core` の型から変換する(`arin-core` は汚れないが、型が二重になる)。T1 で決め、T6 の検査と T8 の実装に反映する。同じ問題が、プロジェクト ID の UUID の生成(M2)と TOML の解析(M6)にもある。M2 と M6 の再分解のときに決める |
| 2 | 設計書 §3.3 の「画面への文字列の差し込み」の禁止リスト(`innerHTML` など)は、フレームワークなしの前提で書かれている。React では `dangerouslySetInnerHTML` が同じ働きをする | **確定: A**(2026-11-14 本人承認) | A(推奨): 禁止リストに `dangerouslySetInnerHTML` を足す / B: 足さない。T1 で決め、T6 に反映する |
| 3 | FR-055(タイルのアテンション。M4)は、BEL と OSC 9/777 の検出に依存するが、検出を作る FR-045 は M6 | **確定: A**(2026-11-14 本人承認) | A(本計画の案): 走査器と判定だけ M4 に前倒しする(T48)。カードの印、タブのバッジ、macOS 通知は M6 のまま / B: FR-055 を M6 に回す(M4 の対象 FR が変わるので PRD §9 の修正が要る) |
| 4 | FR-058(状態ファイルが壊れていたら退避して空で始める)は Should で M6。M2 で `state.json` の読み込みを作る時点で、読めなかったときの振る舞いが決まっていない | **確定: A**(2026-11-14 本人承認) | A(推奨): 「退避して空で始める」だけ M2(T30・T31)で作り、FR-058 の AC の確認は M6 に残す(壊れたファイルを上書きして失うのを避ける)/ B: M2 では起動を止めてエラーを出すだけにする。T28 で決める |
| 5 | M1 は単一プロジェクトだが、プロジェクトの登録は M2。M1 のターミナルの cwd が決まっていない | 【仮定】 | ホームフォルダにする。起動の仕様は cwd を引数で受けるので、M2 でプロジェクトのルートに替えるだけで済む |
| 6 | Go/No-Go の結果と各マイルストーンの AC の合否一覧の出力先 `testreport/` は Git の管理外(`.gitignore`) | 懸念 | Go/No-Go の結論は ADR-001(T27)にも書く。合否一覧を残したいなら `output/reports/test/` にも置く(T13 で本人に確認する) |
| 7 | M1 が No-Go になると M2 以降の計画は作り直しになる | 高 | M2 以降を粗い分解にとどめた。M1 までの成果のうち、`arin-core` と `arin-platform` の PTY は Swift へ戻す場合は使えない |
| 8 | 日本語入力の不具合 #5778 が再現した場合の回避策は未検証 | 高 | T26 で再現を確かめる。再現したら T27 で調査タスクを起こす |
| 9 | T5 の Tauri の模擬ランタイム、T5 の `tauri build` のオプション、T12 の `macos-26` ランナーでの手順、T21 の Playwright の WebKit での WebGL、React 関係と TypeScript 6.0 系の組み合わせは、いずれも未検証 | 中 | 各タスクに代わりの方法を書いた。結果を PROGRESS に書く |
| 10 | M1 の検証用の仕組み(T21 の描画方式の切替、T22 の 9 枚の画面、T23 の記録)は開発ビルドだけで有効にする。リリースビルドに残ると N2(外部制御)に近づく | 中 | 環境変数で切り替え、ソケットや常設の口を作らない。T22 の画面は T49 で撤去する。T23 の走査はアテンションの走査器に引き継ぐ |
| 11 | M3 のドッグフーディングのあいだは取り消し(FR-032)が無い(PRD §11) | 中 | PRD のとおり。移動は確認ダイアログ、削除はゴミ箱で守る |
| 12 | `cargo install cargo-llvm-cov`(T9)と `npx playwright install webkit`(T11)は、手元の環境に道具を入れる | 低 | 実行前に本人に伝える |
| 13 | PRD §9 の依存では M4 は M2 に、M5 は M0 にだけ依存する。本計画は指示どおり M0→M6 の一列で並べた | 参考 | M3 のドッグフーディングの 1 週間のあいだに M4 や M5 を進める並べ替えは、依存の上ではできる。採るかは本人が決める |
| 14 | M4 で足すショートカット(⇧⌘G・⌥⌘←・⌥⌘→・⇧⌘↩)とページ送り・拡大の振る舞いは PRD に無い | 【仮定】 | M4 の【仮定】3〜9。本人が T50a で確かめる。キーは表の 1 行の変更で替えられ、M4.5 で設定ファイルからも変えられる |
| 15 | WebGL 描画で 9 枚を同時に出すと、WebView のコンテキストの数の上限に近づく(ADR-001) | 中 | 隠したページは `setVisible(false)` でコンテキストを手放す(T47d)。T49b でコンテキストの数を記録し、本人が T50a で比べてから方式を決める |
| 16 | タイル表示では写しを送る回数が増える(フォーカス・ページ・アテンション)。写しに通し番号が無く、順が入れ替わると表示が前に戻りうる(M2 の残りの問題) | 低 | T48c は印が立ったときだけ送る。症状が T49a・T50a で出たら通し番号を足すタスクを起こす |
| 17 | 9 枚の検証画面は `src-tauri/tests/` の多くの結合テストの足場になっている | 中 | 撤去を画面(T49c)と Rust(T49d)に分け、T49d で足場を「一時フォルダのプロジェクト」に置き換える。テストの数が減らないことを前後で比べる |
| 18 | M4.5 は PRD に無い要件で、ADR-012 に【要確認】が 4 つある(割り当てを外す書き方、⌘C・⌘X・⌘V、反映のタイミング、説明の置き場所) | 【要確認】 | 推奨案で計画を組んだ。回答が違えばタスクを直す(M4.5 の前提の表)。T64a の着手までに回答をもらえると手戻りが無い。PRD への反映(FR と AC-K)は T66c で本人が判断する |
| 19 | 予約のキーの一覧(ADR-012 の 3)は手で持つので網羅できない。本人が「システム設定」で足した OS のショートカットも検査できない | 低 | 一覧に無いキーは割り当てても効かないことがある。見本に確かめ方を書く(T64c)。T66c で本人が足し引きする |
| 20 | `toml` が macOS 向けの依存の木に入っていない、またはその版が公開から 2 週間未満の可能性(この計画の作成ではコマンドを動かしていない) | 中 | T65a の着手時に `cargo tree`・`cargo info` で確かめ、合わなければ止めて報告する(別の版を推測で選ばない。ADR-003・ADR-009) |
| 21 | 状態フォルダの監視(T65c)は、シンボリックリンクの `settings.toml` のリンクの先の変更を拾えない。状態フォルダには `state.json` の書き込みの知らせも届く | 低 | 名前で `settings.toml` だけを読み直す。リンクの先を変えたときの対処(リンクを触る・再起動)を見本と `docs/` に書く |
| 22 | T50b(M4 の最後)が済んでいない時点で T63 を行った。T50b は M4.5 と同じファイル(`lib.rs`・`commands/app.rs`・`src/ipc/client.ts`・生成物・スモーク)を触る | 中 | T64a・T64b の依存に T50b を入れた(前提 (h))。T50b で描画方式を設定ファイルの項目に回すと決めた場合は、その項目を T56 に置き、M4.5 には足さない |
