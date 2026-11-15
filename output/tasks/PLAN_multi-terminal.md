# 設計ドキュメント: ライト・マルチターミナル「Arin」実装計画

> 生成元: `output/prd/PRD_multi-terminal.md`(承認済み。FR-001〜064、NFR-001〜009)、`output/design/ARCH_multi-terminal.md`(承認済み)
> 生成日: 2026-11-14
> ステータス: レビュー待ち(タスク分解ゲート。承認されるまで実装に着手しない)
> 進捗の引き継ぎ: `output/tasks/PROGRESS.md`

**この文書の読み方**

- マイルストーンは PRD §9 の M0〜M6 をそのまま使う(`PRD_multi-terminal.md:689-701`)。実行順は M0→M1→M2→M3→M4→M5→M6。各マイルストーンの完了条件と人間ゲートは変えていない。
- **M0 と M1 は 1 タスク=1 セッションで終わる粒度まで分解した(T1〜T27)。M2 以降(T28〜T62)は粗い分解にとどめた。** M1 の Go/No-Go の結果(描画方式、CSP、OSC 52、実装技術の見直しの有無)で中身が変わりうるため。各マイルストーンの最初のタスクで `/plan` を再実行し、1 セッション粒度に分解し直す。
- タスク ID は全体で通し番号(T1〜T62)。Phase も全体で通し番号で、Phase 内のタスクは同時に着手できる。
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
- [ ] T35b — 実物の結合テストと計測の準備(変更ファイル: src-tauri/tests/session_pty.rs, crates/arin-platform/tests/pty.rs, scripts/perf/, scripts/perf-run.test.mjs | 依存: T34d)
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

### M3 ツリーとファイル操作(粗い分解。T37 で分解し直す)

完了条件: FR-013〜018(Quick Look を除く)、024〜030、035〜037 の AC 合格と、VSCode を閉じて 1 週間のドッグフーディング。

#### Phase 24(逐次。M2 完了後)

