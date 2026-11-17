// Markdown の文書を、プレビューのための自前の木(../markdownTypes)にする(ADR-015 の 2)。
//
// 解析は `@lezer/markdown`(GFM: 表・タスクリスト・取り消し線・自動リンク)。lezer の木は文字の位置しか持たないので、
// ここで文字を切り出し、印(`#`・`>`・`*` など)を外して要素にする。HTML の文字列は作らない。
//   - 生の HTML(ブロック・タグ・コメント・処理命令)は書かれた文字のまま `raw`
//   - 画像は代わりの文字だけ(URL は捨てる)。リンクは URL の文字だけを持つ(開く情報を持たない)
//   - 文字参照は数値と 6 つの名前(amp・lt・gt・quot・apos・nbsp)だけを直し、ほかは書かれたまま(HTML の名前の表を持ち込まない)
//   - 入れ子(引用・リスト・強調・リンク・画像)は MD_MAX_DEPTH 段で打ち切り、それより深い中身は書かれた文字のまま `truncated`
//     (この変換と React の描画は再帰するので)
//   - lezer の木の組み立ても入れ子の深さだけ再帰する。極端に深い入れ子でスタックがあふれたら(RangeError)、
//     文書を丸ごと書かれた文字のまま `truncated` 1 つにする(あふれる深さは実行環境で違う)

import type { SyntaxNode } from "@lezer/common";
import { GFM, parser as commonMarkParser } from "@lezer/markdown";

import { MD_MAX_DEPTH, type MdAlign, type MdBlock, type MdInline, type MdListItem } from "../markdownTypes";

const parser = commonMarkParser.configure(GFM);

/** Markdown の文書を自前の木にする。空の文書は空の並び。 */
export function parseMarkdown(source: string): MdBlock[] {
  try {
    return new Converter(source).blocks(parser.parse(source).topNode, 0);
  } catch (error) {
    if (error instanceof RangeError) return [{ kind: "truncated", text: source }];
    throw error;
  }
}

/** 直に名前で直す文字参照(ADR-015 の 2 の 1)。 */
const NAMED_ENTITIES: Readonly<Record<string, string>> = {
  amp: "&",
  lt: "<",
  gt: ">",
  quot: '"',
  apos: "'",
  nbsp: " ",
};

/** 文字参照を文字に直す。直さない名前は書かれたまま返す。 */
function decodeEntity(raw: string): string {
  const body = raw.slice(1, -1);
  if (body.startsWith("#")) {
    const hex = body[1] === "x" || body[1] === "X";
    const code = Number.parseInt(body.slice(hex ? 2 : 1), hex ? 16 : 10);
    // CommonMark と同じく、0・範囲外・サロゲートは置き換えの文字にする
    if (!Number.isFinite(code) || code === 0 || code > 0x10ffff || (code >= 0xd800 && code <= 0xdfff)) return "�";
    return String.fromCodePoint(code);
  }
  return NAMED_ENTITIES[body] ?? raw;
}

function childrenOf(node: SyntaxNode): SyntaxNode[] {
  const out: SyntaxNode[] = [];
  for (let child = node.firstChild; child !== null; child = child.nextSibling) out.push(child);
  return out;
}

const HEADING = /^(?:ATXHeading|SetextHeading)([1-6])$/;

/** 文の中で、外して読む印(引用の続きの行の `>`、タスクの印)。 */
const SKIPPED_INLINE_MARKS = new Set(["QuoteMark", "TaskMarker"]);

/** インラインの要素を並べる。続けた文字はまとめ、行頭の字下げと行末の空白を外す。 */
class InlineBuilder {
  readonly out: MdInline[] = [];
  private atLineStart = false;

  /** 書かれた文字(印と要素のあいだ)。行頭の字下げと改行の前後の空白を外す。 */
  source(raw: string): void {
    let text = this.atLineStart ? raw.replace(/^[ \t]+/, "") : raw;
    text = text.replace(/[ \t]*\n[ \t]*/g, "\n");
    if (text === "") return;
    this.atLineStart = text.endsWith("\n");
    this.literal(text);
  }

