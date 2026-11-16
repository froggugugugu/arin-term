// プレビューの部品(ADR-015 の 2、AC-E03・E10)。DOM を持たない Vitest の環境なので、静的な HTML に描いて確かめる。
// 文書は本物の解析(src/editor/codemirror/markdownAst)で木にする(テストは境界検査の対象の外)。

import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { parseMarkdown } from "../../editor/codemirror/markdownAst";
import type { MdBlock } from "../../editor/markdownTypes";
import { MarkdownPreview } from "./MarkdownPreview";

const render = (blocks: readonly MdBlock[], fontSize = 15) =>
  renderToStaticMarkup(createElement(MarkdownPreview, { blocks, fontSize }));
const renderSource = (source: string, fontSize = 15) => render(parseMarkdown(source), fontSize);

/** 開く・読み込む・動かす要素と、入力の要素。 */
const FORBIDDEN_TAGS = new Set(["a", "img", "iframe", "script", "input", "object", "embed", "link", "style", "video", "audio", "source", "form", "button"]);

/** 描いた HTML の開始タグ(名前と属性の名前)。文書の中の `<` は `&lt;` になるので、本物のタグだけが取れる。 */
function tagsOf(html: string): { name: string; attributes: string[] }[] {
  return [...html.matchAll(/<([a-zA-Z][a-zA-Z0-9]*)([^>]*)>/g)].map((m) => ({
    name: (m[1] ?? "").toLowerCase(),
    attributes: [...(m[2] ?? "").matchAll(/\s([a-zA-Z-:]+)=/g)].map((a) => (a[1] ?? "").toLowerCase()),
  }));
}

describe("MarkdownPreview(自前の木を React の要素にする)", () => {
  const documents: [string, string][] = [
    ["リンク", "[押す](https://example.com) [危ない](javascript:alert(1)) <https://a.example> https://b.example"],
    ["画像", '![猫](https://example.com/cat.png "題") ![](x.png)'],
    ["生の_html_のブロック", "<iframe src=\"https://example.com\"></iframe>\n\n<script>alert(1)</script>"],
    ["生の_html_のインライン", 'a <img src=x onerror=alert(1)> <a href="https://e.com">b</a> <input type="checkbox">'],
    ["スタイル", '<div style="color:red">赤</div>\n\n<style>body{display:none}</style>'],
    ["タスク", "- [x] 済み\n- [ ] まだ"],
    [
      "全部入り",
      "# 見出し\n\n段落 *強調* **太字** ~~消し~~ `コード`\n\n> 引用\n\n1. 一\n2. 二\n\n| a | b |\n|:-|-:|\n| 1 | 2 |\n\n```\n<script>\n```\n\n---\n行  \n改行",
    ],
  ];

  it.each(documents)("どの文書でも_a_href_img_iframe_script_input_を作らない(%s)", (_name, source) => {
    const html = renderSource(source);
    const tags = tagsOf(html);
    expect(tags.length).toBeGreaterThan(1);
    expect(tags.filter((tag) => FORBIDDEN_TAGS.has(tag.name))).toEqual([]);
    const attributes = tags.flatMap((tag) => tag.attributes);
    for (const forbidden of ["href", "src", "srcset", "action", "formaction", "id"]) expect(attributes).not.toContain(forbidden);
    expect(attributes.filter((name) => name.startsWith("on"))).toEqual([]);
    // style 属性は外側の 1 つ(本文の大きさ)だけ
    expect(attributes.filter((name) => name === "style")).toHaveLength(1);
    expect(html).toMatch(/^<div class="md-preview" style="/);
  });

  it("生の_html_は文字として出る", () => {
    const html = renderSource('<script>alert("x")</script>\n\nあ<b>い</b>');
    expect(html).toContain('<pre class="md-raw">&lt;script&gt;alert(&quot;x&quot;)&lt;/script&gt;</pre>');
    expect(html).toContain('あ<span class="md-raw">&lt;b&gt;</span>い<span class="md-raw">&lt;/b&gt;</span>');
  });

  it("画像は代わりの文字_リンクは開かない印と_url_の題", () => {
    const html = renderSource("![猫](cat.png) ![](x.png) [押す](javascript:alert(1)) [参照][r]");
    expect(html).toContain('<span class="md-image">[画像: 猫]</span>');
    expect(html).toContain('<span class="md-image">[画像]</span>');
    expect(html).toContain('<span class="md-link" title="リンクは開けません: javascript:alert(1)">押す</span>');
    expect(html).toContain('<span class="md-link" title="リンクは開けません">参照</span>');
  });

  it("タスクの印は読み取り専用の文字", () => {
    const html = renderSource("- [x] 済み\n- [ ] まだ");
    expect(html).toContain('<span class="md-task-mark" role="img" aria-label="済み">☑</span>');
    expect(html).toContain('<span class="md-task-mark" role="img" aria-label="未完了">☐</span>');
  });

  it("見出しに_id_を付けない", () => {
    const html = renderSource("# 一\n## 二\n###### 六");
    expect(html).toContain("<h1>一</h1><h2>二</h2><h6>六</h6>");
    expect(html).not.toMatch(/\sid=/);
  });

  it("要素の対応(段落・強調・コード・引用・リスト・表・水平線・改行)", () => {
    const html = renderSource(
      "a *b* **c** ~~d~~ `e`\n\n> q\n\n3. 三\n\n- x\n\n| l | c | r | n |\n|:-|:-:|-:|-|\n| 1 |\n\n```ts\nx < y\n```\n\n---\n\n行  \n改行",
    );
    expect(html).toContain(
      '<p>a <em>b</em> <strong>c</strong> <del>d</del> <code class="md-inline-code">e</code></p>',
    );
    expect(html).toContain("<blockquote><p>q</p></blockquote>");
    expect(html).toContain('<ol start="3"><li><p>三</p></li></ol>');
    expect(html).toContain("<ul><li><p>x</p></li></ul>");
    expect(html).toContain(
      '<thead><tr><th class="md-align-left">l</th><th class="md-align-center">c</th><th class="md-align-right">r</th><th>n</th></tr></thead>',
    );
    expect(html).toContain(
      '<tbody><tr><td class="md-align-left">1</td><td class="md-align-center"></td><td class="md-align-right"></td><td></td></tr></tbody>',
    );
    expect(html).toContain('<pre class="md-code"><code>x &lt; y</code></pre>');
    expect(html).toContain("<hr/>");
    expect(html).toContain("<p>行<br/>改行</p>");
  });

  it("番号つきのリストが_1_から始まるときは_start_を書かない", () => {
    expect(renderSource("1. 一")).toContain("<ol><li>");
  });

  it("打ち切った中身は書かれた文字のまま", () => {
    const html = render([
      { kind: "truncated", text: "> <b>深い</b>" },
      { kind: "paragraph", children: [{ kind: "truncated", text: "*深い*" }] },
    ]);
    expect(html).toContain('<pre class="md-truncated">&gt; &lt;b&gt;深い&lt;/b&gt;</pre>');
    expect(html).toContain('<p><span class="md-truncated">*深い*</span></p>');
  });

  it("本文の大きさが_props_に追従する", () => {
    expect(render([], 15)).toBe('<div class="md-preview" style="font-size:15px"></div>');
    expect(render([], 22)).toBe('<div class="md-preview" style="font-size:22px"></div>');
  });

  it("空の文書は空の外側だけ", () => {
    expect(renderSource("")).toBe('<div class="md-preview" style="font-size:15px"></div>');
  });
});
