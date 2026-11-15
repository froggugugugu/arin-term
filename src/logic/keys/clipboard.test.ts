import { describe, expect, it } from "vitest";

import { routeClipboard } from "./clipboard";

describe("routeClipboard(画面のコピー・切り取り・ペーストのイベントの振り分け)", () => {
  it("フォーカスがターミナルで選択があれば、コピーは選択のコピーになる", () => {
    expect(routeClipboard("copy", { kind: "terminal", hasSelection: true })).toBe("copy-selection");
  });

  it("フォーカスがターミナルで選択が無ければ、コピーは何もしない", () => {
    expect(routeClipboard("copy", { kind: "terminal", hasSelection: false })).toBe("none");
  });

  it("フォーカスがターミナルなら、ペーストはターミナルへの貼り付けになる(選択の有無によらない)", () => {
    expect(routeClipboard("paste", { kind: "terminal", hasSelection: false })).toBe("paste");
    expect(routeClipboard("paste", { kind: "terminal", hasSelection: true })).toBe("paste");
  });

  it("フォーカスがターミナルなら、切り取りは何もしない(ターミナルの文字は切り取れない)", () => {
    expect(routeClipboard("cut", { kind: "terminal", hasSelection: true })).toBe("none");
    expect(routeClipboard("cut", { kind: "terminal", hasSelection: false })).toBe("none");
  });

  it("フォーカスがターミナルでもツリーでもなければ、どれも振り分けない(画面の既定の動作に任せる)", () => {
    for (const kind of ["copy", "cut", "paste"] as const) {
      expect(routeClipboard(kind, { kind: "other" })).toBe("none");
      // 名前の入力欄では、入力欄の文字のコピー・切り取り・ペーストにする
      expect(routeClipboard(kind, { kind: "name-input" })).toBe("none");
    }
  });

  it("ツリーにフォーカスがあるとコピー_切り取り_ペーストはツリーの操作になる", () => {
    expect(routeClipboard("copy", { kind: "tree", selection: "entry" })).toBe("tree-copy");
    expect(routeClipboard("cut", { kind: "tree", selection: "entry" })).toBe("tree-cut");
    expect(routeClipboard("paste", { kind: "tree", selection: "entry" })).toBe("tree-paste");
  });

  it("ツリーでルートを選んでいると、切り取りは何もしない(FR-035)がコピーとペーストはできる", () => {
    expect(routeClipboard("cut", { kind: "tree", selection: "root" })).toBe("none");
    expect(routeClipboard("copy", { kind: "tree", selection: "root" })).toBe("tree-copy");
    expect(routeClipboard("paste", { kind: "tree", selection: "root" })).toBe("tree-paste");
  });

  it("ツリーで何も選んでいなければ、コピーと切り取りは何もせず、ペーストはルートへの貼り付けになる", () => {
    expect(routeClipboard("copy", { kind: "tree", selection: "none" })).toBe("none");
    expect(routeClipboard("cut", { kind: "tree", selection: "none" })).toBe("none");
    expect(routeClipboard("paste", { kind: "tree", selection: "none" })).toBe("tree-paste");
  });
});
