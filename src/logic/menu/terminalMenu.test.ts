import { describe, expect, it } from "vitest";

import { buildTerminalMenu } from "./terminalMenu";

describe("buildTerminalMenu", () => {
  it("コピー・ペースト・区切り・すべてを選択の順に並ぶ", () => {
    expect(buildTerminalMenu(true).map((e) => (e.kind === "command" ? e.label : "-"))).toEqual([
      "コピー",
      "ペースト",
      "-",
      "すべてを選択",
    ]);
  });
  it("選択が無ければコピーだけ選べない", () => {
    const enabled = buildTerminalMenu(false).flatMap((e) => (e.kind === "command" ? [[e.command, e.enabled]] : []));
    expect(enabled).toEqual([
      ["copy", false],
      ["paste", true],
      ["select-all", true],
    ]);
  });
});
