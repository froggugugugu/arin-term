# ADR-010: ウィンドウの地を macOS のすりガラス(vibrancy)にする

- ステータス: Accepted(実機での見た目は本人の確認待ち)
- 日付: 2026-11-15
- 決定者: 本人(半透過を入れることを決めた)。設定の中身(素材・状態・画面の重ね方)は AI(サブエージェント)が決めた
- 関連: `ARCH_multi-terminal.md` §9.2・§9.3(色とトークン)、`src/ui/design/tokens.css`、`src-tauri/tauri.conf.json`

## コンテキスト

本人の指示(原文):

- M2 の途中の画面を見て「デザインがAI特有のアルアルなデザインになってる。Appleデザインぽくして、ハイセンスなものにしたい。」
- ウィンドウの地について「ウィンドウ背景の半透過はほしかったなー。サイバーな感じがあってすきなんだよね。」

非公開の API(WKWebView の `drawsBackground`)を使うこと、その結果 Mac App Store に出せなくなることを伝えたうえで、半透過を入れることに決まった。配布は公証(notarize)して直接配る計画なので、App Store に出せないことは問題にならない。

## 判断

左ペイン(サイドバー)と上部の帯の地を、macOS のサイドバーの素材のすりガラスにする。ターミナルの置き場所は不透明(黒)のまま。

`src-tauri/tauri.conf.json`:

```json
"app": {
  "macOSPrivateApi": true,
  "windows": [{
    "transparent": true
  }]
}
```

すりガラスの素材は、設定の `windowEffects` には書かず、`src-tauri/src/lib.rs` の `apply_window_effects` が本物のランタイムで起動したときだけ付ける(`WebviewWindow::set_effects`。素材 `Effect::Sidebar`、状態 `EffectState::FollowsWindowActiveState`)。設定に書くと、模擬ランタイムを使う結合テスト(`src-tauri/tests/`)のウィンドウにも付けようとして落ちる(SIGSEGV。14 本)ため。

`src-tauri/Cargo.toml`: `tauri = { workspace = true, features = ["macos-private-api"] }`(版は `=2.12.5` のまま)。

- `transparent: true` でウィンドウと WKWebView の地を透明にし、`set_effects` で `NSVisualEffectView`(素材 `sidebar`)をウィンドウの全面に敷く。効果は透明なウィンドウでないと効かない
- 状態は `followsWindowActiveState`(macOS の標準。ウィンドウが手前でないときは素材が落ち着いた灰色になる)。`radius` は付けない(ウィンドウの角は OS が丸める)
- `macOSPrivateApi` は Tauri 2.12.1 以降は中身の無い設定(非公開 API は常に組み込まれる)。それでも、このアプリが非公開 API に頼ることを設定で明示するために書く。書くと `tauri-build` が `tauri` の機能 `macos-private-api` を要求する(設定と機能の一致の検査)ので、機能も足す。機能の中身も空で、`Cargo.lock` は変わらない(通信のクレートも増えない)
- 画面(`tokens.css`): `html`・`body`・`#root` は地を塗らず、`.app-shell` に半透明の淡い色(`--color-window-bg`。ライト `rgba(238, 238, 240, 0.55)`、ダーク `rgba(30, 30, 32, 0.5)`)を重ねる。左ペインと上部の帯は地を塗らないので、この色とすりガラスがそのまま見える。タブバーはわずかに影を落とした色、ターミナルの置き場所は不透明の黒
- CSP と capabilities は変えない(画面の CSS だけで済み、ウィンドウの操作の権限は要らない)
- ブラウザ(Playwright の WebKit)での扱い: Tauri の中かどうかで分けない。窓の向こうが無いブラウザでは、半透明の色がページの地(ライトは白、ダークは暗い灰色)に重なり、不透明の淡いグレーに見える。どちらの外観でも文字のコントラストは保たれる(撮影で確かめた)。E2E の偽の Rust 側は `__TAURI_INTERNALS__` を差し込むので、それを見て分けるとテストでは常に Tauri と判定されてしまい、分けても意味が無い

## 検討した代替案