  /** 空白を外さない文字(文字参照・逃がし)。 */
  literal(text: string): void {
    this.atLineStart = false;
    const last = this.out[this.out.length - 1];
    if (last?.kind === "text") last.text += text;
    else this.out.push({ kind: "text", text });
  }

  /** 外した印。続く文字の行頭の空白も外す(`> 続き` の空白)。 */
  skipMark(): void {
    this.atLineStart = true;
  }

  push(inline: MdInline): void {
    this.atLineStart = false;
    this.out.push(inline);
  }

  /** 並びの最初と最後の空白・改行を外して返す(`trim` は nbsp も外すので使わない)。 */
  finish(): MdInline[] {
    const first = this.out[0];
    if (first?.kind === "text") first.text = first.text.replace(/^[ \t\n]+/, "");
    const last = this.out[this.out.length - 1];
    if (last?.kind === "text") last.text = last.text.replace(/[ \t\n]+$/, "");
    return this.out.filter((inline) => inline.kind !== "text" || inline.text !== "");
  }
}

class Converter {
  constructor(private readonly src: string) {}

  private slice(from: number, to: number): string {
    return this.src.slice(from, to);
  }

  private lineStart(pos: number): number {
    return this.src.lastIndexOf("\n", pos - 1) + 1;
  }

  /**
   * 複数行の文字(コードの塊・生の HTML)を、容れ物(引用の `>`・リストの字下げ)を外して切り出す。
   * 各行の頭から、塊の 1 行目で塊の前にある分(行頭から `blockFrom` まで)の字数だけ、空白と `>` を外す。
   */
  private dedent(blockFrom: number, from: number, to: number): string {
    const width = blockFrom - this.lineStart(blockFrom);
    return this.slice(this.lineStart(from), to)
      .split("\n")
      .map((line) => {
        let cut = 0;
        while (cut < width && cut < line.length && /[ \t>]/.test(line[cut] ?? "")) cut++;
        return line.slice(cut);
      })
      .join("\n");
  }

  blocks(parent: SyntaxNode, depth: number): MdBlock[] {
    const out: MdBlock[] = [];
    for (const child of childrenOf(parent)) {
      const block = this.block(child, depth);
      if (block !== null) out.push(block);
    }
    return out;
  }

  private block(node: SyntaxNode, depth: number): MdBlock | null {
    const name = node.name;
    const heading = HEADING.exec(name);
    if (heading !== null) {
      const level = Number(heading[1]) as 1 | 2 | 3 | 4 | 5 | 6;
      const marks = childrenOf(node).filter((child) => child.name === "HeaderMark");
      let from = node.from;
      let to = node.to;
      if (name.startsWith("ATX")) {
        const [open, ...rest] = marks;
        if (open !== undefined && open.from === node.from) from = open.to;
        const close = rest[rest.length - 1];
        if (close !== undefined) to = close.from;
      } else {
        const underline = marks[marks.length - 1];
        if (underline !== undefined) to = underline.from;
      }
      return { kind: "heading", level, children: this.inlines(node, from, to, depth) };
    }
    switch (name) {
      case "Paragraph":
        return { kind: "paragraph", children: this.inlines(node, node.from, node.to, depth) };
      case "FencedCode":
      case "CodeBlock": {
        const parts = childrenOf(node);
        const info = parts.find((child) => child.name === "CodeInfo");
        const texts = parts.filter((child) => child.name === "CodeText");
        const first = texts[0];
        const last = texts[texts.length - 1];
        const text = first === undefined || last === undefined ? "" : this.dedent(node.from, first.from, last.to);
        return { kind: "code", info: info === undefined ? "" : this.slice(info.from, info.to), text };
      }
      case "Blockquote":
        if (depth >= MD_MAX_DEPTH) return this.truncated(node);
        return { kind: "quote", children: this.blocks(node, depth + 1) };
      case "BulletList":
      case "OrderedList":
        if (depth >= MD_MAX_DEPTH) return this.truncated(node);
        return this.list(node, name === "OrderedList", depth + 1);
      case "Table":
        return this.table(node, depth);
      case "HorizontalRule":
        return { kind: "rule" };
      case "HTMLBlock":
      case "CommentBlock":
      case "ProcessingInstructionBlock":
        return { kind: "raw", text: this.dedent(node.from, node.from, node.to).replace(/\n+$/, "") };
      // 参照のリンクの定義と、容れ物の印は出さない
      case "LinkReference":
      case "QuoteMark":
      case "ListMark":
        return null;
      default:
        // GFM の範囲では来ない。来たら書かれたまま出す
        return this.truncated(node);
    }
  }

