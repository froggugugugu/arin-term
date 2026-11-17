# サードパーティのライセンス表示

Arin 本体は MIT License([LICENSE](LICENSE))です。

## アプリ(Arin.app)

アプリに組み込まれている Rust クレート(Tauri など)と npm パッケージ(React・xterm.js・CodeMirror など)のライセンス・著作権表示の全文は [THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md) にあります(`scripts/gen-third-party-licenses.mjs` が `Cargo.lock` と `package-lock.json` から生成)。複数のライセンスから選べるパッケージのうち、ライセンスファイルを同梱していないものは MIT License を選んでいます。

配布アプリ(DMG / zip)には、このファイル・THIRD_PARTY_LICENSES.md・[LICENSE](LICENSE) を `Arin.app/Contents/Resources/licenses/` に同梱しています。

## Mozilla Public License 2.0 の部品

アプリには、Mozilla Public License 2.0(MPL-2.0)の次の 5 つのクレートが含まれます(Tauri が引き込むもの)。Arin はこれらのソースに手を加えていません。ライセンスの全文は THIRD_PARTY_LICENSES.md、または https://mozilla.org/MPL/2.0/ にあります。

| クレート | 版 | ソースの入手先 |
| -------- | -- | -------------- |
| cssparser | 0.37.0 | https://github.com/servo/rust-cssparser 、https://crates.io/crates/cssparser/0.37.0 |
| cssparser-macros | 0.7.1 | https://github.com/servo/rust-cssparser 、https://crates.io/crates/cssparser-macros/0.7.1 |
| selectors | 0.38.0 | https://github.com/servo/stylo 、https://crates.io/crates/selectors/0.38.0 |
| dtoa-short | 0.3.5 | https://github.com/upsuper/dtoa-short 、https://crates.io/crates/dtoa-short/0.3.5 |
| option-ext | 0.2.0 | https://github.com/soc/option-ext 、https://crates.io/crates/option-ext/0.2.0 |

版は `Cargo.lock` のものです。依存を更新して版が変わったときは、この表も合わせて直します。

## 商標

macOS は Apple Inc. の商標です。Arin は Apple Inc. とは関係のない、個人が作っているツールです。
