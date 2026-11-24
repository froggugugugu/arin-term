# ADR-018: 新しい版を GitHub の Releases に問い合わせて知らせる(設定で ON にしたときだけ)

- 状態: Accepted(本人が 2026-11-23 に要望を出し、確認の既定と更新の方法を選んだ。実機の見た目は本人の確認待ち)
- 日付: 2026-11-23
- 関係: NFR-008(外部通信なし)に例外を 1 つ足す。ADR-003(依存の版)、ADR-013(設定画面)

## 背景(本人の要望と回答。原文)

「あと、起動時バージョンチェックとアップデート機能が欲しい。」

- 確認の既定: 「既定で OFF・設定で ON」
- 更新の方法: 「知らせてページを開く(推奨)」

NFR-008 は「外部通信なし(テレメトリ・更新確認を含め一切なし)」だった。本人の要望により、更新の確認だけを例外にする。

## 決定

### 1. 通信は 1 か所・1 宛先だけ

- 宛先は `https://api.github.com/repos/froggugugugu/arin-term/releases/latest` だけ(`arin_core::update::LATEST_RELEASE_API`)
- macOS 標準の `/usr/bin/curl` を絶対パスで起動する(`crates/arin-platform/src/release/`)。HTTP の crate(reqwest など)と `tauri-plugin-updater` は入れない(境界検査の `http-client-crate` はそのまま)
  - `-q`(利用者の `.curlrc` を読まない)、`--proto =https`、`--tlsv1.2`、curl に 10 秒・全体で 15 秒の上限、応答は 1 MiB まで
  - 送る頭は `Accept` と固定の `User-Agent: Arin-update-check` だけ。利用者・端末の情報は送らない
  - 環境変数は引き継ぐ(`https_proxy` などのプロキシの設定が効く。会社のプロキシの下でも確かめられるように)。応答の大きさは curl の `--max-filesize` に加えて、読む側でも 1 MiB で打ち切る(大きさの分からない応答に `--max-filesize` は効かない)
- 境界検査に `network-process` を足した: Rust のソース(テストを除く)で `curl`・`wget` の起動、NSURLSession、`std::net` を書けるのは `crates/arin-platform/src/release/` だけ
- 画面(WebView)は通信しない。CSP の `connect-src` は変えない

### 2. 確かめる時機

| 時機 | 条件 | 知らせ |
| ---- | ---- | ------ |
| 起動時 | 設定 `ui.checkUpdates` が真 | 新しい版があるときだけ |
| 設定画面で ON にした直後 | — | 新しい版があるときだけ |
| メニューの「Arin」→「アップデートを確認…」 | いつでも(設定が OFF でも。本人の明示の操作なので) | 新しい版・最新です・確かめられませんでした |

- 設定は `state.json` の `ui.checkUpdates`(真のときだけ書く。無い・真偽値でなければ偽)。写しの `checkUpdates`、コマンド `set_check_updates`
- 問い合わせは別のスレッド。重ねて問い合わせない
- 起動時・ON にした直後の確認は、待つあいだに OFF にされたら知らせない。OFF にしたら出している知らせを消す
- いまの版が `X.Y.Z` で読めない(開発版など)ときは比べない(メニューからは「いまの版を読めません」)
- 「新しい版があります」を出しているあいだは、メニューから確かめて失敗しても上書きしない
- 版はタグ `vX.Y.Z` の 3 つの数字で比べる。プレリリース・下書き・形の違うタグは無視する

### 3. 知らせと更新

- 結果は `update-changed`(中身 `UpdateNotice`)で画面へ送り、Rust も持つ(`get_update_notice`。起動時の確認が画面の購読より先に終わっても取りこぼさない)
- 画面は知らせの行(状態の知らせ・設定の知らせと同じ形)に「新しい版 vX.Y.Z があります」と「ダウンロードのページを開く」を出す
- ボタンは `open_release_page`。URL は Rust が持つ版から作る(`…/releases/tag/vX.Y.Z`)。応答の `html_url` と画面からの URL は使わない。`DesktopPort::open_release_page` は `https://github.com/froggugugugu/arin-term/releases` の下だけを開く
- 入れ替えはしない。本人が dmg で入れ替える

## 却下した案

| 案 | 理由 |
| -- | ---- |
| `tauri-plugin-updater` でアプリの中で入れ替える | 本人が「知らせてページを開く」を選んだ。更新用の署名鍵・`latest.json`・reqwest が要る |
| reqwest などの HTTP の crate を足す | 依存が大きく増え、境界検査の方針(HTTP の crate を入れない)を崩す。1 日 1 回程度の問い合わせには curl で足りる |
| NSURLSession を objc2 で呼ぶ | 完了ハンドラのブロックに `block2` が要り、`unsafe` が増える |
| 既定で ON | 本人が既定で OFF を選んだ(NFR-008 の約束を既定では守る) |

## 影響

- NFR-008 は「設定で ON にしたとき・メニューから確かめたときの、GitHub の Releases への問い合わせだけ例外」になる(PRD に注記)
- GitHub の API は認証なしで 1 時間に 60 回まで。起動時に 1 回なので足りる。超えたら「確かめられませんでした」(起動時は黙る)
- リポジトリの名前・持ち主を変えたら `LATEST_RELEASE_API`・`RELEASES_PAGE` を直す
