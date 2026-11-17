# M3(ツリーとファイル操作)受け入れ基準の合否

- 作成日: 2026-11-15
- 対象: ブランチ `feat/m3-tree`(`feat/m2-projects` から分岐。push していない)
- 完了条件(計画の「M3 の完了条件」): FR-013〜018(Quick Look を除く)、024〜030、035〜037 の AC 合格と、VSCode を閉じて 1 週間のドッグフーディング
- 結論: **自動で確かめられる分は合格。本人の確認(T45 の (1)〜(9))と 1 週間のドッグフーディングが未**

「自動」は自動テストで合格したもの、「本人」は実機の `.app` でしか確かめられないもの(未実施)。

## 受け入れ基準

| FR | 受け入れ基準(要点) | 自動 | 本人(T45) | 根拠 |
| -- | ------------------ | ---- | ---------- | ---- |
| FR-013 | 選択中のプロジェクトのルートからツリーが出て、フォルダを開閉できる | 合格 | 未 | `e2e/tree.spec.ts`、`src-tauri/tests/tree_commands.rs` |
| FR-014 | 外からの変更が 2 秒以内にツリーに出る(AC-T02) | 合格(一時フォルダで約 0.2 秒) | 未 | `crates/arin-platform/tests/watch.rs`、`src-tauri/tests/tree_watch.rs`、`e2e/tree.spec.ts` |
| FR-015 | 相対パス・絶対パスをコピーできる | 合格(名前付きのペーストボード) | 未 | `src-tauri/tests/desktop_commands.rs`、`crates/arin-core/tests/path_format.rs` |
| FR-016 | ツリーからターミナルへドラッグするとパスが挿入され、空白がエスケープされ、Enter は送られない(AC-T04) | 合格 | 未(手触り) | `desktop_commands.rs`(偽 PTY に書かれたバイト列)、`crates/arin-platform/tests/shell_roundtrip.rs`(実際の zsh で 63 個の名前)、`e2e/tree-dnd.spec.ts` |
| FR-017 | `.git`・`node_modules`・`.DS_Store` を既定で隠し、切り替えられる | 合格 | — | `crates/arin-core/tests/tree_rules.rs`、`tree_commands.rs`、`e2e/tree.spec.ts` |
| FR-018 | Finder で表示、既定のアプリで開く(Quick Look は M6) | 合格(偽の Finder) | 未 | `desktop_commands.rs`、`e2e/tree-ops.spec.ts` |
| FR-024 | 右クリックのメニューに PRD の項目が出て、ショートカットで同じ操作ができる | 合格(合成のキー) | 未(実際のキー) | `e2e/tree-ops.spec.ts`、`src/logic/keys/*.test.ts` |
| FR-025 | 新規ファイル・新規フォルダ(インライン編集) | 合格 | 未 | `e2e/tree-ops.spec.ts`、`src-tauri/tests/fileops_commands.rs` |
| FR-026 | 名称変更(拡張子を除いて選択、Enter で確定、Esc で取り消し) | 合格 | 未(日本語入力) | 同上 |
| FR-027 | コピー・切り取り・ペースト(Finder との間も) | 合格(名前付きのペーストボード) | 未(Finder との間) | `fileops_commands.rs`、`crates/arin-core/tests/fileops_service.rs`、`crates/arin-platform/tests/desktop.rs` |
| FR-028 | 削除はゴミ箱に入れる | 合格(偽のゴミ箱。本物は `#[ignore]` で 1 回実行) | 未(「戻す」が出るか) | `fileops_commands.rs`、`crates/arin-platform/tests/trash.rs` |
| FR-029 | ツリー内のドラッグで移動(確認あり)、⌥ でコピー | 合格 | 未(手触り) | `e2e/tree-dnd.spec.ts`。「設定で確認を省略」は設定ファイルが要るので M6(T56)へ先送り |
| FR-030 | 同名があれば上書きせず、置き換える・両方残す・中止を選べる | 合格 | — | `fileops_service.rs`、`crates/arin-platform/tests/fs_race.rs`(300 回の競合)、`e2e/tree-ops.spec.ts` |
| FR-035 | ルートの外・ルート自体への操作を拒否する | 合格 | — | `crates/arin-core/tests/fileops_scope.rs`、`fileops_commands.rs`、`tree_commands.rs`、`desktop_commands.rs` |
| FR-036 | メニューを開いたあとに消された対象は「見つかりません」(AC-T24) | 合格 | — | `fs_race.rs`、`e2e/tree-ops.spec.ts`、`e2e/m3-scenario.spec.ts` |
| FR-037 | ドラッグ中に 100 ファイルが増えても行が動かず、ドロップ後に反映(AC-T25) | 合格 | — | `e2e/tree-dnd.spec.ts`、`e2e/m3-scenario.spec.ts`、`src/logic/tree/updateGate.test.ts` |
| FR-060 | カード・ツリーのヘッダ・タブが同じ色相 | 合格(色の印の計算済みのスタイル) | 未 | `e2e/projects.spec.ts`、`e2e/tabs.spec.ts` |

仕様照合(第三者の照合を 1 回)は、MUST・SHOULD の gap なし。

## 検証コマンドの結果(2026-11-15。最後のコミット `fe4026d` のあと)

