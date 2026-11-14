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

- [ ] T22 — 9 枚同時表示と非表示インスタンスの検証用の画面を作る(開発ビルドだけ)(変更ファイル: src/ui/terminals/VerificationGrid.tsx, src/ui/shell/AppShell.tsx, src-tauri/src/lib.rs, crates/arin-core/src/workspace/ | 依存: T21)
  - 対応: NFR-009 の簡易確認、設計書 §2.2「9 枚の簡易確認」「非表示のインスタンス」。開発用の指定があるときだけ、ターミナルを 9 枚起動して 3×3 に並べる。1 枚だけ表示に切り替える操作(残り 8 枚は隠す)も付ける。WebGL のとき、描画コンテキストの数の上限に当たらないかを見られるようにする
  - 隠したまま作ったインスタンスの行・桁数は、表示中のターミナルと同じ寸法で始める案で作る(設計書 §2.2)
  - これは M4 のタイル表示ではない。M4(T49)で撤去する
  - 並行: 不可
  - 先に書くテスト: Playwright で、9 枚ぶんの偽のチャネルに出力を流し、隠した 8 枚にも `write` が届き続ける(取り込みが済んだ量が返る)こと
  - 検証: `npm test && npm run e2e && npm run lint && npm run app`

#### Phase 16(逐次。T19〜T24 完了後)

- [ ] T25 — Go/No-Go の確認手順書を作る(変更ファイル: output/tasks/M1_GO-NO-GO_CHECKLIST.md | 依存: T19, T20, T22, T23, T24)
  - 対応: 設計書 §2.2 の 12 行(`ARCH_multi-terminal.md:189-202`)を、1 行ずつ「手順・見るところ・結果の欄(DOM / WebGL)」にする。下の「M1 で確かめる項目の割当」の表をもとにする
  - 確認は `npm run app` で作った `.app` を Finder から開いて行う(`npm run dev` では環境変数と WKWebView の条件が変わるため)
  - 並行: 不可
  - 先に書くテスト: なし
  - 検証: 手順書に §2.2 の 12 行がすべて載っている(行数を数える)

#### Phase 17(逐次。人間ゲート)

