# Architecture Decision Records

| # | タイトル | ステータス | 日付 |
|---|---------|-----------|------|
| ADR-001 | [ターミナルエンジンに xterm.js を使い、描画は DOM・全角幅は unicode11 にする(M1 の Go/No-Go と M4 の決定)](ADR_001_terminal-engine.md) | Accepted(M4 の決定で 2026-11-16 に更新) | 2026-11-15 |
| ADR-002 | [ロジック層の置き場所と依存方向の強制方法](ADR_002_logic-placement.md) | Accepted | 2026-11-14 |
| ADR-003 | [画面のフレームワークに React と zustand を使う](ADR_003_ui-framework.md) | Accepted | 2026-11-14 |
| ADR-004 | [PTY のクレートと出力の流し方](ADR_004_pty.md) | Accepted | 2026-11-14 |
| ADR-005 | [設定ファイルの形式](ADR_005_settings-format.md) | Accepted | 2026-11-14 |
| ADR-006 | [テストの構成と画面の自動テストの方式](ADR_006_test-structure.md) | Accepted | 2026-11-14 |
| ADR-007 | [M3 のゴミ箱の方式、アプリ内のドラッグ、ツリーの仮想スクロール、M3 で足す依存](ADR_007_finder-integration.md) | Proposed(ゴミ箱は【検証中】) | 2026-11-15 |
| ADR-008 | [React のフックの検査に Biome を使う](ADR_008_hooks-lint.md) | Accepted | 2026-11-15 |
| ADR-009 | [M2 で arin-platform に足す依存](ADR_009_m2-platform-deps.md) | Accepted | 2026-11-15 |
| ADR-010 | [ウィンドウの地を macOS のすりガラス(vibrancy)にする](ADR_010_window-translucency.md) | Accepted(実機の見た目は確認待ち) | 2026-11-15 |
| ADR-011 | [タイル表示の状態は Rust が持ち、画面は M2 のターミナルの置き場所の仕組みで複数枚を出す](ADR_011_tile-view.md) | Proposed(本人の了承は T50a) | 2026-11-15 |
| ADR-012 | [ショートカットのキーを `settings.toml` の `[shortcuts]` で変えられるようにする(M4.5)](ADR_012_keybindings.md) | Proposed(【要確認】4 点。本人の了承は T66c) | 2026-11-16 |
| ADR-013 | [ターミナルと画面の文字の大きさを、⌘, で開く設定画面で変え、`state.json` に保存する(M4.6)](ADR_013_font-size-settings.md) | Proposed(【要確認】5 点。本人の了承は T73a) | 2026-11-16 |
| ADR-014 | [透過の設定(ウィンドウとターミナルの背景)、タイルの番号キー(⌘1〜9)、Shift+Enter の改行(M4.7)](ADR_014_translucency-tile-keys-newline.md) | Proposed(本人の確認待ち) | 2026-11-17 |
| ADR-015 | [Markdown のファイルを、ターミナルのタブと並ぶエディタのタブで編集・プレビューする(M4.8)](ADR_015_markdown-editor.md) | Accepted(【要確認】3 点は本人が 2026-11-17 に了承。実機の確認は T86a) | 2026-11-17 |

候補の出どころは `ARCH_multi-terminal.md` §14。
