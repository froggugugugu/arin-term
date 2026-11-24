import { describe, expect, it } from "vitest";

import { buildTabMenu, type TabRef, tabsToClose } from "./tabMenu";

const T1: TabRef = { kind: "terminal", id: "t1" };
const T2: TabRef = { kind: "terminal", id: "t2" };
const E1: TabRef = { kind: "editor", id: "e1" };
const E2: TabRef = { kind: "editor", id: "e2" };
const ORDER = [T1, T2, E1, E2];

const enabled = (order: readonly TabRef[], target: TabRef) =>
  buildTabMenu(order, target).map((entry) => [entry.label, entry.enabled]);

describe("buildTabMenu", () => {
  it("3 つの項目を決まった順に出す", () => {
    expect(enabled(ORDER, T2)).toEqual([
      ["タブを閉じる", true],
      ["ほかのタブを閉じる", true],
      ["これより右を閉じる", true],
    ]);
  });

  it("右端のタブでは「これより右を閉じる」、1 枚だけなら「ほかのタブを閉じる」も選べない", () => {
    expect(enabled(ORDER, E2)[2]).toEqual(["これより右を閉じる", false]);
    expect(enabled([T1], T1)).toEqual([
      ["タブを閉じる", true],
      ["ほかのタブを閉じる", false],
      ["これより右を閉じる", false],
    ]);
  });

  it("ID が同じでも種類が違うタブは別のタブ", () => {
    const sameId: TabRef = { kind: "editor", id: "t1" };
    expect(enabled([T1, sameId], sameId)[2]).toEqual(["これより右を閉じる", false]);
    expect(enabled([T1], sameId).every(([, on]) => on === false)).toBe(true);
  });
});

describe("tabsToClose", () => {
  it("タブを閉じる: そのタブだけ", () => {
    expect(tabsToClose(ORDER, E1, "close")).toEqual({ terminalIds: [], editorIds: ["e1"] });
  });

  it("ほかのタブを閉じる: そのタブ以外を種類ごとに", () => {
    expect(tabsToClose(ORDER, T2, "close-others")).toEqual({ terminalIds: ["t1"], editorIds: ["e1", "e2"] });
  });

  it("これより右を閉じる: 見えている並びで右にあるもの(ターミナルの右はエディタも含む)", () => {
    expect(tabsToClose(ORDER, T1, "close-right")).toEqual({ terminalIds: ["t2"], editorIds: ["e1", "e2"] });
    expect(tabsToClose(ORDER, E1, "close-right")).toEqual({ terminalIds: [], editorIds: ["e2"] });
    expect(tabsToClose(ORDER, E2, "close-right")).toEqual({ terminalIds: [], editorIds: [] });
  });

  it("並びに無いタブなら何も閉じない", () => {
    expect(tabsToClose(ORDER, { kind: "terminal", id: "gone" }, "close-others")).toEqual({
      terminalIds: [],
      editorIds: [],
    });
  });
});
