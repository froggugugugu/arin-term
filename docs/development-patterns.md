# 開発パターン集

プロジェクト固有のコード規約・落とし穴だけをまとめる。構成・スキーマは `docs/architecture.md`、`docs/data-model.md` を参照。

---

## 1. 境界の規則(`npm run lint` が検査)

- `@tauri-apps/*` を import できるのは `src/ipc/` だけ。`@xterm/*` を import できるのは `src/terminal/xterm/` だけ。`src/ui/` は `src/ipc/`・`src/terminal/xterm/` を import しない(操作は `src/state`・`src/terminal` 経由)。`src/terminal/xterm/` は `src/ipc/` を import しない
- `arin-core` は `std::fs`・`std::process`・`std::net` を使わない。依存は serde・serde_json・thiserror・ts-rs だけ。入出力は `arin-platform` に置く
- 環境変数を読むのは `src-tauri/src/launch.rs` だけ。`arin-core` は値を引数で受け取る
- 画面のソースに通信の API(`fetch(` など)と HTML の差し込み(`innerHTML`・`dangerouslySetInnerHTML` など)を書かない
- CSP(`tauri.conf.json` の `csp`・`devCsp`): `connect-src` は IPC と自アプリだけ、スクリプトは `'self'`/`'none'` だけ、スタイルは `'self'`/`'none'`/`'unsafe-inline'` だけ
- xterm.js の要素は React の管理の外に置く(ADR-003)
- 画面から絶対パスを受け取らない・返さない(引数はプロジェクトの ID と相対パス)。ターミナルへ書く文字列は Rust が作る(`insertion_text`)
- ファイルシステムの書き込みには実パスを渡す(途中にシンボリックリンクがあると失敗する)。削除と上書きの経路を作らない(置き換えはゴミ箱へ入れてから)

## 1.1 標準エラーの頭の文字列

| 頭 | 出すところ | 備考 |
| -- | ---------- | ---- |
| `[arin:frontend-error]` | `diagnostics.rs`(`report_frontend_error`) | CSP 違反・例外・読み込み失敗。常に有効。スモークは 1 行でもあれば失敗 |
| `[arin:verify]` | `verify_log.rs` | OSC 52 の回数と流量制御の集計。開発ビルド(`cfg(debug_assertions)`)だけ |
| `[arin:launch]` | `lib.rs`(`apply_window_effects`) | ウィンドウの半透過を付けられなかった(起動は続ける) |
| `[arin:state]` | `persistence.rs` | 状態ファイルを読めずに退避した・退避できなかった・読み取りや保存に失敗した |
| `[arin:fileops]` | `fileops.rs`(`report_staging_leftover`) | コピーの作業用フォルダ `.arin-copy-…` を片付けられずに残した(場所を出す。消す処理は無い) |
| `[arin:watch]` | `tree_watch.rs` | ツリーの監視を張れなかった |

## 2. 状態管理

- zustand は `zustand/vanilla` の生成関数で作り、`main.tsx` から props で渡す。モジュールの読み込み時にストアを作らない
- `AppSnapshot` は画面で書き換えず、届いたもので丸ごと置き換える
- ショートカットのキーは表(`arin-core/shortcuts`、`src/logic/keys/shortcutTable.ts`)から引き、部品やメニューにキーの文字列を直書きしない。見本の表 `defaultShortcuts.ts` は生成物と同じ中身であることを `scripts/default-shortcuts.test.mjs` が確かめる

## 3. 型の生成

- Rust の型(`AppSnapshot` など)を変えたら `npm run gen:types` で `src/ipc/generated/` を再生成する。生成物は手で編集しない。古いままだと `cargo test` が失敗する

## 4. 依存の版

