# 公開前の点検: ライセンスと、公開されるファイルの中身

- 作成日: 2026-11-17
- 対象: ブランチ `feat/m4.8-markdown-editor`(`c06b42d`)
- 目的: リポジトリを公開して OSS にする前に、依存のライセンスと、公開されて困る文字列の有無を確かめる
- 結論: **MIT で公開して差し支える依存は見つからなかった。公開の前に足すものが 3 つある(下の「やること」)**

名前「Arin」の調査は別の節(末尾)。

## 1. 依存のライセンス

調べ方: Rust は `cargo metadata --filter-platform aarch64-apple-darwin --locked` から、ワークスペースの通常の依存とビルド時の依存をたどった(テスト用の依存は除く)。npm は `npm ls --omit=dev --all` の各 `package.json`。どちらも各パッケージが申告しているライセンスの欄を読んだもので、ライセンスの本文は 1 つずつ開いていない。

### Rust(251 個)

| ライセンス | 個数 |
| ---------- | ---- |
| MIT と Apache-2.0 の選択(書き方の違いを含む) | 148 |
| MIT | 45 |
| Unicode-3.0(ICU4X の系統) | 18 |
| Zlib・Apache-2.0・MIT の選択(objc2 の系統など) | 14 |
| Unlicense と MIT の選択 | 8 |
| MPL-2.0 | 5 |
| BSD-3-Clause(MIT との併記を含む) | 4 |
| Zlib | 2 |
| CC0-1.0(選択つきを含む) | 2 |
| そのほか(0BSD・BSD-2-Clause・Apache-2.0 単独・Apache-2.0 と MIT の併記・Unicode-3.0 との併記) | 5 |

### npm(配布物に入る 36 個)

| ライセンス | 個数 |
| ---------- | ---- |
| MIT | 35 |
| Apache-2.0 と MIT の選択 | 1 |

### 評価

- **GPL・AGPL・LGPL・SSPL・商用の制限つきのものは無い。**
- **MPL-2.0 が 5 個ある**(`cssparser`・`cssparser-macros`・`selectors`・`dtoa-short`・`option-ext`。Tauri が引き込む)。ファイル単位のコピーレフトで、手を加えずに使うぶんには自分のコードのライセンスに影響しない。配布物に「MPL-2.0 の部品を含むこと」と「そのソースの入手先」を表示する必要がある。
- Apache-2.0 を選んで使う部品は、NOTICE があればその表示が要る。MIT・BSD・Zlib・Unicode-3.0 は著作権表示とライセンスの本文の同梱が要る。
- 開発用の依存(Vitest・Playwright・Biome など)は配布物に入らない。

## 2. 同梱している素材

- 画像は `src-tauri/icons/` の 5 つだけ(自作の仮のアイコン)。フォント・音・外部の画像は同梱していない。
- 画面のフォントはシステムのもの(`system-ui`・`ui-monospace`・`SF Mono`・`Menlo`)を名前で指すだけ。
- ソースに、ほかのプロジェクトから持ってきたことを示す表示(著作権表示・SPDX・「〜から移植」)は無い。

## 3. 公開されるファイルの中身(`input/`・`output/`・ハーネスを含める前提)

| 調べたもの | 結果 |
| ---------- | ---- |
| 本人のホームのパス(利用者名を含む絶対パス) | 追跡中のファイルに 0 件。`/Users/someone` などは見本の名前 |
| メールアドレス | 追跡中のファイルに 0 件 |
| Claude のセッションの URL | 追跡中のファイルに 0 件 |
| 秘密鍵・トークンの形の文字列 | 実物は 0 件。当たったのは検出用の正規表現(`.agents/hooks/`)と、テストの見本(`settings_load.rs` の「ひみつの文字列-ghp_0123456789」) |
| Apple の Team ID・証明書の名前 | 0 件(署名はまだ組み込んでいない) |

**履歴(コミット)を公開する場合だけ出るもの**:

- コミットの作者のメールアドレス(180 件。GitHub のアカウントのアドレス)
- コミットの本文の Claude のセッションの URL(182 件。本人しか開けないが、URL の文字列は見える)

追跡していない `output/reports/sessions/` と、`.gitignore` で外している `testreport/` は公開されない。

## 4. ほかのプロジェクトとの関係