- [ ] T26 — **M1 の Go/No-Go(人間が判定する)**(変更ファイル: output/tasks/M1_GO-NO-GO_CHECKLIST.md | 依存: T25)
  - **人間がやること**: T25 の手順書に沿って、Claude Code と Codex を実際に動かし、結果を書き込む。Go / No-Go を決める。描画方式(DOM / WebGL)、全角幅のアドオン、OSC 52 を許すか(設計書 §15.2 #9)、流量制御のしきい値を決める
  - AI がやること: 手順の補助と、記録(T23)と計測(T24)の結果の整理だけ。合否は判定しない
  - No-Go のとき: 原因に応じて、描画方式の変更か実装技術の見直し(Swift ネイティブへ戻すことを含む)を本人が判断する(PRD §7)。T28 以降は止める
  - 日本語入力の不具合 #5778(英数キーでの確定で文字が二重になる)が再現したとき: 回避策の調査を別タスクとして T27 で起こす(いまは中身を決められない)

#### Phase 18(逐次。T26 完了後)

- [ ] T27 — ADR-001 を書き、M1 を締め、M2 以降を計画し直す(変更ファイル: output/design/ADR_001_terminal-engine.md, output/design/ADR_INDEX.md, output/tasks/PLAN_multi-terminal.md, output/tasks/PROGRESS.md, docs/project.md, docs/architecture.md, project-config.md | 依存: T26)
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

### M2 マルチプロジェクト(粗い分解。T28 で分解し直す)

完了条件: FR-001〜004、007〜010、042、044、056、059〜061(タブ表示の部分)、063、064 の AC 合格。

#### Phase 19(逐次。M1 が Go のとき)

- [ ] T28 — M2 の着手時の確認と再分解(変更ファイル: output/tasks/PLAN_multi-terminal.md, output/tasks/PROGRESS.md, output/design/ | 依存: T27)
  - (a) React のフック向けの lint を入れるか再検討する(設計書 §15.1 #10。`ARCH_multi-terminal.md:968`。Biome などを足すなら ADR に残し `npm run lint` に組み込む)
  - (b) ショートカットの受け口(メニューバーか画面内か)を実機で確かめる(設計書 §15.3 #7)。FR-044 と FR-006 の作り方が決まる
  - (c) Rust から macOS のブックマークを作って解決できることを小さなプログラムで確かめる(設計書 §5.4。Rust からは未実行)
  - (d) `state.json` が読めないときの M2 での振る舞いを決める(§リスク #4)
  - (e) T29〜T36 を 1 セッション粒度に分解し直す
  - 検証: (a) は `npm run lint`。(b)(c) は結果を PROGRESS に書く

#### Phase 20(並行可能。T28 完了後)

- [ ] T29 — プロジェクトの規則(`arin-core/project`)(変更ファイル: crates/arin-core/src/project/, crates/arin-core/src/ports/ | 依存: T28)
  - 対応: FR-001、003、010、059。先に書くテスト: 登録、二重登録の防止と入れ子の許可、削除、色の割当(8 色で 3 件がすべて違う。使用数が最少の色、同数ならパレットの順)。検証: `npm test`
- [ ] T30 — 状態の保存と復元、表示の状態(`arin-core/persistence`、`arin-core/layout`)(変更ファイル: crates/arin-core/src/persistence/, crates/arin-core/src/layout/ | 依存: T28)
  - 対応: FR-004、009、056。先に書くテスト: `state.json` 版 1 の読み書きの往復、欠けたキーを既定値で補う、保存のまとめ(偽の時計で 0.5 秒)、ペイン寸法の最小値。検証: `npm test`
- [ ] T31 — OS 側の実装(ブックマーク、ルートの位置、状態ファイル、ペーストボードの文字列)(変更ファイル: crates/arin-platform/src/bookmark/, crates/arin-platform/src/watch/, crates/arin-platform/src/storage/, crates/arin-platform/src/desktop/, crates/arin-platform/tests/ | 依存: T28)
  - 対応: FR-007、056、064。先に書くテスト(一時ディレクトリ): 改名と移動への追従、削除で「見つからない」、原子的な書き込みと権限(0700 / 0600)。検証: `npm test`

#### Phase 21(逐次。Phase 20 完了後)

- [ ] T32 — ストアをまたぐ操作とターミナルの複数化(`arin-core/workspace`、`arin-core/terminal`)(変更ファイル: crates/arin-core/src/workspace/, crates/arin-core/src/terminal/ | 依存: T29, T30)
  - 対応: FR-002、039(複数タブ、起動時に全プロジェクトで 1 タブ)、042、044(0 枚を許す)、063(動作中の一覧)。先に書くテスト: 切替、削除の手順、起動時の復元、タブの追加・削除・前後、状態の写しの中身。検証: `npm test`
- [ ] T33 — コマンド、イベント、メニューバー、終了の確認、見回り(変更ファイル: src-tauri/src/commands/, src-tauri/src/events.rs, src-tauri/src/menu.rs, src-tauri/src/lifecycle.rs, src-tauri/src/lib.rs, src-tauri/Cargo.toml, src/ipc/ | 依存: T31, T32)
  - 対応: FR-001(ダイアログとドロップ)、002、003、007、044、063、064。`tauri-plugin-dialog` を足す。先に書くテスト: コマンドの結合テスト。検証: `npm test && npm run lint`

#### Phase 22(逐次。Phase 21 完了後)

- [ ] T34 — 画面(カード一覧、ペインの分割、タブバー、確認ダイアログ、空の状態、プロジェクト色)(変更ファイル: src/state/, src/ui/shell/, src/ui/projects/, src/ui/terminals/, src/ui/dialogs/, src/ui/design/, src/logic/keys/, src/logic/menu/ | 依存: T33)
  - 対応: FR-001、002、003、007(「見つからない」と再リンク、ツールチップ)、008、009、010、044、059〜061(カードのタイトル部とタブ。ツリーのヘッダは M3)、063、064。先に書くテスト: `src/logic` の単体と、Playwright のシナリオ。検証: `npm test && npm run e2e`

#### Phase 23(逐次。Phase 22 完了後)

- [ ] T35 — 結合テストと画面の自動テストの仕上げ(変更ファイル: crates/arin-platform/tests/, src-tauri/tests/, e2e/, scripts/smoke.mjs | 依存: T34)
  - 対応: FR-042(AC-M04)、NFR-006、PRD §8「UI: 起動 → プロジェクト追加 → 切替」。検証: `npm test && npm run e2e && npm run smoke`
- [ ] T36 — M2 の完了確認(変更ファイル: output/tasks/PROGRESS.md, docs/, project-config.md | 依存: T35)
  - AC の合否一覧を `testreport/` に出す。NFR-001〜003 を計測して記録する(合否にしない)。`docs/` を同期する。**人間**: 実機で体感を確認する(マイルストーン末に 1 回)

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

M2〜M6(粗い):

```text
T27 ──→ T28 ──┬──→ T29 ──┐
              ├──→ T30 ──┴──→ T32 ──┐
              └──→ T31 ─────────────┴──→ T33 ──→ T34 ──→ T35 ──→ T36(人間)

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
