import { describe, expect, it } from "vitest";

import type { ClipboardFocus } from "./clipboard";
import { routeMenuCommand } from "./menuCommand";

describe("routeMenuCommand(メニューから届く操作の振り分け)", () => {
  it("すべてを選択の振り分けは変わらない", () => {
    expect(routeMenuCommand("select-all", { kind: "terminal" })).toBe("select-all-terminal");
    expect(routeMenuCommand("select-all", { kind: "tree" })).toBe("none");
    expect(routeMenuCommand("select-all", { kind: "name-input" })).toBe("none");
    expect(routeMenuCommand("select-all", { kind: "other" })).toBe("none");
  });

  it("設定を開くのメニューの操作は_フォーカス位置によらず設定画面を開く", () => {
    for (const kind of ["terminal", "tree", "name-input", "other"] as const) {
      expect(routeMenuCommand("open-settings", { kind })).toBe("open-settings");
    }
  });

  it("すべてを選択はターミナルにフォーカスがあればターミナルの全選択", () => {
    expect(routeMenuCommand("select-all", { kind: "terminal" })).toBe("select-all-terminal");
  });

  it("コピー・ペーストのフォーカス位置(選択の有無つき)もそのまま渡せる", () => {
    const focus: ClipboardFocus = { kind: "terminal", hasSelection: true };
    expect(routeMenuCommand("select-all", focus)).toBe("select-all-terminal");
  });

  it("それ以外では画面の既定に任せる", () => {
    expect(routeMenuCommand("select-all", { kind: "other" })).toBe("none");
    expect(routeMenuCommand("select-all", { kind: "name-input" })).toBe("none");
  });

  it("ツリーにフォーカスがあっても、すべてを選択は何もしない(選択は 1 つだけ。複数選択は M6)", () => {
    expect(routeMenuCommand("select-all", { kind: "tree" })).toBe("none");
    const focus: ClipboardFocus = { kind: "tree", selection: "entry" };
    expect(routeMenuCommand("select-all", focus)).toBe("none");
  });
});

describe("エディタ(M4.8。ADR-015 の 6)", () => {
  it("エディタにフォーカスがあるときすべてを選択はエディタへ", () => {
    expect(routeMenuCommand("select-all", { kind: "editor" })).toBe("select-all-editor");
    expect(routeMenuCommand("open-settings", { kind: "editor" })).toBe("open-settings");
  });
});