- [ ] T37 — M3 の最初の検証と再分解(変更ファイル: output/design/ADR_007_finder-integration.md, output/design/ADR_INDEX.md, output/tasks/PLAN_multi-terminal.md, output/tasks/PROGRESS.md | 依存: T36)
  - (a) **ゴミ箱の「戻す」**(設計書 §15.3 #3。`ARCH_multi-terminal.md:997`): 一時ディレクトリのファイルで、`trashItemAtURL` → `NSWorkspace` の `recycleURLs` → Finder に頼む方式 の順に試す。Finder のゴミ箱に「戻す」が出るかは**人間が目で見る**。テストが入れた項目だけを片付ける
  - (b) アプリ内のドラッグ(設計書 §15.3 #4): マウスのイベントによる自前の実装で、ツリーからターミナルへ(FR-016)とツリー内(FR-029)が成り立つかを実機で確かめる。HTML 標準のドロップが届かないこと(設計書 §1.4 #8)も実機で確かめる
  - (c) ツリーの仮想スクロールを自作するか React 用のライブラリを使うかを決める(設計書 §9.1。足すなら ADR)
  - (d) 結果を ADR-007 に書き、T38〜T45 を 1 セッション粒度に分解し直す
  - 検証コードは使い捨てにする(スクラッチパッドに置き、リポジトリに入れない)

#### Phase 25(並行可能。T37 完了後)

- [ ] T38 — パスと除外の規則(`arin-core/paths`、`arin-core/tree`)(変更ファイル: crates/arin-core/src/paths/, crates/arin-core/src/tree/ | 依存: T37)
  - 対応: FR-015、016、017。先に書くテスト: 相対・絶対パス(`src/a b.ts`)、シェルエスケープ(`src/a\ b.ts` と末尾の空白、制御文字の無害化)、除外の判定。検証: `npm test`
- [ ] T39 — ファイル操作の手順(`arin-core/fileops`)(変更ファイル: crates/arin-core/src/fileops/, crates/arin-core/src/ports/ | 依存: T37)
  - 対応: FR-025〜030、035、036。先に書くテスト(偽のポート): 対象範囲の検査、名前の検証、実行時の存在確認、衝突の 3 択、切り取りとペースト。取り消しの履歴は M6(T59)。検証: `npm test`
- [ ] T41 — 画面の純粋ロジック(`src/logic`)(変更ファイル: src/logic/tree/, src/logic/dnd/, src/logic/keys/, src/logic/menu/ | 依存: T37)
  - 対応: FR-014、024、035、037。先に書くテスト: 差分の適用、更新の保留(100 ファイルの追加を保留して後からまとめて反映)、ドラッグの状態、メニュー項目と有効・無効。検証: `npm test`

#### Phase 26(逐次。T39 完了後)

- [ ] T40 — OS 側の実装(フォルダの読み込み、上書きしない操作、ゴミ箱、監視、ペーストボード、Finder 連携)(変更ファイル: crates/arin-platform/src/fs/, crates/arin-platform/src/watch/, crates/arin-platform/src/desktop/, crates/arin-platform/tests/ | 依存: T38, T39)
  - 対応: FR-013、014、018(Finder で表示、開く)、027、028、030、036。`notify` 8.2 系を足す。先に書くテスト(一時ディレクトリ): 既にあれば失敗する、ゴミ箱、監視の 2 秒以内の検知、操作の途中での外部からの削除・追加。検証: `npm test`

#### Phase 27(逐次。Phase 25・26 完了後)

- [ ] T42 — ツリー・ファイル操作・デスクトップのコマンドとイベント、監視の張り替え(変更ファイル: src-tauri/src/commands/, src-tauri/src/events.rs, src-tauri/src/lib.rs, src/ipc/ | 依存: T40)
  - 対応: FR-013〜018、024〜030、035、036。画面からは絶対パスを受け取らない。検証: `npm test && npm run lint`
- [ ] T43 — 画面(ツリー、インライン編集、コンテキストメニュー、衝突の確認、ドラッグ)(変更ファイル: src/state/, src/ui/tree/, src/ui/menu/, src/ui/dialogs/, src/ui/terminals/ | 依存: T41, T42)
  - 対応: FR-013〜018、024〜030、035〜037、FR-060(ツリーのヘッダ)。検証: `npm test && npm run e2e`

#### Phase 28(逐次。Phase 27 完了後)

- [ ] T44 — 同時操作のテスト、画面の自動テスト、NFR-005 の計測(変更ファイル: crates/arin-platform/tests/, e2e/, scripts/perf/ | 依存: T43)
  - 対応: FR-036、037、NFR-005(10 万ファイルの見本を生成して展開の時間を記録。合否にしない)。検証: `npm test && npm run e2e && npm run perf`
- [ ] T45 — M3 の完了確認とドッグフーディング(変更ファイル: output/tasks/PROGRESS.md, output/tasks/DOGFOODING_NOTES.md, docs/, project-config.md | 依存: T44)
  - AC の合否一覧を出す。**人間**: VSCode を閉じて 1 週間使い、不満を `DOGFOODING_NOTES.md` に書く(M6 の完了条件の「不満リスト」になる)。期間中は取り消し(FR-032)が無い点に注意する(PRD §11)

### M4 タイル表示(粗い分解。T46 の着手時に分解し直す)

完了条件: FR-048、051〜055、FR-060・061 のタイル部分の AC 合格と、9 枚同時表示の体感の判定(人間)。

#### Phase 29(並行可能。M3 完了後)

- [ ] T46 — タイルの規則(`arin-core/layout`、`arin-core/workspace`)(変更ファイル: crates/arin-core/src/layout/, crates/arin-core/src/workspace/ | 依存: T45)
  - 対応: FR-048、051、052、053、054。先に書くテスト: グリッド(1→1×1、2→2×1、3〜4→2×2、5〜6→3×2、7〜9→3×3)、10 枚で `1/2`、並び順(プロジェクト順 → タブ順)、タイルとカードの連動、拡大と復帰、表示モードの保存。検証: `npm test`
- [ ] T48 — アテンションの走査を前倒しで作る【要確認】(変更ファイル: crates/arin-core/src/terminal/, src-tauri/src/sessions.rs | 依存: T45)
  - 対応: FR-055。FR-055(M4)は、FR-045(M6)が作るはずの BEL と OSC 9/777 の検出に依存している(§リスク #3)。本計画では走査器と判定を M4 に前倒しし、カードの印・タブのバッジ・macOS 通知は M6(T57)に残す案で書いた
  - 先に書くテスト: BEL 単独、OSC 9、OSC 777 の検出。OSC の終端としての BEL と OSC 9;4 を除く。チャンクの切れ目をまたぐ列。表示中かどうかの判定(タイル表示=表示中ページ)。検証: `npm test`

#### Phase 30(逐次。Phase 29 完了後)

- [ ] T47 — 画面(タイルのグリッド、ページ送り、タイトルバー、拡大、モード切替)(変更ファイル: src/ui/terminals/, src/state/, src/logic/keys/, src-tauri/src/commands/, src-tauri/src/menu.rs, src/ipc/ | 依存: T46, T48)
  - 対応: FR-048、051〜055、060、061。表示中ページのタイルだけを画面に出す(NFR-009)。検証: `npm test && npm run e2e`

#### Phase 31(逐次。Phase 30 完了後)

- [ ] T49 — 画面の自動テスト、NFR-009 の計測、M1 の検証用の画面の撤去(変更ファイル: e2e/, scripts/perf/, src/ui/terminals/VerificationGrid.tsx, src/ui/shell/AppShell.tsx | 依存: T47)
  - 対応: AC-M14、AC-M15、AC-C02、NFR-009(9 枚で同時に `yes` を流して記録。合否にしない)。T22 の検証用の画面を削除する。検証: `npm test && npm run e2e && npm run perf`
- [ ] T50 — M4 の完了確認(変更ファイル: output/tasks/PROGRESS.md, docs/ | 依存: T49)
  - AC の合否一覧を出す。**人間**: 9 枚同時表示の体感を判定する

### M5 公証・配布(粗い分解。秘密情報を扱う作業は人間)

完了条件: 別ユーザーアカウントで Gatekeeper の警告なく起動する。PRD の依存は M0 だけなので前倒しもできるが、本計画では M4 の後に置いた。

#### Phase 32(並行可能。M4 完了後)

- [ ] T51 — 署名・公証のスクリプトと手順書を書く(AI。実行はしない)(変更ファイル: scripts/package-mac.sh, package.json, src-tauri/tauri.conf.json, docs/release-notarization.md | 依存: T50)
  - 対応: G5、PRD §7 C1・C2。tadcap の `scripts/package-mac.sh` と `docs/release-notarization.md` を下敷きにする(Tauri が `.app` を署名・公証し、スクリプトが `.dmg` を署名・公証・staple して検証する)。`npm run dist:mac` を足す
  - 着手時に決める【要確認】: エンタイトルメント(設計書 §15.3 #2)、配布物の置き場所(同 #11)
  - 秘密情報(証明書、API キー、パスワード)は読まない・書かない。スクリプトは環境変数の名前だけを参照する
  - 検証: `bash -n scripts/package-mac.sh`(構文だけ)。`npm run lint`
- [ ] T52 — tag のときの公証のワークフローを書く(AI)(変更ファイル: .github/workflows/release.yml | 依存: T50)
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
M0 ──→ M1 ──(Go)──→ M2 ──→ M3 ──→ M4 ──→ M5 ──→ M6
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

M3〜M6(粗い):

```text
T36 ──→ T37(人間の目視あり) ──┬──→ T38 ──┐
                               ├──→ T39 ──┴──→ T40 ──→ T42 ──┐
                               └──→ T41 ─────────────────────┴──→ T43 ──→ T44 ──→ T45(人間。1 週間)

T45 ──┬──→ T46 ──┐
      └──→ T48 ──┴──→ T47 ──→ T49 ──→ T50(人間)

T50 ──┬──→ T51 ──→ T53(人間) ──┐
      └──→ T52 ────────────────┴──→ T54(人間)

T54 ──→ T55 ──→ T56 / T57 / T58 / T59 / T60 / T61 ──→ T62(人間)
```

## テスト戦略

方針は設計書 §10 のとおり。ここには、どのタスクでどの種類のテストを足すかだけを書く。

### ユニットテスト

- Rust(`cargo test`、`arin-core`): 起動の仕様(T14)、OSC の走査(T23、T48)、プロジェクトと色(T29)、状態の読み書きと保存のまとめ(T30)、ストアをまたぐ操作(T32)、パス整形とシェルエスケープと除外(T38)、ファイル操作の手順(T39。偽のポート)、タイルのグリッドとページ(T46)、設定の検証と破損時の回復(T56)
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
- M4: 10 枚で `1/2`、ページ送り、タイルとカードの連動、拡大(T49)
- 実アプリの起動スモーク(`npm run smoke`): T10 で作り、各タスクの着手前に通す
- ブラウザ方式では Rust 側の実物と WKWebView 固有の挙動を通らない。そこは結合テストと人間ゲートで補う

### 人間ゲート

| マイルストーン | 人間がやること | タスク |
| ---- | ---- | ---- |
| M0 | `.app` の起動を見る。push して CI の緑を見る | T13 |
| M1 | Claude Code と Codex で Go/No-Go を判定する | T26 |
| M2 | 実機の体感確認 | T36 |
| M3 | ゴミ箱の「戻す」の目視(T37)。1 週間のドッグフーディング(T45) | T37、T45 |
| M4 | 9 枚同時表示の体感の判定 | T50 |
| M5 | 証明書と API キーの用意、署名・公証の実行、別アカウントでの起動確認 | T53、T54 |
| M6 | ライト・ダークのコントラストの確認。不満リストが空になったことの確認 | T56、T62 |

### 性能の計測(記録だけ。合否にしない)

| NFR | 時機 | タスク |
| ---- | ---- | ---- |
| NFR-004 | M1 | T24 |
| NFR-009 | M1(簡易)、M4 | T24、T49 |
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
- `docs/data-model.md`: `state.json` 版 1(T36)、`settings.toml`(T62)
- `docs/development-patterns.md`: 実装中に見つけた規約と落とし穴
- `docs/release-notarization.md`: T51 で新規作成

### output/design/

- `ARCH_multi-terminal.md` §4: T1 で React 前提に直す
- ADR-003(T1)、ADR-002・004・005・006(T2)、ADR-001(T27)、ADR-007(T37)

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