- 版は正確に固定する(npm は `^` なし、Cargo は `=`)。新しく選ぶ版は公開から 2 週間未満のものを避ける(ADR-003)
- OS 連携のクレート(objc2 系・libc・base64)は Tauri が引き込む版と同じにする。Tauri を上げたら `Cargo.lock` に 2 つの版が載っていないか確かめ、ずれたら合わせる(ADR-009)
- `notify` は 8.2.1 に固定。推移的な `notify-types` は `Cargo.lock` で 2.1.0 に止めている(`cargo update` で上がりうる)
- Biome は React のフックの 2 規則だけを見る。わざと依存を外すときは `biome-ignore` に理由を書く。後片付けで `ref.current` を読む誤りは検出されないのでレビューで見る(ADR-008)

## 5. テスト

- 状態フォルダは `ARIN_STATE_DIR`(絶対パス)で差し替える。テストとスモークは本物の `~/Library/Application Support/Arin/` を読み書きしない。`ARIN_SMOKE=1` は差し替え時だけ有効
- Tauri のアプリ(模擬ランタイム)を作る `src-tauri/tests/` は `harness = false`(macOS ではメニューの部品をメインスレッドでしか作れず、標準ハーネスの別スレッドでは panic する)。テストを足すときは `src-tauri/Cargo.toml` に `[[test]]` を足す
- 結合テストで起動直後にターミナルが要るときは `src-tauri/tests/support/` の `launch_with_projects_in`(空の状態フォルダの起動は 0 枚)、`Workspace` を直接作るときは `workspace_with_projects`
- 実 PTY のテスト(`crates/arin-platform/tests/pty.rs` にならう): シェルは `/bin/zsh` 固定、`HOME` を一時フォルダに向ける、出力は目印の文字列で囲ませて取り出す、共通のロックで直列にする、どの待ちにも上限時間を付ける
- ペーストボードのテストは一般のペーストボード(利用者のクリップボード)に触れず、名前付きのペーストボードに書いて終わったら解放する(ADR-009)
- 性能の計測テストは `#[ignore]` にし、`npm run perf` が `--release -- --ignored` で動かす
- `npm run e2e` は実アプリではなく Vite の画面を WebKit で開き、Rust との通信は `e2e/fixtures/tauriMock.ts` が差し替える(出力は `sendChannelBytes`、イベントは `emitEvent` で流す)。製品のソースに差し込み口は置かない。1420 番を使うので `npm run dev` を動かしたままだと失敗する
- `tauriMock.ts` の `withTileDisplay` はタイル表示の Rust の規則(グリッド・ページ・フォーカス)の写し。Rust の規則を変えたら合わせる(合っていないと製品の修正で E2E が落ちる)
- 計測のテスト(`#[ignore]`・`e2e/perf-tiles.spec.ts`)は通常のテストでは走らない。足場(`support/`・`tauriMock.ts`・状態ファイルの置き方)を変えたら `npm run perf` を回して確かめる
- 本物のゴミ箱を使うテストは `#[ignore]`(`cargo test -p arin-platform --test trash -- --ignored`。入れた項目は自分で片付ける)。Finder・既定のアプリには偽の `DesktopPort` を使う
- ファイル操作のテストは一時フォルダを `canonicalize` した実パスで使う(macOS の一時領域は `/var` → `/private/var` のリンクを含む)
- E2E でスクロールや描き直しの直後の状態を測るときは、落ち着いた状態を `expect.poll` で待つ

## 6. 過去の問題事例

