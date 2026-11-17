// プレビューに使う Markdown の木(ADR-015 の 2)。React も lezer も含まない純粋なデータ。
//
// `src/editor/codemirror/markdownAst.ts` が lezer の構文木からこの木を作り、`src/ui/editor/MarkdownPreview.tsx` が
// React の要素にする。HTML の文字列は作らない。開く・読み込む情報は持たない:
//   - 生の HTML(ブロックとインライン)は書かれた文字のまま `raw` に入れる
//   - 画像は代わりの文字(`alt`)だけ。URL は持たない
//   - リンクは URL の文字を持つが、プレビューは開かない(`title` に出すだけ)
//   - 入れ子が深すぎる中身は書かれた文字のまま `truncated` に入れる(MD_MAX_DEPTH)

/** 入れ子(引用・リスト・強調など)の上限の段数。これより深い中身は書かれた文字のまま出す。 */
export const MD_MAX_DEPTH = 32;

/** 文の中の要素。 */
export type MdInline =
  | { kind: "text"; text: string }
  | { kind: "emphasis"; children: MdInline[] }
  | { kind: "strong"; children: MdInline[] }
  | { kind: "strikethrough"; children: MdInline[] }
  | { kind: "code"; text: string }
  /** リンク。`url` は書かれた文字のまま(検査しない。開かないので)。参照のリンクで URL が無いときは空の文字。 */
  | { kind: "link"; url: string; children: MdInline[] }
  /** 画像。読み込まないので、代わりの文字だけを持つ。 */
  | { kind: "image"; alt: string }
  /** 生の HTML のタグ・コメント・処理命令(書かれた文字のまま)。 */
  | { kind: "raw"; text: string }
  /** 強制の改行(行末の 2 つの空白か `\`)。 */
  | { kind: "break" }
  /** 入れ子が深すぎて打ち切った中身(書かれた文字のまま)。 */
  | { kind: "truncated"; text: string };

/** 表の列の揃え。`null` は指定なし。 */
export type MdAlign = "left" | "center" | "right" | null;

/** リストの項目 1 つ。`checked` はタスクの印(`[x]` は真、`[ ]` は偽)。タスクでなければ `null`。 */
export interface MdListItem {
  checked: boolean | null;
  children: MdBlock[];
}

/** ブロックの要素。 */
export type MdBlock =
  | { kind: "heading"; level: 1 | 2 | 3 | 4 | 5 | 6; children: MdInline[] }
  | { kind: "paragraph"; children: MdInline[] }
  /** コードの塊。`info` はフェンスの言語名など(色付けはしない。言語を読み込まない)。 */
  | { kind: "code"; info: string; text: string }
  | { kind: "quote"; children: MdBlock[] }
  /** リスト。`start` は番号つきの最初の番号(番号なしは `null`)。 */
  | { kind: "list"; ordered: boolean; start: number | null; items: MdListItem[] }
  /** 表。`head` と各行のセルの数は `align` の数にそろえる。 */
  | { kind: "table"; align: MdAlign[]; head: MdInline[][]; rows: MdInline[][][] }
  | { kind: "rule" }
  /** 生の HTML のブロック・コメント・処理命令(書かれた文字のまま)。 */
  | { kind: "raw"; text: string }
  /** 入れ子が深すぎて打ち切った中身(書かれた文字のまま)。 */
  | { kind: "truncated"; text: string };