- 開発のハーネス(`.agents/`・`.claude/`・`.codex/`・`CLAUDE.md`・`AGENTS.md`・`constitution.md`)は、本人の `froggugugugu/project-blueprints`(公開、MIT)から来ている。
- `input/`・`output/` は cmux・tmux・VS Code・iTerm2 に比較と動機の説明として触れている(cmux は 5 行)。機能や見た目を写した記録は無い。
- mdslide・tadcap は本人の過去作(依存の版と作り方を合わせた)。

## 5. やること(公開の前)

| 番号 | やること | 理由 |
| ---- | -------- | ---- |
| 1 | `LICENSE` を置き、`package.json` と `Cargo.toml` に `license` を書く | いまはどちらも無い(ライセンスが無い公開は「利用を許していない」扱いになる) |
| 2 | 依存のライセンスの一覧(`THIRD_PARTY_LICENSES.md`。本文つき)と、要点の表示(`THIRD_PARTY_NOTICES.md`。MPL-2.0 の部品とソースの入手先を含む)を作り、配布物かリポジトリに置く | 上の評価のとおり。tadcap と同じ形 |
| 3 | 履歴を公開するなら、作者のメールアドレスとセッションの URL が見えることを承知するか、履歴を 1 コミットにまとめる | 3 の「履歴を公開する場合だけ出るもの」 |

## 6. 確かめていないこと

- 各依存のライセンスの本文と NOTICE の有無(申告の欄だけを読んだ)
- 依存の既知の脆弱性(`npm audit`・`cargo audit`。この点検の範囲の外)
- 輸出管理(暗号の扱い)。このアプリは自前で暗号や通信を持たない

## 7. 名前「Arin」の調査(2026-11-17。Web の検索による)

結論: **ターミナル・エディタの分野に同じ名前の製品は見つからなかった。ただし「ARIN」(American Registry for Internet Numbers。IP アドレスの登録機関)がネットワークの界隈でよく知られ、米国で商標を登録しているという二次情報がある。条件つきで使える見込み。**

### 見つかったもの(分野が近い順)

| 名前 | 何のものか | 分野の近さ | 出どころ |
| ---- | ---------- | ---------- | -------- |
| ARIN(American Registry for Internet Numbers) | IP アドレスの登録機関。`arincli` など API の道具が GitHub にある | 開発者・ネットワークの界隈で知名度が高い。製品の種類は違う | <https://www.arin.net/>、<https://github.com/arineng/arincli> |
| crates.io の `arin` | AI エージェント向けの注釈の層の Rust API(2026-07 作成、ダウンロード計 159) | 開発者向けでやや近い。小規模 | <https://crates.io/crates/arin> |
| npm の `arin` | Sass の枠組み(2018 年から更新なし) | 遠い | <https://www.npmjs.com/package/arin> |
| Arin Corporation | AI のマーケティング SaaS の会社 | 遠い | <https://www.crunchbase.com/organization/arin-corporation> |

### 確かめた事実

- Homebrew に `arin` という formula・cask は無い
- App Store に「Arin」ちょうどの名前のアプリは見つからなかった
- 「Arin terminal」「arin-term」の検索で、同じ名前の製品は出なかった
- GitHub の検索の上位は ARINC(航空の規格)・AR 関係・ARIN の Whois の道具で、「arin」という名前のターミナルは無い

### 商標

- 米国: 二次情報(商標の集約サイト)によると、ARIN の団体が「ARIN」を 42 類で登録している(登録番号 3047439、ほかに 3043320)
- **確かめられなかったこと**: USPTO の原本と指定役務の文言、9 類(ソフトウェア)の登録の有無、日本(J-PlatPat)の登録の有無、ARIN が名前の使い方の方針を出しているか

### 推測(確かめていない)

ARIN の登録は IP アドレスの登録の役務に向けたものとみられ、ターミナルのアプリとはまぎれにくい。大文字の「ARIN」と書いたり、ネットワークの道具と受け取られたりすると混同されやすくなる。

### 名前を変えない場合の手当て

1. 大文字の「ARIN」を使わない。説明では「Arin — macOS のライト・マルチターミナル」のように、何のアプリかを必ず添える
2. README に「American Registry for Internet Numbers とは関係がない」と 1 文書く。アイコンを ARIN のロゴに似せない
3. crates.io・npm に `arin` の名前で公開しない(このアプリは公開の予定が無い。クレート名は `arin-core`・`arin-platform`)
4. 公開の前に、本人が USPTO(9・42 類)と J-PlatPat(9・42 類。「アリン」も)を直接引く
