// エディタの「/」コマンド(ADR-017)。行の先頭か空白のあとで「/」を打つと候補を出し、選ぶと書式を差し込む。
// ここは候補の表・絞り込み・差し込む文の組み立てだけ(純粋)。CodeMirror への取り付けは src/editor/codemirror/slashCompletion.ts。

export interface SlashCommand {
  readonly id: string;
  /** 候補に出す名前。 */
  readonly label: string;
  /** 絞り込みで引ける別名(英語の小文字)。 */
  readonly aliases: readonly string[];
  /** 差し込む文(改行は `\n`)。 */
  readonly text: string;
  /** 差し込んだあとのカーソルの位置(`text` の中の文字の位置)。 */
  readonly cursor: number;
  /** 行の先頭に置く書式か。行の途中で選んだら、改行してから差し込む。 */
  readonly block: boolean;
  /**
   * 前の行が空でないとき、空行を挟む書式か。区切り線の `---` は段落のすぐ下だと、段落を見出しに変えてしまう(setext 見出し)。
   */
  readonly blankLineBefore?: boolean;
}

const at = (text: string, marker: string): number => {
  const index = text.indexOf(marker);
  if (index < 0) throw new Error(`「${marker}」が無い: ${text}`);
  return index;
};

/** 終わりにカーソルを置く 1 行の書式。 */
const line = (id: string, label: string, aliases: string[], text: string): SlashCommand => ({
  id,
  label,
  aliases,
  text,
  cursor: text.length,
  block: true,
});

const CODE = "```\n\n```";
const TABLE = "| 列 1 | 列 2 |\n| --- | --- |\n|  |  |";
const FLOWCHART = "```mermaid\nflowchart TD\n  A[始め] --> B[終わり]\n```";
const SEQUENCE = "```mermaid\nsequenceDiagram\n  A->>B: こんにちは\n```";

/** 候補の表(出す順)。 */
export const SLASH_COMMANDS: readonly SlashCommand[] = [
  line("heading-1", "見出し 1", ["h1", "heading1", "midashi1"], "# "),
  line("heading-2", "見出し 2", ["h2", "heading2", "midashi2"], "## "),
  line("heading-3", "見出し 3", ["h3", "heading3", "midashi3"], "### "),
  line("bullet-list", "箇条書き", ["ul", "bullet", "list"], "- "),
  line("ordered-list", "番号付きリスト", ["ol", "ordered", "number", "list"], "1. "),
  line("task-list", "チェックリスト", ["todo", "task", "check", "list"], "- [ ] "),
  line("quote", "引用", ["quote", "blockquote"], "> "),
  { ...line("rule", "区切り線", ["hr", "rule", "divider"], "---\n"), blankLineBefore: true },
  { id: "code", label: "コードの塊", aliases: ["code", "codeblock"], text: CODE, cursor: 4, block: true },
  { id: "table", label: "表", aliases: ["table"], text: TABLE, cursor: at(TABLE, "列 1"), block: true },
  {
    id: "mermaid-flowchart",
    label: "Mermaid のフローチャート",
    aliases: ["mermaid", "flowchart", "diagram", "zu"],
    text: FLOWCHART,
    cursor: at(FLOWCHART, "\n```"),
    block: true,
  },
  {
    id: "mermaid-sequence",
    label: "Mermaid のシーケンス図",
    aliases: ["mermaid", "sequence", "diagram", "zu"],
    text: SEQUENCE,
    cursor: at(SEQUENCE, "\n```"),
    block: true,
  },
];

/**
 * 問い合わせ(「/」のあとの文字)で絞り込む。空なら全部。名前か別名の先頭が合うものを先に、途中が合うものをあとに、
 * それぞれ表の順で並べる。大文字・小文字は区別しない。
 */
export function filterSlashCommands(query: string, commands: readonly SlashCommand[] = SLASH_COMMANDS): SlashCommand[] {
  const q = query.toLowerCase();
  if (q === "") return [...commands];
  const names = (c: SlashCommand) => [c.label.toLowerCase(), ...c.aliases];
  const prefix = commands.filter((c) => names(c).some((n) => n.startsWith(q)));
  const inner = commands.filter((c) => !prefix.includes(c) && names(c).some((n) => n.includes(q)));
  return [...prefix, ...inner];
}

/** 「/」の問い合わせ。`slash` は行の中の「/」の位置、`query` はそのあとの文字。 */
export interface SlashQuery {
  readonly slash: number;
  readonly query: string;
}

/**
 * カーソルより前の行の文字(`beforeCursor`)から「/」の問い合わせを取り出す。「/」は行の先頭か空白のあとだけ
 * (`a/b` やパスの途中では出さない)。問い合わせは空白と「/」を含まない。当てはまらなければ null。
 */
export function findSlashQuery(beforeCursor: string): SlashQuery | null {
  const match = /(?:^|\s)\/([^\s/]*)$/.exec(beforeCursor);
  if (match === null) return null;
  const query = match[1] ?? "";
  return { slash: beforeCursor.length - query.length - 1, query };
}

/** 差し込む文と、差し込んだあとのカーソルの位置(差し込む文の中の位置)。 */
export interface SlashInsertion {
  readonly text: string;
  readonly cursor: number;
}

/**
 * 「/問い合わせ」を置き換える文を作る。`linePrefix` は同じ行の「/」より前の文字、`previousLine` は 1 つ前の行(無ければ空)。
 * - 行の先頭用の書式で、前に字下げ以外の文字があれば、改行して新しい行に置く
 * - `blankLineBefore` の書式は、すぐ上の行が空でなければ空行を 1 つ挟む
 * - 前が字下げだけなら、2 行目からも同じ字下げを付ける(リストの中で使える)
 */
export function planSlashInsertion(command: SlashCommand, linePrefix: string, previousLine = ""): SlashInsertion {
  const indentOnly = /^\s*$/.test(linePrefix);
  if (command.block && !indentOnly) {
    const lead = command.blankLineBefore === true ? "\n\n" : "\n";
    return { text: `${lead}${command.text}`, cursor: command.cursor + lead.length };
  }
  if (command.blankLineBefore === true && previousLine.trim() !== "") {
    const shifted = planSlashInsertion({ ...command, blankLineBefore: false }, linePrefix);
    return { text: `\n${linePrefix}${shifted.text}`, cursor: shifted.cursor + 1 + linePrefix.length };
  }
  if (linePrefix === "" || !indentOnly) return { text: command.text, cursor: command.cursor };
  const text = command.text.split("\n").join(`\n${linePrefix}`);
  // カーソルより前の改行の数だけ、字下げの分ずれる
  const newlines = command.text.slice(0, command.cursor).split("\n").length - 1;
  return { text, cursor: command.cursor + newlines * linePrefix.length };
}
