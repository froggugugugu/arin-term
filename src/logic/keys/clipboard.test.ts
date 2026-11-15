import { describe, expect, it } from "vitest";

import { routeClipboard } from "./clipboard";

describe("routeClipboard(画面のコピー・ペーストのイベントの振り分け)", () => {
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

  it("フォーカスがターミナルでなければ、どちらも振り分けない(画面の既定の動作に任せる)", () => {
    expect(routeClipboard("copy", { kind: "other" })).toBe("none");
    expect(routeClipboard("paste", { kind: "other" })).toBe("none");
  });
});
