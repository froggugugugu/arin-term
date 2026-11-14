# ADR-006: 単体は `cargo test` と Vitest、画面は Playwright のブラウザ方式と起動スモークで見て、型は `ts-rs` で生成し、カバレッジ 80% は `arin-core` と `src/logic` のそれぞれに課す

- ステータス: Accepted
- 日付: 2026-11-14
- 決定者: 本人(設計ゲート)。§15.1 #5・#6・#7・#8 は、推奨どおりで進める旨を提示し、異論がなかったため推奨を採った
- 関連: `ARCH_multi-terminal.md` §1.4 #5・#18、§2、§3.1、§10、§13.1、§15.1 #5〜#8、§16、ADR-002、ADR-003

## コンテキスト

PRD は TDD とユニット・E2E のテストを求め、カバレッジの目標を 80% としている(`project-config.md` §6、C-15)。C-15 の対象は「ロジック層」で、その範囲は設計で決める必要があった。

ロジックの正本は Rust の `arin-core` にあり、画面の純粋ロジックは `src/logic` にある(ADR-002)。画面は React で作り、部品の単体テストは書かない(ADR-003)。

画面の自動テストには制約がある。

- Playwright は macOS の Tauri アプリ(WKWebView)を直接操作できない。Tauri 公式の WebDriver も macOS では使えない(WKWebView 用のドライバが無い)(§1.4 #18。Tauri 公式の WebDriver のページで確認)。
- WebdriverIO の `@wdio/tauri-service` は、アプリに WebDriver のサーバーを埋め込む方式で macOS に対応している(プラグインは 2026-05 公開、1.5.1)(§1.4 #18。crates.io で確認)。
- tadcap の Playwright は Tauri を起動せず、Vite の開発サーバーが配るページを Chromium で開き、Rust との通信を偽物に差し替えている(§1.4 #5)。

ブラウザ方式では画面が偽の Rust 側と通信するので、偽物が実物とずれると、テストが通っても実アプリで壊れる。

## 判断

単体テストは `cargo test` と Vitest で書き、画面の自動テストは Playwright のブラウザ方式(WebKit)と実アプリの起動スモークを併用し、Rust と TypeScript でやり取りする型は `ts-rs` で Rust から生成し、カバレッジ 80% は `arin-core` と `src/logic` のそれぞれに課す。

### テストの道具と構成(§15.1 #5)

| 種別 | ツール | 主な対象 | 配置 |
| ---- | ------ | -------- | ---- |
| 単体(Rust) | `cargo test` | `arin-core` 全体(偽のポートを使う) | `crates/arin-core/src/**` と `crates/arin-core/tests/` |
| 単体(TypeScript) | Vitest | `src/logic` 全体、`src/ipc` の包み、`src/state` の zustand のストア(React なしで動かす) | 対象の隣に `*.test.ts` |
| ファイル操作・監視・実 PTY | `cargo test`(実 PTY と FSEvents は直列実行。待ち時間に上限) | 実ファイルシステムと実 PTY を通す結合テスト | `crates/arin-platform/tests/` |
| 結合(コマンド) | `cargo test`(Tauri の模擬ランタイム) | コマンドが `Workspace` を正しく呼ぶこと | `src-tauri/tests/` |
| 画面(E2E) | Playwright(ブラウザ方式。WebKit) | 画面の操作と表示のシナリオ | `e2e/` |
| 実アプリのスモーク | スクリプト | ビルドした `.app` が起動し、画面が準備完了を通知すること | `scripts/smoke.mjs` |

全体の表は §10.1。入口は `npm test`、`npm run e2e`、`npm run smoke`、`npm run test:coverage`(§13.1)。

### 画面の自動テストの方式(§15.1 #6)

- Tauri を起動せず、Vite の開発サーバーが配る画面を Playwright で開く。Rust との通信は偽物(`e2e/fixtures/`)に差し替える。
- ブラウザは WebKit を主にする(tadcap は Chromium)。WKWebView と同じ系統のエンジンで動かすためである。ただし Playwright の WebKit は macOS 標準の WKWebView とは別のビルドで、同じ挙動は保証されない。
- ターミナルは xterm.js の実物を使い、偽のチャネルから出力を流す。
- Rust 側の実物は、結合テスト(実 PTY、実ファイル、コマンド)で通す。
- 実アプリの起動スモークで、実物が起動し画面と Rust がつながることを確かめる。

### 型の合わせ方(§15.1 #7)

- やり取りする型は Rust の型から `ts-rs`(12 系)で生成し、`src/ipc/generated/` に置く。手で編集しない。
- 偽の Rust 側(`e2e/fixtures/`)もこの型を使い、実物とのずれを型検査で捕まえる。
- `ts-rs` は `arin-core` の依存に加える。型の生成だけに使い、入出力をしない(§3.1)。

### カバレッジ 80% の対象(§15.1 #8)

- `arin-core` と `src/logic` の**それぞれ**で 80% 以上を実装ゲートの基準にする。合算では判定しない。
- 道具は `cargo-llvm-cov`(0.9 系。`llvm-tools-preview` が要る)と `@vitest/coverage-v8`(§2)。出力先は `testreport/coverage/`。
- 設計書が定めたのはこの 2 か所だけで、`arin-platform`、`src-tauri`、`src/state` などのカバレッジの目標は定めていない。

## 代替案

### A: 実アプリの自動操作(§15.1 #6 の B。WebdriverIO の `@wdio/tauri-service`)

- メリット: 実物の WKWebView と実物の Rust 側を通して操作できる。
- デメリット: Playwright とは別のテスト基盤が要り、基盤が 2 つになる。アプリに操作用のサーバーを埋め込むので、テスト用のビルドだけに限る必要がある(N2・NFR-008 に触れないようにするため)。
- 採らなかった理由: まずブラウザ方式で始め、WKWebView 固有の不具合が自動テストをすり抜けることが続いたら検討する進め方を採った(§15.1 #6)。入れる場合の差し込み方は §16(機能フラグでテスト用のビルドだけに足し、リリースビルドに入っていないことを検査する)。

### B: ブラウザ方式を Chromium で動かす(tadcap と同じ)

- メリット: tadcap の設定をそのまま使える。
- デメリット: 本番の WKWebView とはエンジンの系統が違う。
- 採らなかった理由: WKWebView に近い WebKit で動かすほうが、本番との差を小さくできるため。

### C: 型を手書きし、照合テストで実物とのずれを見る(§15.1 #7 の B)

- メリット: `ts-rs` の依存が増えない。型の書き方を自由に決められる。
- デメリット: Rust と TypeScript に同じ型を 2 回書く。照合テストを自分で書いて保守する。
- 採らなかった理由: ロジックの正本が Rust にあり(ADR-002)、ブラウザ方式の E2E が偽の Rust 側を通るので、偽物の信頼性が重要になるため。設計書は「#1 が B なら重要度は下がる」としており、#1 が A に決まったことで生成の価値が上がった。

### 単体テストの道具とカバレッジの対象の別案について

§15.1 #5(単体テストの道具)と #8(カバレッジの対象)は推奨案だけが本文に残っており、設計書には別案の中身が記録されていない。そのため本 ADR では別案を挙げない。

## 影響

### ポジティブ

- 画面の E2E が速く、CI(`macos-26`)で安定して動く見込み。tadcap で実績のある方式である。
- 型を生成するので、偽の Rust 側と実物のずれを型検査で捕まえられる。
- カバレッジの基準をロジック層の 2 か所それぞれに課すので、片方の高さで他方の低さが隠れない。
- テスト用の DOM とライブラリ(jsdom、Testing Library)を足さずに済む(ADR-003)。

### ネガティブ

- ブラウザ方式の E2E は Rust 側の実物を通らず、WKWebView 固有の挙動も見られない。Playwright の WebKit と WKWebView の差は残る。
- 日本語入力や TUI の表示(FR-041)、ドラッグの体感などは自動テストでは見られず、人間ゲートに頼る(§10.1)。
- 実 PTY と FSEvents のテストは直列で、時間がかかる。
- `cargo-llvm-cov` はこの Mac に入っていない(§1.4 #2)。M0 で `cargo install cargo-llvm-cov` と `rustup component add llvm-tools-preview` が要る(§13.1)。
- `ts-rs` は §3.1 で `arin-core` の依存に加えたが、T3 の時点の `crates/arin-core/Cargo.toml` にはまだ無い(2026-11-14 にこのファイルを読んで確認)。型の生成を始めるタスクで足す必要がある。
