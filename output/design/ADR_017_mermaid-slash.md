# ADR-017: Markdown のプレビューで Mermaid の図を出し、エディタに「/」コマンドを足す

- 状態: Accepted(本人が 2026-11-22 に要望を出し、表示の方法と「/」の範囲を選んで計画を了承した。実機の見た目は本人の確認待ち)
- 日付: 2026-11-22
- 関係: ADR-003(依存の版)、ADR-015(Markdown のエディタ。プレビューの 2 に例外を 1 つ足す)、ADR-016(テーマ)

## 背景(本人の要望と回答。原文)

「markdownエディタで、マーメイドの表示ができるようにしてほしい。あと、outline（OSS)の「/」コマンドのように便利な機能があると嬉しいんだけど実装できる？」

- Mermaid の表示の方法: 「画像として表示(推奨)」
- 「/」で出すもの: 「見出し・リスト類, コード・表, Mermaid の雛形」

ADR-015 の 2 はプレビューで `<img>` を作らず、HTML の文字列も差し込まない。Mermaid の図は SVG の文字列として作られるので、出すにはこの決まりに例外が要る。

## 決定

### 1. Mermaid の図は SVG を blob: の URL にして `<img>` で出す

- 言語名(フェンスの info の最初の語。大文字・小文字は区別しない)が `mermaid` のコードの塊を図にする(`src/editor/diagram.ts` の `isMermaidInfo`)。引用・リストの中の塊も同じ
- 描くのは `src/editor/mermaid/MermaidRenderer.ts`。SVG の文字列を `Blob`(`image/svg+xml`)にし、`URL.createObjectURL` の URL を `<img>` に渡す(`src/ui/editor/MermaidBlock.tsx`)
  - `<img>` の中の SVG は、スクリプト・リンク・外の読み込みが動かない。SVG を画面の DOM に直接入れない(`innerHTML`・`DOMParser` を使わない)
  - CSP は変えない(`img-src 'self' blob:` はもとからある)
  - 例外は「自前の描画が作った Mermaid の SVG」だけ。文書の画像(`![](...)`)は今までどおり読み込まない
- ライト用(`neutral`)とダーク用(`dark`)の 2 つを描き、`<picture>` の `<source media="(prefers-color-scheme: dark)">` で選ぶ。テーマの切替(ADR-016)は外観を替えるだけで追従し、JS で分けない
- 描けるまで・描けなかったときは、書かれた文をコードの塊のまま出す。失敗の理由は文字だけで出す(「図を描けませんでした: …」)
- 文が変わったら 300 ms 待ってから描く(プレビューの解析の 150 ms のあと。作った直後はすぐ描く)。前の図は新しい図が届くまで出したまま。URL は差し替えと外すときに手放す
- 図の React のキーは「何番目の図か」にする(位置のキーだと、上に段落を足すたびに作り直され、図がいったんコードの塊に戻った)
- 同じ文の結果は 32 件まで使い回す(mermaid を読めなかった失敗は使い回さず、次の描画でもう一度読む)。mermaid の設定は全体で 1 つなので、描画は 1 つずつ順に行う

### 2. mermaid の設定

| 項目 | 値 | 理由 |
| ---- | -- | ---- |
| `securityLevel` | `strict` | 図の中の HTML を文字にし、クリックの処理を付けない |
| `htmlLabels` | `false` | 文字を SVG の `<text>` にする(`<img>` の中で foreignObject に頼らない) |
| `suppressErrorRendering` | `true` | 失敗したときに mermaid がエラーの図を文書に残さない |
| 図の種類ごとの `useMaxWidth` | `false` | 図の大きさを SVG に書かせる。`width="100%"` のままだと小さな図が横いっぱいに拡大された |
| `fontFamily` | システムの書体(`-apple-system`・ヒラギノ角ゴ) | 画面の文字とそろえる |

### 3. 依存と境界

- `mermaid` 12.3.0(MIT。2026-11-03 公開。13 系は alpha なので使わない)
- 推移的な依存のうち公開から 2 週間未満の 2 つを `overrides` で止めた: `dompurify` 3.4.17(3.4.18 は 11-14 公開)、`cytoscape` 3.34.4(3.34.5・3.35.0 は 11-10 公開)
  - 3.4.18 には DOMPurify の無害化の抜け道の修正が入っている。Arin は図を `<img>` で出し、CSP がインラインのスクリプトを止めるので、2 週間たってから上げる(ADR-003 の規則のとおり)
