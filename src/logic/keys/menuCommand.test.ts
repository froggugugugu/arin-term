import { describe, expect, it } from "vitest";

import type { ClipboardFocus } from "./clipboard";
import { routeMenuCommand } from "./menuCommand";

describe("routeMenuCommand(メニューから届く操作の振り分け)", () => {
  it("すべてを選択はターミナルにフォーカスがあればターミナルの全選択", () => {
    expect(routeMenuCommand("select-all", { kind: "terminal" })).toBe("select-all-terminal");
  });

  it("コピー・ペーストのフォーカス位置(選択の有無つき)もそのまま渡せる", () => {
    const focus: ClipboardFocus = { kind: "terminal", hasSelection: true };
    expect(routeMenuCommand("select-all", focus)).toBe("select-all-terminal");
  });

  it("それ以外では画面の既定に任せる", () => {
    expect(routeMenuCommand("select-all", { kind: "other" })).toBe("none");
  });
});
