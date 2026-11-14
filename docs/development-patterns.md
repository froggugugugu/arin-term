# 開発パターン集

プロジェクト固有のコード規約・落とし穴だけをまとめる。構成・スキーマは `docs/architecture.md`、`docs/data-model.md` を参照。

---

## 1. 境界の規則(`npm run lint` が検査)

- `@tauri-apps/*` を import できるのは `src/ipc/` だけ。`src/ui/` は `src/ipc/` を import しない(操作は `src/state` 経由)
- `arin-core` は `std::fs`・`std::process`・`std::net` を使わない。依存は serde・serde_json・thiserror・ts-rs だけ。入出力は `arin-platform` に置く
- 環境変数を読むのは `src-tauri/src/launch.rs` だけ。`arin-core` は値を引数で受け取る
- xterm.js の要素は React の管理の外に置く(ADR-003)。M1 で実装するときに守る

## 2. 状態管理

- zustand は `zustand/vanilla` の生成関数で作り、`main.tsx` から props で渡す。モジュールの読み込み時にストアを作らない
- `AppSnapshot` は画面で書き換えず、届いたもので丸ごと置き換える

## 3. 型の生成

- Rust の型(`AppSnapshot` など)を変えたら `npm run gen:types` で `src/ipc/generated/` を再生成する。生成物は手で編集しない。古いままだと `cargo test` が失敗する

## 4. 依存の版

- 版は正確に固定する(npm は `^` なし、Cargo は `=`)。新しく選ぶ版は公開から 2 週間未満のものを避ける(ADR-003)

## 5. テスト

- 状態フォルダは `ARIN_STATE_DIR`(絶対パス)で差し替える。テストとスモークは本物の `~/Library/Application Support/Arin/` を読み書きしない。`ARIN_SMOKE=1` は差し替え時だけ有効
- `src-tauri/tests/` は `harness = false`(macOS ではメニューの部品をメインスレッドでしか作れず、標準ハーネスの別スレッドでは panic する)。テストを足すときは `src-tauri/Cargo.toml` に `[[test]]` を足す
- `npm run e2e` は実アプリではなく Vite の画面を WebKit で開き、Rust との通信は `e2e/fixtures/tauriMock.ts` が差し替える。製品のソースに差し込み口は置かない。1420 番を使うので `npm run dev` を動かしたままだと失敗する

## 6. 過去の問題事例

| 問題 | 原因 | 対策 |
| ---- | ---- | ---- |
| `node --test scripts/` が失敗 | Node 24 はディレクトリをモジュールとして読もうとする | `node --test "scripts/*.test.mjs"`(`npm test` に含む) |
| `rm -rf` が止められる | `.claude/settings.json` の deny に `Bash(rm -rf *)` がある | `npm ci` は自分で `node_modules` を消すので不要 |
| `Cargo.lock` に `reqwest` が載る | Lock は全 OS の依存を載せる。Tauri は Android・iOS でだけ使う | 境界検査は `tauri → reqwest` の 1 辺だけを理由付きで除外。Tauri を上げたら `cargo tree -i reqwest --target aarch64-apple-darwin` で再確認 |
| 型検査用の一時ファイルから `.d.ts` が `src/` に出てコミットされた | 型検査失敗時にも出力していた | `tsconfig` に `noEmitOnError: true`(src・src/logic・e2e) |
| 素の `cargo build` のバイナリは最低 OS が 11.0 になる | 最低 OS 15.0(`tauri.conf.json` の `minimumSystemVersion`)は `tauri build` が渡す | 配布物は必ず `npm run app` で作る(`otool -l` の `minos` で確認できる) |
