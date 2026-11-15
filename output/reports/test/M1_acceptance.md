# M1(ターミナル 1 枚)受け入れ基準の合否

- 作成日: 2026-11-15
- 対象: ブランチ `feat/m1-terminal`
- 完了条件(PRD §9): Claude Code と Codex が実用に耐えるかを人間が判定する(Go/No-Go)
- 結論: **Go**(本人。2026-11-15)。決定の中身は `output/design/ADR_001_terminal-engine.md`

本人が確かめたのは、配布版を既定の構成(DOM 描画、`unicode11`)で起動して Claude Code を動かすところまで。手順書(`output/tasks/M1_GO-NO-GO_CHECKLIST.md`)の項目ごとの記入は無いので、下の表では「本人が見たこと」「自動で確かめたこと」「誰も確かめていないこと」を分けた。

## 受け入れ基準

| FR | 受け入れ基準 | 状態 | 根拠 |
| -- | ------------ | ---- | ---- |
| FR-039 | 各プロジェクトが複数のタブを持てる。0 枚も許す | **M2 で作る** | M1 は 1 枚だけ |
| FR-039 | 新規タブの初期 cwd がプロジェクトルート | **M2 で作る** | M1 の cwd はホーム固定【仮定】。cwd が指定のフォルダになること自体は、実 PTY のテストで合格 |
| FR-039 | 起動時に全プロジェクトで 1 タブずつ開く | **M2 で作る** | プロジェクトの管理が未実装 |
| FR-040 | `which claude` が Terminal.app と同じパスを返す | 本人が Claude Code を起動できた。パスの突き合わせの記入は無い | 自動テスト: ログインシェルとして起動する(`argv[0]` が `-zsh`)、`PATH` が同じ最小の環境の `zsh -l` と一致する(`crates/arin-platform/tests/pty.rs`) |
| FR-041 | truecolor、代替スクリーン、マウス、ブラケットペースト、日本語入力、全角幅、絵文字が、Claude Code と Codex で実用上崩れない | **本人が Go と判定**(Claude Code。既定の構成) | 本人の発言「良さそうだ」。Codex と、項目ごとの結果は記入が無い。全角幅と色は Playwright の WebKit で合格(実機の WKWebView とは別のビルド) |
| FR-043 | 選択範囲をコピーでき、ペーストできる | 自動テストは合格。**実機の ⌘C・⌘V は誰も確かめていない** | Vitest と Playwright(`e2e/terminal.spec.ts`)、メニューのテスト 3 件 |
| FR-043 | 既定で 10,000 行までさかのぼれる | 自動テストは合格 | Playwright(`e2e/terminal.spec.ts`) |
| FR-043 | ウィンドウの大きさに行・桁数が追従する | 自動テストは合格 | Playwright、実 PTY のテスト(寸法の変更がシェルに届く) |

## 検証コマンドの結果

M1 の締め(T27)で文書だけを変えたあとに再実行した結果は、`output/tasks/PROGRESS.md` のセッションログに書く。コードの最終検証(レビュー対応のコミット `afd280f` のあと)は次のとおりで、すべて終了コード 0。

| コマンド | 内容 |
| -------- | ---- |
| `npm test` | Vitest 151 件、`cargo test`、`node --test` |
| `npm run lint` | 型検査、clippy、`cargo fmt --check`、境界検査 11 項目 |
| `npm run build` | 型検査、画面のビルド、Rust のビルド |
| `npm run e2e` | Playwright(WebKit)28 件 |
| `npm run test:coverage` | `src/logic` と `arin-core` が行 100% |
| `npm run app`、`npm run app:debug` | 配布版とデバッグ版の `.app` |
| `npm run smoke` | 既定、WebGL、9 枚 + WebGL、デバッグ版のそれぞれで成功 |

## 計測(T24。Apple M5、参考値)

| 項目 | 値 |
| ---- | -- |
| 10 万行(7.7 MB)の `cat`、PTY から送り先まで | 約 41ms |
| 起動からターミナルの表示準備まで | 約 312ms |
| メモリ(アプリ本体 + WebKit、1 枚) | 79.7 MiB |
| 未処理の量の最大 | 525,184 B(上限 512 KiB の少し上で頭打ち) |

## 誰も確かめていないこと

- WebGL 描画の見た目、9 枚の見た目、非表示のインスタンスの見た目
- 最小化して 5 分以上置いたあとの挙動
- 実機の ⌘C・⌘V と、貼り付けの許可の吹き出しが出ないこと
- Option・Ctrl のキー、OSC 52 を使う場面
- Codex
- `npm run dev` の動作(`devCsp` で止まる可能性がある)

問題が見つかったら、その時点で不具合として扱う。
