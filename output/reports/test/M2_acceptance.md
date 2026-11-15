# M2(マルチプロジェクト)受け入れ基準の合否

- 作成日: 2026-11-15
- 対象: ブランチ `feat/m2-projects`
- 完了条件(計画の「M2 の完了条件」): 下の表の AC が自動テストと本人の確認(T36)で全件合格
- 結論: **自動で確かめられる分は合格。本人の確認(T36 の (1)〜(8))が未**

「自動」は自動テストで合格したもの、「本人」は実機の `.app` でしか確かめられないもの(未実施)。

## 受け入れ基準

| FR | 受け入れ基準 | 自動 | 本人(T36) | 根拠 |
| -- | ------------ | ---- | ---------- | ---- |
| FR-001 | ダイアログでフォルダを選ぶとカードが 1 枚増える | 合格(偽のダイアログ) | (3) 未 | `src-tauri/tests/project_commands.rs`、`e2e/projects.spec.ts`、`e2e/m2-scenario.spec.ts` |
| FR-001 | Finder からのドロップでカードが 1 枚増える | 合格(関数の単位) | (3) 未 | `project_commands.rs`。ウィンドウのドロップのイベントからのつなぎは自動テストが無い |
| FR-001・FR-056 | プロジェクトのフォルダに Arin のファイルができない | 合格 | — | `scripts/smoke.mjs`、`src-tauri/tests/app_persistence.rs` |
| FR-002 | B をクリックすると B のタブ群が出て、A のプロセスが終わらない | 合格 | (5) 未 | `e2e/tabs.spec.ts`、`e2e/m2-scenario.spec.ts`、`src-tauri/tests/session_pty.rs` |
| FR-003 | 削除でカードが消え、フォルダと中身が残る。動作中があると確認が出て、中止で何も変わらない | 合格 | — | `project_commands.rs`、`e2e/projects.spec.ts`、`e2e/m2-scenario.spec.ts` |
| FR-004 | A・B・C を登録し B を選んで再起動すると、同じ順で B が選択されている(AC-P04) | 部品ごとに合格 | (7) 未 | `crates/arin-core/tests/workspace_projects.rs`、`persistence_codec.rs`、`e2e/m2-scenario.spec.ts`。3 件と選択を起動し直しまで通す結合テストは無い |
| FR-007 | 改名すると同じカードのままパスが変わる。削除すると「見つからない」で残り、再リンクと削除を選べる | 合格 | (4) 未 | `src-tauri/tests/root_follow.rs`、`crates/arin-core/tests/workspace_root_follow.rs`、`e2e/projects.spec.ts` |
| FR-008 | 1 プロジェクト 1 カードで選択中が強調される。カードが多いと上段だけがスクロールする | 合格 | (8) 未 | `e2e/projects.spec.ts` |
| FR-009 | ペインの配分が再起動後も同じ。最小を下回らない。左ペインを折りたためる(AC-P09) | 合格 | (7) 未 | `root_follow.rs`、`e2e/projects.spec.ts`、`e2e/m2-scenario.spec.ts` |
| FR-010 | 同じフォルダは増えず既存が選択される。入れ子のフォルダは別のカード(AC-P10) | 合格 | — | `crates/arin-core` のプロジェクトの規則のテスト、`project_commands.rs` |
| FR-039(M2 の分) | 複数タブ、0 枚、新規タブの cwd、起動時に全プロジェクトで 1 タブ | 合格 | (7) 未 | `crates/arin-core/tests/terminal_store.rs`、`src-tauri/tests/sessions.rs`、`e2e/tabs.spec.ts`、`npm run smoke` |
| FR-042 | A でループを流し、B に切り替えて戻ると途切れず進んでいる(AC-M04) | 合格(時間は短縮) | (5) 未 | `session_pty.rs`(実 PTY、1〜30 が欠けない)、`e2e/m2-scenario.spec.ts`(欠番なし、接続 1 回、取り込み量が一致) |
| FR-044 | ⌘T・⌘W・⌘⇧[・⌘⇧]。最後の 1 枚を閉じると 0 枚で、自動で開かない | 合格(メニューの項目の単位) | (1) 未 | `src-tauri/tests/app_menu.rs`、`e2e/tabs.spec.ts`。実際のキーは自動テストに無い |
| FR-059 | 8 色で 3 件が違う色。色をあとから変えられる(AC-C01) | 合格 | — | `crates/arin-core` の色のテスト、`project_commands.rs`、`e2e/projects.spec.ts` |
| FR-060 | カードのタイトル部・ツリーのヘッダ・タブが同じ色相(タブ表示) | 合格(計算済みのスタイル) | (8) 未 | `e2e/projects.spec.ts`、`e2e/tabs.spec.ts`(ライト・ダーク) |
| FR-061(M2 の分) | タブに文字の名前が出る | 合格 | — | `e2e/tabs.spec.ts` |
| FR-063 | 動作中があると終了時に確認が出る | 合格(⌘Q の項目と閉じるボタン) | (6) 未 | `src-tauri/tests/app_quit.rs`、`e2e/tabs.spec.ts`。**Dock の「終了」・ログアウトでは確認が出ない見込み**(下の「判断が要る点」) |
| FR-064 | カードの右クリックで絶対パスをコピーでき、選択が変わらない | 合格 | — | `project_commands.rs`(名前付きのペーストボード)、`e2e/projects.spec.ts` |
| NFR-006 | 1 枚の終了が他のプロジェクトを巻き込まない | 合格 | — | `session_pty.rs`(タブを閉じる、外から kill、前面のコマンドごと閉じる) |

