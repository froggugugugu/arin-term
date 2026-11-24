import { describe, expect, it } from "vitest";

import { filterSlashCommands, findSlashQuery, planSlashInsertion, SLASH_COMMANDS, type SlashCommand } from "./slashCommands";

const command = (id: string): SlashCommand => {
  const found = SLASH_COMMANDS.find((c) => c.id === id);
  if (found === undefined) throw new Error(id);
  return found;
};

describe("SLASH_COMMANDS(「/」コマンドの表。ADR-017)", () => {
  it("ID は重ならず、カーソルは差し込む文の中にある", () => {
    expect(new Set(SLASH_COMMANDS.map((c) => c.id)).size).toBe(SLASH_COMMANDS.length);
    for (const c of SLASH_COMMANDS) {
      expect(c.cursor).toBeGreaterThanOrEqual(0);
      expect(c.cursor).toBeLessThanOrEqual(c.text.length);
    }
  });

  it("コードの塊はフェンスの間の空の行に、表は最初の列名に、Mermaid は最後の行の終わりにカーソルを置く", () => {
    const code = command("code");
    expect(code.text.slice(0, code.cursor)).toBe("```\n");
    const table = command("table");
    expect(table.text.slice(table.cursor)).toMatch(/^列 1/);
    const flow = command("mermaid-flowchart");
    expect(flow.text.slice(flow.cursor)).toBe("\n```");
  });
});

describe("filterSlashCommands(絞り込み)", () => {
  it("空なら全部を表の順で返す", () => {
    expect(filterSlashCommands("").map((c) => c.id)).toEqual(SLASH_COMMANDS.map((c) => c.id));
  });

  it("名前と別名の先頭で引け、大文字・小文字は区別しない", () => {
    expect(filterSlashCommands("H2").map((c) => c.id)).toEqual(["heading-2"]);
    expect(filterSlashCommands("見出し").map((c) => c.id)).toEqual(["heading-1", "heading-2", "heading-3"]);
    expect(filterSlashCommands("todo").map((c) => c.id)).toEqual(["task-list"]);
    expect(filterSlashCommands("mermaid").map((c) => c.id)).toEqual(["mermaid-flowchart", "mermaid-sequence"]);
  });

  it("先頭が合うものを先に、途中が合うものをあとに並べる", () => {
    // 「list」は箇条書き・番号付き・チェックリストの別名の先頭。「リスト」は番号付きリスト・チェックリストの名前の途中
    expect(filterSlashCommands("list").map((c) => c.id)).toEqual(["bullet-list", "ordered-list", "task-list"]);
    expect(filterSlashCommands("リスト").map((c) => c.id)).toEqual(["ordered-list", "task-list"]);
    expect(filterSlashCommands("lock").map((c) => c.id)).toEqual(["quote", "code"]);
  });

  it("合わなければ空", () => {
    expect(filterSlashCommands("zzz")).toEqual([]);
  });
});

describe("findSlashQuery(「/」の問い合わせを取り出す)", () => {
  it.each([
    ["/", { slash: 0, query: "" }],
    ["/h2", { slash: 0, query: "h2" }],
    ["  /見出", { slash: 2, query: "見出" }],
    ["文の途中 /表", { slash: 5, query: "表" }],
    ["タブ\t/x", { slash: 3, query: "x" }],
  ])("%j → %j", (before, expected) => {
    expect(findSlashQuery(before)).toEqual(expected);
  });

  it.each([["a/b"], ["https://example"], ["/usr/"], ["/h2 "], ["文字/"], [""]])("%j では出さない", (before) => {
    expect(findSlashQuery(before)).toBeNull();
  });
});

describe("planSlashInsertion(差し込む文を作る)", () => {
  it("行の先頭ならそのまま", () => {
    expect(planSlashInsertion(command("heading-2"), "")).toEqual({ text: "## ", cursor: 3 });
  });

  it("行の途中で行の先頭用の書式を選んだら、改行してから置く", () => {
    expect(planSlashInsertion(command("bullet-list"), "文 ")).toEqual({ text: "\n- ", cursor: 3 });
    const code = planSlashInsertion(command("code"), "文 ");
    expect(code.text).toBe("\n```\n\n```");
    expect(code.text.slice(0, code.cursor)).toBe("\n```\n");
  });

  it("前が字下げだけなら、2 行目からも同じ字下げを付け、カーソルもずらす", () => {
    expect(planSlashInsertion(command("heading-1"), "  ")).toEqual({ text: "# ", cursor: 2 });
    const code = planSlashInsertion(command("code"), "  ");
    expect(code.text).toBe("```\n  \n  ```");
    expect(code.text.slice(0, code.cursor)).toBe("```\n  ");
    const flow = planSlashInsertion(command("mermaid-flowchart"), "\t");
    expect(flow.text).toBe("```mermaid\n\tflowchart TD\n\t  A[始め] --> B[終わり]\n\t```");
    expect(flow.text.slice(flow.cursor)).toBe("\n\t```");
  });

  it("区切り線は、段落のすぐ下や行の途中なら空行を挟む(段落が見出しに変わらないように)", () => {
    expect(planSlashInsertion(command("rule"), "", "段落")).toEqual({ text: "\n---\n", cursor: 5 });
    expect(planSlashInsertion(command("rule"), "文 ")).toEqual({ text: "\n\n---\n", cursor: 6 });
    expect(planSlashInsertion(command("rule"), "", "")).toEqual({ text: "---\n", cursor: 4 });
    expect(planSlashInsertion(command("rule"), "", "   ")).toEqual({ text: "---\n", cursor: 4 });
    // 字下げつき: 空行は字下げなし、区切り線の行と次の行は同じ字下げ
    expect(planSlashInsertion(command("rule"), "  ", "  - 項目")).toEqual({ text: "\n  ---\n  ", cursor: 9 });
    // ほかの書式は段落のすぐ下でも空行を挟まない
    expect(planSlashInsertion(command("bullet-list"), "", "段落")).toEqual({ text: "- ", cursor: 2 });
  });
});