| 案 | 採らなかった理由 |
| -- | ---------------- |
| 半透過を入れない(不透明の地) | 本人が求めている |
| `liquidGlassRegular`・`liquidGlassClear`(macOS 26 以降) | ウィンドウ全体のガラスで、サイドバーの見た目と違う。macOS 15 では効かない(最低 OS は 15.0)。素材 `sidebar` と並べて書けば 15 で `sidebar` に戻るが、見た目が OS で変わる |
| 素材 `underWindowBackground`・`hudWindow` など | `sidebar` が Finder・Xcode の左ペインと同じで、方針(Apple らしさ)に合う |
| 状態 `active`(常にすりガラス) | ウィンドウが手前でなくても色が付き続け、macOS の作法と違う。本人が「サイバー」さを優先するなら 1 行で切り替えられる |
| Rust でウィンドウを作り、左ペインの部分だけに素材を敷く | Rust のソースの変更が要り、今回の作業の範囲外。いまは画面の CSS で、ターミナルの置き場所を不透明にして同じ効果を得る |
| Tauri の中だけ透明にする(JS で判定して属性を付ける) | 上のとおり E2E では判定が常に真になり、半透明の色を重ねる方がブラウザでも破綻しない |

## 影響

- Mac App Store には出せない(非公開 API)。公証しての直接配布には影響しない
- 本人の判断(2026-11-17): 非公開 API に頼ることを承知のうえで、半透過は残す。Mac App Store には並べない。最終的にリポジトリを公開して OSS にする予定。公証が通ることは M5 で実際に確かめる
- ウィンドウ全体が透明になるので、地を塗らない要素はすべてすりガラスの上に乗る。新しい部品で地が要るときは、`tokens.css` の色(半透明なら `--color-window-bg` の上で読めること)を使う
- 半透明の色の上の文字は、背景(デスクトップの壁紙・後ろのウィンドウ)で見え方が変わる。`--color-window-bg` の濃さで調整する
- 見直す条件: 本人が実機で見て濃さ・状態を変えたいとき、Tauri 3 で `macOSPrivateApi` が無くなったとき(設定と機能を外す)、最低 OS を 26 に上げて Liquid Glass を使うとき

## 確かめた資料

- `~/.cargo/registry/src/index.crates.io-*/tauri-utils-2.10.2/src/config.rs`: `WindowConfig.transparent`、`WindowConfig.window_effects`(「Requires the window to be transparent」)、`WindowEffectsConfig { effects, state, radius, color, interactive }`、`AppConfig.macos_private_api`(`#[serde(rename = "macOSPrivateApi")]`、「No-op in Tauri 2.12.1+ because the APIs are always enabled now」)、`AppConfig::features()` が `macos-private-api` を返すこと
- 同 `tauri-utils-2.10.2/src/lib.rs`: `WindowEffect::Sidebar`(`rename_all = "camelCase"` で `"sidebar"`)、`WindowEffectState::FollowsWindowActiveState`(`"followsWindowActiveState"`)
- `tauri-2.12.5/src/window/mod.rs`(作ったウィンドウに設定の `window_effects` を適用)、`tauri-2.12.5/src/vibrancy/macos.rs`(`window_vibrancy::apply_vibrancy` に `NSVisualEffectMaterial::Sidebar` を渡す)、`tauri-2.12.5/Cargo.toml`(機能 `macos-private-api`)、`tauri-2.12.5/src/lib.rs`(機能の説明の「No-op since Tauri 2.12.1」)
- `tauri-build-2.7.4/src/manifest.rs`: 設定の機能と `Cargo.toml` の `tauri` の機能の一致の検査
- `wry-0.57.2/src/wkwebview/mod.rs`: `transparent` のとき `drawsBackground` を偽にする(非公開の KVC キー)。`wry-0.57.0/CHANGELOG.md`: macOS の `transparent` 機能を外し常に有効にした
- `node_modules/@tauri-apps/cli/config.schema.json`: 上の名前と値の一覧(スキーマ)
- 公式ドキュメント https://v2.tauri.app/reference/config/ : `macOSPrivateApi` の「No-op in Tauri 2.12.1+」、`windowEffects` の「Requires the window to be transparent」、`sidebar`(macOS 10.11+)、`WindowEffectState` の 3 つの値