  private truncated(node: SyntaxNode): { kind: "truncated"; text: string } {
    return { kind: "truncated", text: this.slice(node.from, node.to) };
  }

  private list(node: SyntaxNode, ordered: boolean, depth: number): MdBlock {
    const items: MdListItem[] = [];
    let start: number | null = null;
    for (const item of childrenOf(node)) {
      if (item.name !== "ListItem") continue;
      let checked: boolean | null = null;
      const children: MdBlock[] = [];
      for (const child of childrenOf(item)) {
        if (child.name === "ListMark") {
          if (ordered && start === null) start = Number.parseInt(this.slice(child.from, child.to), 10);
          continue;
        }
        if (child.name === "Task") {
          const marker = childrenOf(child).find((part) => part.name === "TaskMarker");
          checked = marker !== undefined && /\[[xX]\]/.test(this.slice(marker.from, marker.to));
          children.push({ kind: "paragraph", children: this.inlines(child, child.from, child.to, depth) });
          continue;
        }
        const block = this.block(child, depth);
        if (block !== null) children.push(block);
      }
      items.push({ checked, children });
    }
    return { kind: "list", ordered, start: ordered ? (start ?? 1) : null, items };
  }

  private table(node: SyntaxNode, depth: number): MdBlock {
    const parts = childrenOf(node);
    const delimiter = parts.find((child) => child.name === "TableDelimiter");
    const align = delimiter === undefined ? [] : parseAlign(this.slice(delimiter.from, delimiter.to));
    const cellsOf = (row: SyntaxNode): MdInline[][] => {
      const cells: MdInline[][] = Array.from({ length: align.length }, () => []);
      let slot = 0;
      for (const child of childrenOf(row)) {
        if (child.name === "TableDelimiter") {
          if (child.from !== row.from) slot++;
        } else if (child.name === "TableCell" && slot < cells.length) {
          cells[slot] = this.inlines(child, child.from, child.to, depth);
        }
      }
      return cells;
    };
    const header = parts.find((child) => child.name === "TableHeader");
    return {
      kind: "table",
      align,
      head: header === undefined ? align.map(() => []) : cellsOf(header),
      rows: parts.filter((child) => child.name === "TableRow").map(cellsOf),
    };
  }

  /** `parent` の子のうち [from, to) にあるものと、そのあいだの文字をインラインの並びにする。 */
  private inlines(parent: SyntaxNode, from: number, to: number, depth: number): MdInline[] {
    const builder = new InlineBuilder();
    this.fill(builder, parent, from, to, depth);
    return builder.finish();
  }

  private fill(builder: InlineBuilder, parent: SyntaxNode, from: number, to: number, depth: number): void {
    let pos = from;
    for (const child of childrenOf(parent)) {
      if (child.to <= from || child.from >= to) continue;
      if (child.from > pos) builder.source(this.slice(pos, child.from));
      this.inline(builder, child, depth);
      pos = Math.max(pos, child.to);
    }
    if (pos < to) builder.source(this.slice(pos, to));
  }

  /** 印(最初と最後の子)のあいだを中身にする容れ物(強調・太字・取り消し線)。 */
  private wrapped(node: SyntaxNode, depth: number, mark: string): MdInline[] {
    const marks = childrenOf(node).filter((child) => child.name === mark);
    const open = marks[0];
    const close = marks[marks.length - 1];
    const from = open === undefined ? node.from : open.to;
    const to = close === undefined || close === open ? node.to : close.from;
    const builder = new InlineBuilder();
    this.fill(builder, node, from, to, depth + 1);
    return builder.out;
  }

