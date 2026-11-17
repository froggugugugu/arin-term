import { describe, expect, it } from "vitest";

import {
  beginDividerDrag,
  cardAreaHeightDuringDrag,
  computePaneLayout,
  DEFAULT_PANE_SIZES,
  MIN_CARD_AREA_HEIGHT,
  MIN_SIDEBAR_WIDTH,
  MIN_TERMINAL_WIDTH,
  MIN_TREE_AREA_HEIGHT,
  type PaneSizes,
  sidebarWidthDuringDrag,
} from "./paneLayout";

const sizes = (overrides: Partial<PaneSizes> = {}): PaneSizes => ({
  ...DEFAULT_PANE_SIZES,
  ...overrides,
});

describe("寸法の定数(【仮定】10)", () => {
  it("最小寸法と初期値", () => {
    expect(MIN_CARD_AREA_HEIGHT).toBe(72);
    expect(MIN_TREE_AREA_HEIGHT).toBe(120);
    expect(MIN_SIDEBAR_WIDTH).toBe(180);
    expect(MIN_TERMINAL_WIDTH).toBe(320);
    expect(DEFAULT_PANE_SIZES).toEqual({
      sidebarWidth: 260,
      sidebarCollapsed: false,
      cardAreaHeight: 200,
    });
  });
});

describe("computePaneLayout(ウィンドウの大きさと保存値から表示の寸法を決める)", () => {
  it("十分に広いウィンドウでは保存値のまま", () => {
    expect(computePaneLayout({ width: 1280, height: 800 }, sizes())).toEqual({
      sidebarWidth: 260,
      terminalWidth: 1020,
      cardAreaHeight: 200,
      treeAreaHeight: 600,
    });
  });

  it("保存値が最小を下回っていても最小で表示する", () => {
    expect(
      computePaneLayout({ width: 1280, height: 800 }, sizes({ sidebarWidth: 10, cardAreaHeight: 0 })),
    ).toEqual({ sidebarWidth: 180, terminalWidth: 1100, cardAreaHeight: 72, treeAreaHeight: 728 });
  });

  it("右ペインとツリー領域の最小を残すように左ペインとカード領域を縮める", () => {
    expect(
      computePaneLayout({ width: 600, height: 300 }, sizes({ sidebarWidth: 500, cardAreaHeight: 250 })),
    ).toEqual({ sidebarWidth: 280, terminalWidth: 320, cardAreaHeight: 180, treeAreaHeight: 120 });
  });

  it("ウィンドウが狭くても各ペインが最小寸法を下回らない", () => {
    const layout = computePaneLayout({ width: 300, height: 100 }, sizes());

    expect(layout).toEqual({
      sidebarWidth: MIN_SIDEBAR_WIDTH,
      terminalWidth: MIN_TERMINAL_WIDTH,
      cardAreaHeight: MIN_CARD_AREA_HEIGHT,
      treeAreaHeight: MIN_TREE_AREA_HEIGHT,
    });
  });

  it("ウィンドウを縮めても保存値は変えず、広げ直すと元の寸法に戻る", () => {
    const saved = sizes({ sidebarWidth: 400, cardAreaHeight: 300 });
    computePaneLayout({ width: 500, height: 250 }, saved);

    expect(saved).toEqual({ sidebarWidth: 400, sidebarCollapsed: false, cardAreaHeight: 300 });
    expect(computePaneLayout({ width: 1280, height: 800 }, saved)).toMatchObject({
      sidebarWidth: 400,
      cardAreaHeight: 300,
    });
  });

  it("折りたたむと右ペインが全幅になり 戻すと前の幅", () => {
    const window = { width: 1280, height: 800 };
    const saved = sizes({ sidebarWidth: 340 });

    expect(computePaneLayout(window, { ...saved, sidebarCollapsed: true })).toMatchObject({
      sidebarWidth: 0,
      terminalWidth: 1280,
    });
    expect(computePaneLayout(window, saved)).toMatchObject({
      sidebarWidth: 340,
      terminalWidth: 940,
    });
  });

  it("折りたたんでいてもウィンドウが狭ければ右ペインは最小の幅", () => {
    expect(
      computePaneLayout({ width: 200, height: 800 }, sizes({ sidebarCollapsed: true })),
    ).toMatchObject({ sidebarWidth: 0, terminalWidth: MIN_TERMINAL_WIDTH });
  });
});

describe("境界のドラッグ中の位置", () => {
  it("左右の境界を動かした分だけ左ペインの幅が変わる(小数は四捨五入)", () => {
    const drag = beginDividerDrag(100.5, 260);

    expect(sidebarWidthDuringDrag(drag, 150.5, 1280)).toBe(310);
    expect(sidebarWidthDuringDrag(drag, 60.2, 1280)).toBe(220);
  });

  it("左ペインの幅は最小と右ペインの最小で止まる", () => {
    const drag = beginDividerDrag(260, 260);

    expect(sidebarWidthDuringDrag(drag, 0, 1280)).toBe(MIN_SIDEBAR_WIDTH);
    expect(sidebarWidthDuringDrag(drag, 2000, 1280)).toBe(1280 - MIN_TERMINAL_WIDTH);
  });

  it("カード領域を広げるとツリー領域が縮み最小で止まる", () => {
    const height = 800;
    const drag = beginDividerDrag(200, 200);

    const wider = cardAreaHeightDuringDrag(drag, 300, height);
    expect(wider).toBe(300);
    expect(computePaneLayout({ width: 1280, height }, sizes({ cardAreaHeight: wider }))).toMatchObject(
      { cardAreaHeight: 300, treeAreaHeight: 500 },
    );

    const widest = cardAreaHeightDuringDrag(drag, 5000, height);
    expect(widest).toBe(height - MIN_TREE_AREA_HEIGHT);
    expect(
      computePaneLayout({ width: 1280, height }, sizes({ cardAreaHeight: widest })),
    ).toMatchObject({ treeAreaHeight: MIN_TREE_AREA_HEIGHT });
  });

  it("カード領域を縮めると最小で止まる", () => {
    const drag = beginDividerDrag(200, 200);

    expect(cardAreaHeightDuringDrag(drag, -100, 800)).toBe(MIN_CARD_AREA_HEIGHT);
  });

  it("領域が最小の合計より狭いときは、動かした側の最小で止まる", () => {
    expect(sidebarWidthDuringDrag(beginDividerDrag(0, 180), 100, 400)).toBe(MIN_SIDEBAR_WIDTH);
    expect(cardAreaHeightDuringDrag(beginDividerDrag(0, 72), 50, 150)).toBe(MIN_CARD_AREA_HEIGHT);
  });
});
