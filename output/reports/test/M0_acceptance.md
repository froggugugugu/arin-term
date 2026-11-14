# M0(骨格)完了条件の合否

- 実行日: 2026-11-15
- 対象: ブランチ `feat/m0-skeleton`、コミット `049bdb6`(このあとに入った変更は `docs/` と `output/` の文書だけ)
- 実行環境: 本人の Mac(Apple Silicon、Command Line Tools、Rust 1.98.1、Node v24.15.0)
- 完了条件(PRD §9): CI 緑、アプリ生成コマンドで作った `.app` が起動する

## 完了条件

| 条件 | 判定 | 根拠 |
| ---- | ---- | ---- |
| アプリ生成コマンドで作った `.app` が起動する | 自動確認は合格。**目視は未実施** | `npm run app` が成功し、`npm run smoke` が `.app` を起動して、画面からの準備完了の通知を確認した。ウィンドウの見た目は人が見ていない |
| CI 緑 | **未確認** | `.github/workflows/ci.yml` を追加した。push していないので、CI はまだ 1 度も走っていない。`macos-26` のランナーで Rust 1.98.1 と Playwright の WebKit が入るかは未検証 |

## 検証コマンドの結果

7 つを 1 つずつ実行した。すべて終了コード 0。

| コマンド | 終了コード | 内容 |
| -------- | ---------- | ---- |
| `npm run build` | 0 | 型検査、画面のビルド、Rust のビルド |
| `npm test` | 0 | Vitest 4 ファイル 17 件、`cargo test` 32 件、`node --test` 40 件。失敗 0 |
| `npm run lint` | 0 | 型検査、clippy(`-D warnings`)、`cargo fmt --check`、境界検査 10 項目(違反なし) |
| `npm run app` | 0 | `target/aarch64-apple-darwin/release/bundle/macos/Arin.app` を生成 |
| `npm run smoke` | 0 | `.app` を一時の状態フォルダで起動し、0.6 秒で準備完了の印を確認して終了させた |
| `npm run e2e` | 0 | Playwright(WebKit)3 件合格、1.3 秒 |
| `npm run test:coverage` | 0 | 総合で合格(下の表) |

実行後の状態:

- Arin のプロセスは残っていない(`pgrep -fl Arin.app` が該当なし)
- 1420 番のポートを使うプロセスは残っていない(`lsof -i :1420` が該当なし)
- 本物の `~/Library/Application Support/Arin/` は作られていない

## カバレッジ

基準は行のカバレッジ 80%。`src/logic` と `arin-core` のそれぞれに課す。

| 対象 | 行 | 判定 |
| ---- | -- | ---- |
| `src/logic` | 100.00%(1/1) | 合格 |
| `arin-core` | 100.00%(51/51) | 合格 |
| `src` のその他 | 72.92%(35/48) | 記録のみ |
| `arin-platform` | 71.43%(10/14) | 記録のみ |
| `src-tauri` | 78.57%(77/98) | 記録のみ |

コードがまだ少ないので、100% という数字に大きな意味は無い。

## 生成した `.app`

| 項目 | 値 | 確かめ方 |
| ---- | -- | -------- |
| Bundle ID | `com.froggugugugu.arin` | `Info.plist` の `CFBundleIdentifier` |
| 最低 OS | 15.0 | `Info.plist` の `LSMinimumSystemVersion` |
| アーキテクチャ | arm64 | `file` コマンド |
| サイズ | 9.4MB | `du -sh` |
| 署名 | していない | M5 で行う |

## タスクごとの状態

| タスク | 状態 |
| ------ | ---- |
| T1〜T12 | 完了。T1 だけ、本人による差分の了承が残っている |
| T13 | AI の分は完了。人間の分(下)が残っている |

## 人間が行う確認(未実施)

1. `npm run app` で作った `Arin.app` を開き、空ウィンドウ(「Arin」の見出しとアプリの版)が出ることを見る。未署名なので、初回は Finder で右クリックして「開く」が要る可能性がある
2. push して、CI が緑になることを見る
3. T1 の差分(設計書 §3・§4・§9〜§12 と ADR-003。コミット `b938d3d`)を読んで了承する

## 未検証の点・残っている課題

- 開発時(`npm run dev`)の動作は確かめていない。GUI を起動して待ち続けるコマンドを使っていないため
- 開発時の CSP は適用されない見込み(Tauri のソースを読んだ結果で、実機では未確認)
- アイコンは仮のもの(単色の正方形)
- 公開から 2 週間未満のクレートが、推移的な依存に 24 個ある(Tauri が引き込むもの)。直接指定した依存はすべて 2 週間以上前の版
- `Cargo.lock` の `reqwest` は macOS 向けには組み込まれないが、境界検査では例外として扱っている。Tauri の版を上げるときに確かめ直す必要がある
- 設計書に、React への変更前の表現が数か所残っている(`src/ui` が「DOM の部品」、確定済みの項目に「推奨」の表記)
- `playwright.config.ts` と `vite.config.ts` は型検査の対象に入っていない(`@types/node` を入れていないため)
- カバレッジは CI では計測していない(`cargo-llvm-cov` の導入に時間がかかるため。手元の `npm run test:coverage` で見る)