| 問題 | 原因 | 対策 |
| ---- | ---- | ---- |
| `node --test scripts/` が失敗 | Node 24 はディレクトリをモジュールとして読もうとする | `node --test "scripts/*.test.mjs"`(`npm test` に含む) |
| `rm -rf` が止められる | `.claude/settings.json` の deny に `Bash(rm -rf *)` がある | `npm ci` は自分で `node_modules` を消すので不要 |
| `Cargo.lock` に `reqwest` が載る | Lock は全 OS の依存を載せる。Tauri は Android・iOS でだけ使う | 境界検査は `tauri → reqwest` の 1 辺だけを理由付きで除外。Tauri を上げたら `cargo tree -i reqwest --target aarch64-apple-darwin` で再確認 |
| 型検査用の一時ファイルから `.d.ts` が `src/` に出てコミットされた | 型検査失敗時にも出力していた | `tsconfig` に `noEmitOnError: true`(src・src/logic・e2e) |
| 素の `cargo build` のバイナリは最低 OS が 11.0 になる | 最低 OS 15.0(`tauri.conf.json` の `minimumSystemVersion`)は `tauri build` が渡す | 配布物は必ず `npm run app` で作る(`otool -l` の `minos` で確認できる) |
| `portable-pty` でログインシェルの `argv[0]` を `-zsh` にできない・環境変数が漏れる・不正な値が黙って置き換わる | `CommandBuilder::new` は `argv[0]` を変えられない。作った時点で親の環境変数を取り込む。`SHELL` が実行できない・cwd がフォルダでないと別の値に切り替える | `new_default_prog()` + `SHELL` で起動し、`env_clear()` してから許可リストの値だけ渡す。シェルと cwd は起動前に確かめてエラーにする(`arin-platform/src/pty/native.rs`) |
| xterm.js が CSP で止められる | 実行時に `<style>` 要素を作り、truecolor の文字に style 属性を書く | `style-src-elem 'self' 'unsafe-inline'` と `style-src-attr 'unsafe-inline'` を足す(`style-src`・`script-src` は変えない) |
| Tauri の既定のメニューが ⌘W などを持つ | メニューを指定しないと macOS で既定のメニューが付く | `menu.rs` で自前のメニュー(「Arin」「ターミナル」「編集」「表示」)に置き換えた。⌘C・⌘V はメニュー経由で画面の `copy`・`paste` イベントになる |
| `footprint` が別のプロセスを測る | PID を位置引数で渡すと名前の部分一致になる | 必ず `-p <pid>` で渡す(`scripts/perf/app-memory.mjs`) |
| フォルダを削除しても「見つからない」にならない | `F_GETPATH` は削除後も消える前のパスを返す(macOS 26、APFS) | 記述子の `fstat` が `ENOENT` かと、返ったパスの装置番号・inode が一致するかで判定する(`arin-platform/src/watch/`) |
| `tauri-plugin-dialog` を足すとファイル操作のプラグインが入る | `tauri-plugin-fs` が optional でない依存 | `NSOpenPanel` を直接呼ぶ(`desktop/folder_panel.rs`)。メインスレッドでしか開けないので `run_on_main_thread` で開いて待つ |
| Finder に渡した日本語の名前が別物になる | `NSURL::fileURLWithPath` は名前を NFD に変える | パスを自前で百分率符号化して URL を作る(`desktop/workspace.rs` の `file_url_from_str`) |
| 検査のあとに途中のフォルダをリンクへ差し替えられるとルートの外に書く | パスの検査と操作の間に隙がある | 親を `O_NOFOLLOW_ANY` で開き fd 相対で操作する(`fs/local.rs`) |
| 大文字・小文字だけの名称変更が衝突にならない | APFS では `RENAME_EXCL` でも成功する(設計書の見込みと違った) | 前提にしない |
| ツリーに `.arin-copy-…` が見える | コピーは宛先のフォルダに一時的な作業用フォルダを作る | 監視はこの名前を隠す。一覧は隠しファイル表示が OFF のとき隠し、ON なら残ったものを出す(`[arin:fileops]` の行と合わせて本人が片付ける) |
| `scripts/smoke.test.mjs` がときどき落ちる | 一時フォルダの数を数える検査が、ほかのテストと同時に動かすと乱れる | 単独で再実行して確かめる |
| 画面の表示が前の状態に戻る(タイルのフォーカス・ページ) | 写しを作る→送るが直列でなく、古い写しがあとから届いた(送り手はコマンド・読み取りスレッド・見回りなど複数) | `SessionRegistry::notify` の中で写しを作って送るまでを直列にした(`sessions.rs`)。写しを送る経路を足すときも `notify` を通す |
| 既定のショートカットが JIS 配列で打てない(⇧⌘\ など) | US 配列の記号のキーは JIS で別の位置・別の組み合わせになる | JIS 配列で打てない記号のキーを既定にしない(M4 は ⇧⌘G に替えた) |
