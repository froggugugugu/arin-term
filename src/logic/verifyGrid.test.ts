import { describe, expect, it } from "vitest";

import { isShown, SHOW_ALL, showOnly, visibilityChanges } from "./verifyGrid";

const IDS = Array.from({ length: 9 }, (_, i) => `terminal-${i + 1}`);

describe("9 枚の検証画面の表示の状態(T22)", () => {
  it("最初は全部を表示する", () => {
    expect(IDS.every((id) => isShown(SHOW_ALL, id))).toBe(true);
  });

  it("1 枚だけ表示では、その 1 枚だけを表示する", () => {
    const view = showOnly(IDS, "terminal-5");

    expect(view).toEqual({ mode: "single", id: "terminal-5" });
    expect(IDS.filter((id) => isShown(view, id))).toEqual(["terminal-5"]);
  });

  it("一覧に無い ID の 1 枚だけ表示は、全部を表示する", () => {
    expect(showOnly(IDS, "terminal-10")).toEqual(SHOW_ALL);
  });

  it("全部 → 1 枚だけでは、残りの 8 枚を隠すだけ(表示するものは無い)", () => {
    const changes = visibilityChanges(IDS, SHOW_ALL, showOnly(IDS, "terminal-1"));

    expect(changes).toEqual(IDS.slice(1).map((id) => ({ id, visible: false })));
  });

  it("1 枚だけ → 全部では、隠していた 8 枚を表示するだけ", () => {
    const changes = visibilityChanges(IDS, showOnly(IDS, "terminal-9"), SHOW_ALL);

    expect(changes).toEqual(IDS.slice(0, 8).map((id) => ({ id, visible: true })));
  });

  it("1 枚だけ → 別の 1 枚だけでは、先に隠してから表示する", () => {
    const changes = visibilityChanges(IDS, showOnly(IDS, "terminal-2"), showOnly(IDS, "terminal-7"));

    expect(changes).toEqual([
      { id: "terminal-2", visible: false },
      { id: "terminal-7", visible: true },
    ]);
  });

  it("同じ状態のあいだでは何も切り替えない", () => {
    expect(visibilityChanges(IDS, SHOW_ALL, SHOW_ALL)).toEqual([]);
    expect(visibilityChanges(IDS, showOnly(IDS, "terminal-3"), showOnly(IDS, "terminal-3"))).toEqual([]);
  });
});