## 検証コマンドの結果(2026-11-15。すべて終了コード 0)

| コマンド | 内容 |
| -------- | ---- |
| `npm run lint` | 型検査、Biome、clippy、`cargo fmt --check`、境界検査(違反なし) |
| `npm test` | Vitest 258 件、`cargo test` 失敗 0、`node --test` 73 件 |
| `npm run build` | 型検査、画面のビルド、Rust のビルド |
| `npm run test:coverage` | `src/logic` 行 100%、`arin-core` 行 99.14% |
| `npm run e2e` | Playwright(WebKit)96 件。3 回続けて合格 |
| `npm run app` | 配布用の `.app` |
| `npm run smoke` | 通常と、壊れた `state.json` の 2 回とも成功 |
| `node scripts/smoke.mjs --env=ARIN_VERIFY_GRID=1` | 成功。9 個のシェルがアプリの終了後に残らない |
| `npm run perf` | 記録だけ(下) |

スモークと計測は、本物の `~/Library/Application Support/Arin/` を変えていない(スクリプトの検査が通っている)。

## 計測(5 プロジェクト・各 1 タブ。Apple M5、参考値。合否にしない)

| 項目 | 値 | 目安 |
| ---- | -- | ---- |
| アプリ本体のメモリ | 27.4 MiB | NFR-001: 200MB 以下 |
| 本体 + WebKit | 136.9 MiB | — |
| 起動からターミナルの表示準備まで | 358ms | NFR-002: 1 秒以内 |
| アイドル時の CPU(NFR-003) | 未計測 | スクリプトでは測らない |

全体は `testreport/perf/latest.md`。

## 完了前のレビューで見つけて直したもの

- 前面でコマンドが動いているタブを閉じると、閉じる処理が戻らなくなる(T35b の実 PTY のテストで発見。`crates/arin-platform/src/pty/native.rs`)
- シェルが終わるたびに、遅れて届く処理済みの通知が失敗として診断に出る(`src-tauri/src/sessions.rs` の `ack`)
- キーボードでタブを閉じたあと・最後の 1 枚を閉じたあとに、フォーカスが失われる(`src/ui/terminals/`)

## 判断が要る点・残っている問題

| 項目 | 状態 |
| ---- | ---- |
| FR-063: Dock の「終了」・ログアウト・再起動では確認が出ない見込み(ソースからの推定。tao が `applicationShouldTerminate` を実装していない) | 計画の範囲は ⌘Q と閉じるボタン。範囲を広げるかは本人の判断 |
| 書き込みが止まったままのタブを閉じると、書き込みが戻るまでシェルと前面のコマンドが残る(入力を読まないコマンドに大量に貼り付けた場合。条件は未確認) | 計画(T33b)の決めたとおりの動き。PTY のロックの外から終わらせる口を足せば直せる。未対応 |
| 状態の写しが、作った順と違う順で画面に届くことがあり得る(写しに版の番号が無い。頻度は低い) | 未対応 |
| 回収の上限(1 秒)を過ぎたシェルは、アプリの終了までゾンビで残る | 未対応 |
| シェルが終わってもバックグラウンドのジョブが端末を開いたままだと、読み取りスレッドが残る | 未対応 |
| 見つからないプロジェクトは、同じパスにフォルダを作り直すと、起動中は自動でつながる(起動時はつながない) | 仕様の確認が要る(FR-007) |
| 応答しないネットワークのボリュームで、終了時に見回りのスレッドを待って止まる恐れ(推測) | 未対応 |
| 使われなくなった `terminal::TerminalSnapshot` と `LaunchOptions::terminal_count()` | 片付けていない |

## 誰も確かめていないこと(本人の確認の範囲)

計画の T36 の (1)〜(8): 実際のキー(⌘T・⌘W・⌘⇧[・⌘⇧]。US と JIS)、⌘C・⌘V・⌘A、フォルダ選択ダイアログと Finder からのドロップ、Finder での改名と削除への追従、Claude Code を動かしたままの切替、⌘Q と閉じるボタンの確認、再起動後の復元、体感。