| コマンド | 結果 |
| -------- | ---- |
| `npm run lint` | 終了コード 0(型検査、Biome、clippy、`cargo fmt --check`、境界検査は違反なし) |
| `npm test` | 終了コード 0(Vitest 465 件、`cargo test` 800 件・失敗 0、`node --test` 79 件) |
| `npm run build` | 終了コード 0 |
| `npm run test:coverage` | 終了コード 0(`src/logic` 行 99.72%、`arin-core` 行 97.74%) |
| `npm run e2e` | 終了コード 0(168 件)。3 回続けて合格 |
| `npm run app` | 終了コード 0 |
| `npm run smoke` | 終了コード 0(通常と、壊れた `state.json` の 2 回とも成功) |
| `node scripts/smoke.mjs --env=ARIN_VERIFY_GRID=1` | 終了コード 0 |
| `npm run perf` | 終了コード 0(記録だけ) |

最初の通しでは `npm run perf` の途中でシステムのメモリが足りなくなり、Claude Code がコマンドを止めた(コマンドの失敗ではない)。残り(検証画面のスモーク、E2E の 2〜3 回目、計測)は、ほかの処理を止めて 1 つずつ取り直した。

## 計測(参考値。合否にしない)

| 項目 | 値 | 目安 |
| ---- | -- | ---- |
| 1 フォルダに 10 万ファイルの読み込み + 除外 + 並べ替え(Rust) | 147.6ms(うち読み込み 39.7ms。残りはほぼ並べ替え) | NFR-005: 100ms。超えている |
| 1,000 項目のフォルダ | 0.89ms | — |
| 10 万件のフォルダを開いて行が出るまで(画面) | 中央値 21ms | — |
| 外からの変更の検知 | 約 0.2 秒 | 2 秒以内 |

アプリ(5 プロジェクト・各 1 タブ): 本体 27.9 MiB(NFR-001 の目安 200MB 以下)、本体 + WebKit 138.3 MiB、起動からターミナルの表示準備まで 379ms(NFR-002 の目安 1 秒以内)。全体は `testreport/perf/latest.md`。

## 完了前のレビューで見つけて直したもの

- 検査のあとに途中のフォルダをリンクへ差し替えられると、ルートの外に書く(T44a の実ファイルのテストで発見。書き込みを fd 相対にした)
- 複数項目のペーストの衝突で、確認に出ていない項目までゴミ箱に入る(数を伝えて文面に出す)
- 置けないと分かる移動の前に既存をゴミ箱に入れる/コピーの中の `EEXIST` を衝突と読む
- 途中で衝突した複数項目のペーストの呼び直しが失敗する・重複を作る
- プロジェクトが変わったときに、ツリーの表示だけの状態(メニュー・入力・確認・フォーカスの印)が残り、更新が止まる
- 残ったコピーの作業用フォルダがどこにも出ない/長いコピーのあいだロックを持ち続ける
- 「開く」が実行につながる種類を渡しうる/リンクのコピーがリンクの先として書かれる
- `NSURL::fileURLWithPath` が日本語の名前を NFD に変える
- ウィンドウの半透過の設定で、模擬ランタイムの結合テストが落ちる

## 判断が要る点・残っている問題

| 項目 | 状態 |
| ---- | ---- |
| ゴミ箱の方式(ADR-007 の (a)) | `trashItemAtURL` で実装。Finder で「戻す」が出るかの目視が未。出なければ差し替える |
| 残る競合の隙間 | `copyfile` とゴミ箱がパスで扱う間の差し替え、コピーの元の読み出し、押さえたフォルダそのものを外へ `mv` された場合。どれも既存のファイルは上書きしない |
| 「開く」の範囲 | ふつうのファイルで、実行の権限が無く、実行につながる拡張子でないものだけ。`chmod +x` したスクリプトはエディタでも開けない。緩めるかは本人の判断 |
| NFR-005(10 万ファイル) | 並べ替えが主因で目安を超える。比べる前に小文字の鍵を作れば縮む見込み。未対応 |
| 切り替えたあとに届いた前のプロジェクトの応答で、衝突の確認が開きうる | 未対応 |
| ルートの読み込みが失敗したとき、知らせは出るが「読み込み中…」が残る | 未対応 |
| タブの「+」の読み上げ用のキーの表示(`Meta+T`)が直書き | M4.5(ショートカットの設定)で表から引く形に直す |
| M2 から残っている項目 | `output/reports/test/M2_acceptance.md` の「判断が要る点・残っている問題」のまま |

## 誰も確かめていないこと(本人の確認の範囲)

- すりガラス(半透過)の実際の見え方、ライトとダークでの読みやすさ(ADR-010)
- メニューバーに無い ⌘ の組み合わせ(⌘⌫・⌘N・⇧⌘N・⌘O・⌥⌘R・⌘↩・⇧⌥⌘C・⌥⌘C)が実際の WKWebView で画面に届くか。US と JIS の配列
- ツリーにフォーカスがあるとき、「編集」メニューのコピー・切り取り・ペーストが有効になり、⌘C・⌘X・⌘V がツリーの操作になるか
- ツリーでの ⌘C のあと、クリップボードにファイルが残るか。Finder との間でコピー・ペーストできるか
- 実際の日本語入力で、変換の確定の Enter が名前の確定にならないか
- ドラッグの手触り(しきい値、掴んだ項目の写し、ウィンドウの外で離したとき)
- Finder で表示・既定のアプリで開く、本物のゴミ箱と「戻す」
- 自分の本物のプロジェクトのフォルダでの、外からの変更の反映