- `elkjs` 0.12.1(EPL-2.0 OR GPL-3.0-or-later)が入る。EPL-2.0 を選び、`THIRD_PARTY_NOTICES.md` に MPL-2.0 と同じ形で表示する
- 境界検査に 3 つ足した(`scripts/check-boundaries.mjs`): `mermaid`・`@mermaid-js/*` を import してよいのは `src/editor/mermaid/` だけ(`mermaid-import`)、`src/editor/mermaid` は `src/ipc` を import しない(`mermaid-ipc`)、`src/ui` は `src/editor/mermaid` を import しない(`ui-import` に追加)
- `EditorManager` が描画を初めて図を描くときに動的 import で読む(CodeMirror と同じ。起動に重さを乗せない)

### 4. 「/」コマンド

- 行の先頭か空白のあとで「/」を打つと候補を出す。`a/b` やパスの途中、コードの塊・行内のコード・生の HTML・HTML のコメントの中、読み取り専用では出さない。Vim では挿入モードの入力のときだけ出る(ノーマルモードの「/」は Vim の検索のまま)
- 候補(12 個): 見出し 1〜3、箇条書き、番号付きリスト、チェックリスト、引用、区切り線、コードの塊、表(2 列の雛形)、Mermaid のフローチャート、Mermaid のシーケンス図。名前と英語の別名(`h2`・`todo`・`table`・`mermaid` など)の先頭で絞り、合わなければ途中で絞る
- 選ぶと「/問い合わせ」を書式に置き換え、決めた位置にカーソルを置く(コードの塊はフェンスの間、表は最初の列名の前)。行の途中で選んだら改行してから置く。前が字下げだけなら 2 行目からも同じ字下げを付ける。区切り線(`---`)は、すぐ上の行が空でなければ空行を挟む(段落のすぐ下の `---` は段落を見出しに変える。setext 見出し)
- 候補の表と組み立ては `src/logic/editor/slashCommands.ts`(純粋)、CodeMirror への取り付けは `src/editor/codemirror/slashCompletion.ts`。`basicSetup` にもとからある補完の仕組みに、言語の `autocomplete` として足す(`@codemirror/autocomplete` 6.20.4 を直接の依存に加えた。lock の版と同じ)
- 候補の表示中の ↑↓・Enter・Esc は補完が受ける。`editorKeyOwner` はメニューのキーだけを外へ出すので変えない。Vim では挿入モードの入力のときだけ出る
- 見た目は画面の右クリックのメニュー(`ContextMenu.css`)と同じトークン(すりガラスの地・角丸・選んだ行はアクセント色)

## 却下した案

| 案 | 理由 |
| -- | ---- |
| SVG を `DOMParser` で読んで画面に直接入れる | 図の文字を選べて色も追従しやすいが、外から来た文書の中身が画面の DOM に入る。消毒の検査とテストが増える。本人は画像を選んだ |
| 図を描くのを Rust に任せる | Mermaid はブラウザの描画と寸法の測りに頼るので、Rust では描けない |
| テーマを JS で見て 1 つだけ描く | ADR-016 の「画面の JS にテーマの切替を書かない」に反する。2 回描くほうが単純 |
| 日付・リンクの雛形も「/」に入れる | 本人が今回は選ばなかった |

## 影響

- 塊の大きさ(`vite build`): 本体 `index-*.js` は 724.8 kB(gzip 204.5 kB)。mermaid は図の種類ごとの別の塊で、初めて図を描くときに要るものだけ読む。大きいものは `elk-*.js` 1,499 kB、`cytoscape.esm-*.js` 435 kB、`katex-*.js` 259 kB、`mermaid.core-*.js` 84 kB。`dist/` の JS は合わせて 6.5 MB
- プレビューの DOM に `img`・`source`・`picture` が出るのは Mermaid の図のときだけ(`a[href]`・`script`・`svg`・`foreignObject` は出ない。E2E が確かめる)

## リスク

- 本物の CSP の下で mermaid が動くか(実行時の `<style>` は `style-src-elem 'unsafe-inline'` で足りる見込み)は、実機で確かめる。E2E は Vite の画面なので CSP が効いていない
- 日本語の IME がオンのとき、JIS 配列の「/」キーは「・」になる。「/」コマンドは半角の「/」だけで出る(英数に切り替えて打つ)
