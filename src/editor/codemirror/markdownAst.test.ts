import { describe, expect, it } from "vitest";

import { MD_MAX_DEPTH, type MdBlock, type MdInline } from "../markdownTypes";
import { parseMarkdown } from "./markdownAst";

const text = (t: string): MdInline => ({ kind: "text", text: t });
const para = (...children: MdInline[]): MdBlock => ({ kind: "paragraph", children });

/** 木の中のインラインの要素を、深さ優先で全部集める。 */
function allInlines(blocks: readonly MdBlock[]): MdInline[] {
  const out: MdInline[] = [];
  const inl = (xs: readonly MdInline[]) => {
    for (const x of xs) {
      out.push(x);
      if ("children" in x) inl(x.children);
    }
  };
  const blk = (bs: readonly MdBlock[]) => {
    for (const b of bs) {
      switch (b.kind) {
        case "heading":
        case "paragraph":
          inl(b.children);
          break;
        case "quote":
          blk(b.children);
          break;
        case "list":
          for (const item of b.items) blk(item.children);
          break;
        case "table":
          for (const cell of b.head) inl(cell);
          for (const row of b.rows) for (const cell of row) inl(cell);
          break;
        default:
          break;
      }
    }
  };
  blk(blocks);
  return out;
}

describe("parseMarkdown(lezer の構文木を自前の木にする。ADR-015 の 2)", () => {
  it("空の文書は空の並び", () => {
    expect(parseMarkdown("")).toEqual([]);
    expect(parseMarkdown("\n\n  \n")).toEqual([]);
  });

  it("見出し_1_から_6", () => {
    const blocks = parseMarkdown("# 一\n## 二\n### 三 ###\n#### 四\n##### 五\n###### 六\n\nセット\n===\n\nセット 2\n---");
    expect(blocks).toEqual([
      { kind: "heading", level: 1, children: [text("一")] },
      { kind: "heading", level: 2, children: [text("二")] },
      { kind: "heading", level: 3, children: [text("三")] },
      { kind: "heading", level: 4, children: [text("四")] },
      { kind: "heading", level: 5, children: [text("五")] },
      { kind: "heading", level: 6, children: [text("六")] },
      { kind: "heading", level: 1, children: [text("セット")] },
      { kind: "heading", level: 2, children: [text("セット 2")] },
    ]);
  });

  it("強調と太字と取り消し線", () => {
    expect(parseMarkdown("a *b* **c** ~~d~~ ***e***")).toEqual([
      para(
        text("a "),
        { kind: "emphasis", children: [text("b")] },
        text(" "),
        { kind: "strong", children: [text("c")] },
        text(" "),
        { kind: "strikethrough", children: [text("d")] },
        text(" "),
        { kind: "emphasis", children: [{ kind: "strong", children: [text("e")] }] },
      ),
    ]);
  });

  it("段落の中の改行と強制の改行", () => {
    expect(parseMarkdown("一行目\n  二行目  \n三行目\\\n四行目")).toEqual([
      para(text("一行目\n二行目"), { kind: "break" }, text("三行目"), { kind: "break" }, text("四行目")),
    ]);
  });

  it("番号つきとなしのリストとタスク", () => {
    expect(parseMarkdown("- a\n- [x] 済み\n  続き\n- [ ] まだ\n\n3. 三\n4. 四\n   - 入れ子")).toEqual([
      {
        kind: "list",
        ordered: false,
        start: null,
        items: [
          { checked: null, children: [para(text("a"))] },
          { checked: true, children: [para(text("済み\n続き"))] },
          { checked: false, children: [para(text("まだ"))] },
        ],
      },
      {
        kind: "list",
        ordered: true,
        start: 3,
        items: [
          { checked: null, children: [para(text("三"))] },
          {
            checked: null,
            children: [
              para(text("四")),
              { kind: "list", ordered: false, start: null, items: [{ checked: null, children: [para(text("入れ子"))] }] },
            ],
          },
        ],
      },
    ]);
  });

  it("大文字の_X_もタスクの済み", () => {
    const [list] = parseMarkdown("* [X] 大文字");
    expect(list).toEqual({
      kind: "list",
      ordered: false,
      start: null,
      items: [{ checked: true, children: [para(text("大文字"))] }],
    });
  });

  it("表の揃え", () => {
    expect(parseMarkdown("| 左 | 中 | 右 | 無 |\n|:--|:-:|--:|---|\n| 1 | **2** | | 4 |\n| a |")).toEqual([
      {
        kind: "table",
        align: ["left", "center", "right", null],
        head: [[text("左")], [text("中")], [text("右")], [text("無")]],
        rows: [
          [[text("1")], [{ kind: "strong", children: [text("2")] }], [], [text("4")]],
          [[text("a")], [], [], []],
        ],
      },
    ]);
  });

  it("両端に縦線の無い表も読む", () => {
    expect(parseMarkdown("a | b\n--|:-\n1 | 2")).toEqual([
      { kind: "table", align: [null, "left"], head: [[text("a")], [text("b")]], rows: [[[text("1")], [text("2")]]] },
    ]);
  });

  it("コードの塊とインラインのコード", () => {
    expect(parseMarkdown("```ts\nconst a = 1;\n  b\n```\n\n    インデント\n    二行目\n\n`` `x` `` と `y`\n\n```\n```")).toEqual([
      { kind: "code", info: "ts", text: "const a = 1;\n  b" },
      { kind: "code", info: "", text: "インデント\n二行目" },
      para({ kind: "code", text: "`x`" }, text(" と "), { kind: "code", text: "y" }),
      { kind: "code", info: "", text: "" },
    ]);
  });

  it("リストの中のコードの塊は項目の字下げを外す", () => {
    expect(parseMarkdown("- 項目\n\n  ```\n  一\n    二\n  ```")).toEqual([
      {
        kind: "list",
        ordered: false,
        start: null,
        items: [{ checked: null, children: [para(text("項目")), { kind: "code", info: "", text: "一\n  二" }] }],
      },
    ]);
  });

  it("引用と水平線", () => {
    expect(parseMarkdown("> 一\n> 二\n>\n> > 入れ子\n\n---\n\n***")).toEqual([
      { kind: "quote", children: [para(text("一\n二")), { kind: "quote", children: [para(text("入れ子"))] }] },
      { kind: "rule" },
      { kind: "rule" },
    ]);
  });

  it("生の_html_は文字のまま", () => {
    expect(parseMarkdown('<div class="x">\n<b>太</b>\n</div>\n\nあ<span style="color:red">い</span>う <!-- c -->')).toEqual([
      { kind: "raw", text: '<div class="x">\n<b>太</b>\n</div>' },
      para(
        text("あ"),
        { kind: "raw", text: '<span style="color:red">' },
        text("い"),
        { kind: "raw", text: "</span>" },
        text("う "),
        { kind: "raw", text: "<!-- c -->" },
      ),
    ]);
  });

  it("script_の要素も文字のまま", () => {
    expect(parseMarkdown("<script>alert(1)</script>\n\n<!-- コメント -->\n\n<?php x ?>")).toEqual([
      { kind: "raw", text: "<script>alert(1)</script>" },
      { kind: "raw", text: "<!-- コメント -->" },
      { kind: "raw", text: "<?php x ?>" },
    ]);
  });

  it("画像は代わりの文字だけ", () => {
    expect(parseMarkdown('![猫の *写真*](https://example.com/cat.png "題")')).toEqual([
      para({ kind: "image", alt: "猫の 写真" }),
    ]);
    // URL はどこにも残らない
    expect(JSON.stringify(parseMarkdown("![a](https://example.com/x.png)"))).not.toContain("example.com");
  });

  it("javascript_の_url_のリンクも文字の情報だけ", () => {
    expect(parseMarkdown('[押す](javascript:alert(1) "t") <https://a.example> https://b.example [参照][r]\n\n[r]: https://r.example')).toEqual([
      para(
        { kind: "link", url: "javascript:alert(1)", children: [text("押す")] },
        text(" "),
        { kind: "link", url: "https://a.example", children: [text("https://a.example")] },
        text(" "),
        { kind: "link", url: "https://b.example", children: [text("https://b.example")] },
        text(" "),
        { kind: "link", url: "", children: [text("参照")] },
      ),
    ]);
  });

  it("数値と_6_つの名前の文字参照を直しほかは書かれたまま", () => {
    expect(parseMarkdown("&amp; &lt; &gt; &quot; &apos; &nbsp; &#60; &#x3C; &#X1F600; &copy; &#0; &#x110000;")).toEqual([
      para(text("& < > \" '   < < \u{1F600} &copy; � �")),
    ]);
  });

  it("バックスラッシュの逃がしは記号だけにする", () => {
    expect(parseMarkdown("\\*星\\* \\<b\\>")).toEqual([para(text("*星* <b>"))]);
  });

  it("33_段の引用は_32_段で打ち切る", () => {
    const source = `${"> ".repeat(MD_MAX_DEPTH + 1)}深い`;
    let blocks = parseMarkdown(source);
    for (let depth = 1; depth <= MD_MAX_DEPTH; depth++) {
      expect(blocks).toHaveLength(1);
      const [quote] = blocks;
      if (quote?.kind !== "quote") throw new Error(`${depth} 段目が引用でない`);
      blocks = quote.children;
    }
    expect(blocks).toEqual([{ kind: "truncated", text: "> 深い" }]);
  });

  it("深いリストと強調の入れ子も打ち切り_数千段でも落ちない", () => {
    const quotes = parseMarkdown(`${">".repeat(5000)} x`);
    expect(quotes).toHaveLength(1);
    const list = parseMarkdown(Array.from({ length: 60 }, (_, i) => `${"  ".repeat(i)}- ${i}`).join("\n"));
    expect(JSON.stringify(list)).toContain('"truncated"');
    const emphasis = parseMarkdown(`${"*a ".repeat(100)}b${" a*".repeat(100)}`);
    expect(JSON.stringify(emphasis)).toContain('"truncated"');
  });

  it("画像の入れ子も段数で打ち切る(代わりの文字の中の画像を際限なくたどらない)", () => {
    const nested = (n: number) => `${"![".repeat(n)}x${"](u)".repeat(n)}`;
    expect(parseMarkdown(nested(3))).toEqual([para({ kind: "image", alt: "x" })]);

    const [deep] = parseMarkdown(nested(MD_MAX_DEPTH + 8));
    if (deep?.kind !== "paragraph") throw new Error("段落でない");
    const [image] = deep.children;
    if (image?.kind !== "image") throw new Error("画像でない");
    // 打ち切った先は書かれた文字のまま代わりの文字に残る
    expect(image.alt).toContain("![x](u)");
    expect(parseMarkdown(nested(3000))).toHaveLength(1);
  });

  it("リンクの参照の定義は出さない", () => {
    expect(parseMarkdown("[r]: https://r.example\n\n本文")).toEqual([para(text("本文"))]);
  });

  it("どの要素にも開く・読み込む情報を持たない", () => {
    const blocks = parseMarkdown("[a](u1) ![b](u2) <i>c</i>");
    for (const inline of allInlines(blocks)) {
      expect(["text", "link", "image", "raw"]).toContain(inline.kind);
    }
  });
});