  private inline(builder: InlineBuilder, node: SyntaxNode, depth: number): void {
    const name = node.name;
    if (SKIPPED_INLINE_MARKS.has(name)) {
      builder.skipMark();
      return;
    }
    const nests =
      name === "Emphasis" || name === "StrongEmphasis" || name === "Strikethrough" || name === "Link" || name === "Image";
    if (nests && depth >= MD_MAX_DEPTH) {
      builder.push({ kind: "truncated", text: this.slice(node.from, node.to) });
      return;
    }
    switch (name) {
      case "Emphasis":
        builder.push({ kind: "emphasis", children: this.wrapped(node, depth, "EmphasisMark") });
        return;
      case "StrongEmphasis":
        builder.push({ kind: "strong", children: this.wrapped(node, depth, "EmphasisMark") });
        return;
      case "Strikethrough":
        builder.push({ kind: "strikethrough", children: this.wrapped(node, depth, "StrikethroughMark") });
        return;
      case "InlineCode": {
        const marks = childrenOf(node).filter((child) => child.name === "CodeMark");
        const open = marks[0];
        const close = marks[marks.length - 1];
        let text = this.slice(open?.to ?? node.from, close === open ? node.to : (close?.from ?? node.to));
        text = text.replace(/\n/g, " ");
        // CommonMark: 両端に空白があり、空白だけでなければ 1 つずつ外す
        if (text.length >= 2 && text.startsWith(" ") && text.endsWith(" ") && text.trim() !== "") text = text.slice(1, -1);
        builder.push({ kind: "code", text });
        return;
      }
      case "Link": {
        const parts = childrenOf(node);
        const marks = parts.filter((child) => child.name === "LinkMark");
        const url = parts.find((child) => child.name === "URL");
        const open = marks[0];
        const close = marks[1];
        const inner = new InlineBuilder();
        this.fill(inner, node, open?.to ?? node.from, close?.from ?? node.to, depth + 1);
        builder.push({ kind: "link", url: url === undefined ? "" : this.slice(url.from, url.to), children: inner.finish() });
        return;
      }
      case "Autolink":
      case "URL": {
        const url = name === "URL" ? node : childrenOf(node).find((child) => child.name === "URL");
        const text = url === undefined ? this.slice(node.from, node.to) : this.slice(url.from, url.to);
        builder.push({ kind: "link", url: text, children: [{ kind: "text", text }] });
        return;
      }
      case "Image": {
        const marks = childrenOf(node).filter((child) => child.name === "LinkMark");
        const inner = new InlineBuilder();
        // 代わりの文字は中身を平らな文字にしたもの(強調などの印は外す)
        this.fill(inner, node, marks[0]?.to ?? node.from, marks[1]?.from ?? node.to, depth + 1);
        builder.push({ kind: "image", alt: plainText(inner.finish()) });
        return;
      }
      case "HTMLTag":
      case "Comment":
      case "ProcessingInstruction":
        builder.push({ kind: "raw", text: this.slice(node.from, node.to) });
        return;
      case "Entity":
        builder.literal(decodeEntity(this.slice(node.from, node.to)));
        return;
      case "Escape":
        builder.literal(this.slice(node.from + 1, node.to));
        return;
      case "HardBreak":
        builder.push({ kind: "break" });
        builder.skipMark();
        return;
      default:
        // 印(EmphasisMark など)が単独で来たときと、知らない要素は書かれたまま
        builder.literal(this.slice(node.from, node.to));
    }
  }
}

/** 表の区切りの行(`|:--|:-:|`)から列の揃えを読む。 */
function parseAlign(row: string): MdAlign[] {
  let line = row.trim();
  if (line.startsWith("|")) line = line.slice(1);
  if (line.endsWith("|")) line = line.slice(0, -1);
  return line.split("|").map((cell) => {
    const spec = cell.trim();
    const left = spec.startsWith(":");
    const right = spec.endsWith(":");
    if (left && right) return "center";
    if (right) return "right";
    if (left) return "left";
    return null;
  });
}

/** インラインの並びを平らな文字にする(画像の代わりの文字)。 */
function plainText(inlines: readonly MdInline[]): string {
  return inlines
    .map((inline) => {
      switch (inline.kind) {
        case "text":
        case "code":
        case "raw":
        case "truncated":
          return inline.text;
        case "image":
          return inline.alt;
        case "break":
          return "\n";
        default:
          return plainText(inline.children);
      }
    })
    .join("");
}
