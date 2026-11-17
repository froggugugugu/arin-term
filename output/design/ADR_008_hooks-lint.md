# ADR-008: React のフックの検査に Biome 2.6.4 を使い、規則は 2 つだけ有効にする

- ステータス: Accepted
- 日付: 2026-11-15
- 決定者: AI(司令塔)。開発時だけの依存なので、本人から任された自律の範囲で決めた。本人の了承は M2 の報告で求める
- 関連: `ARCH_multi-terminal.md` §15.1 #10、§9.1。計画 T28a

## コンテキスト

フックの誤用(依存配列の漏れ、条件分岐の中での呼び出し)は `tsc` では検出できない。M2 でカード一覧、タブバー、ダイアログなどの部品が増える。`npm run lint` は `tsc -b`、clippy、`cargo fmt --check`、境界検査だけで、フックを見る検査が無かった。

## 判断

`@biomejs/biome` 2.6.4(2026-10-28 公開。MIT OR Apache-2.0)を開発用の依存に足し、`npm run lint` で `biome lint --error-on-warnings` を走らせる。

- 有効にする規則は `useExhaustiveDependencies` と `useHookAtTopLevel` の 2 つだけ(`biome.json`。`preset` は `none`)。整形、import の並べ替え、ほかの規則は切る
- 対象は `src/**/*.{ts,tsx}`。`src/ipc/generated` は除く
- わざと余分な依存を書くときは、`biome-ignore` に理由を書く

確かめたこと(2026-11-15、この Mac): いまのコードは違反 0 件。条件分岐の中の `useEffect` と依存の漏れを入れた一時ファイルでは 2 件の error で失敗した。

## 検討した代替案

| 案 | 足すもの | 採らなかった理由 |
| -- | -------- | ---------------- |
| ESLint + `eslint-plugin-react-hooks` + `@typescript-eslint/parser` | 直接 3 つ、推移的に約 120 件(うち約 20 件が公開から 2 週間未満) | 依存が多く、版を上げるたびに 2 週間の決まりを確かめ直す手間が大きい。parser が TypeScript の版の上限を持つ |
| 入れない | なし | 依存配列の誤りはテストをすり抜けやすい |

Biome が足すのは本体と、この Mac 向けのバイナリの 1 件だけで、JavaScript の推移的な依存は増えない。

## 影響

- 後片付けの中で `ref.current` を読む誤りは Biome では検出されない(ESLint のプラグインは検出する)。レビューで見る
- `package-lock.json` に各 OS 向けのバイナリの項目が載る(入るのは 1 件)
- 見直す条件: React Compiler を使うとき、誤検出が続いたとき、Biome の設定の形が変わったとき
