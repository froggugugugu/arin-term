import { describe, expect, it } from "vitest";

import { menuKeyAction, moveMenuFocus } from "./menuNavigation";

describe("moveMenuFocus(メニューの矢印キーでの移動)", () => {
  it("下の矢印で次の項目へ、最後からは最初へ回り込む", () => {
    expect(moveMenuFocus(0, 3, "ArrowDown")).toBe(1);
    expect(moveMenuFocus(2, 3, "ArrowDown")).toBe(0);
  });

  it("上の矢印で前の項目へ、最初からは最後へ回り込む", () => {
    expect(moveMenuFocus(1, 3, "ArrowUp")).toBe(0);
    expect(moveMenuFocus(0, 3, "ArrowUp")).toBe(2);
  });

  it("Home で最初、End で最後", () => {
    expect(moveMenuFocus(1, 4, "Home")).toBe(0);
    expect(moveMenuFocus(1, 4, "End")).toBe(3);
  });

  it("どこにもフォーカスが無いとき、下の矢印は最初、上の矢印は最後", () => {
    expect(moveMenuFocus(-1, 3, "ArrowDown")).toBe(0);
    expect(moveMenuFocus(-1, 3, "ArrowUp")).toBe(2);
  });

  it("移動のキーでなければ null、項目が 0 個でも null", () => {
    expect(moveMenuFocus(0, 3, "a")).toBeNull();
    expect(moveMenuFocus(0, 0, "ArrowDown")).toBeNull();
  });
});

describe("menuKeyAction(メニューのキーの意味)", () => {
  it("Enter と Space は選ぶ", () => {
    expect(menuKeyAction("Enter", { inSubmenu: false })).toBe("activate");
    expect(menuKeyAction(" ", { inSubmenu: true })).toBe("activate");
  });

  it("右の矢印は下の階層を開く(下の階層の中では何もしない)", () => {
    expect(menuKeyAction("ArrowRight", { inSubmenu: false })).toBe("open-submenu");
    expect(menuKeyAction("ArrowRight", { inSubmenu: true })).toBe("none");
  });

  it("左の矢印は下の階層を閉じる(上の階層では何もしない)", () => {
    expect(menuKeyAction("ArrowLeft", { inSubmenu: true })).toBe("close-submenu");
    expect(menuKeyAction("ArrowLeft", { inSubmenu: false })).toBe("none");
  });

  it("Esc は下の階層の中ならそれだけを閉じ、上の階層ならメニューを閉じる", () => {
    expect(menuKeyAction("Escape", { inSubmenu: true })).toBe("close-submenu");
    expect(menuKeyAction("Escape", { inSubmenu: false })).toBe("close");
  });

  it("Tab はメニューを閉じる", () => {
    expect(menuKeyAction("Tab", { inSubmenu: false })).toBe("close");
    expect(menuKeyAction("Tab", { inSubmenu: true })).toBe("close");
  });

  it("それ以外のキーは何もしない", () => {
    expect(menuKeyAction("x", { inSubmenu: false })).toBe("none");
  });
});
