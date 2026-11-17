// ペインの寸法(FR-009、設計書 §6.2 の `ui`)。DOM を使わない純粋な関数。単位は CSS ピクセル。
//
// 保存値(`PaneSizes`。Rust の写しの `layout`)と表示値(`PaneLayout`)を分ける。
// ウィンドウが狭いときは表示値だけを縮め、保存値は変えない(広げ直すと元の寸法に戻る)。
// 保存値の範囲の検査(数でない値・負の値)は Rust 側(計画 T30b)が行う。ここでは有限の数を受け取る前提。

/** カード領域(左ペインの上段)の最小の高さ(計画 M2 の【仮定】10)。 */
export const MIN_CARD_AREA_HEIGHT = 72;
/** ツリー領域(左ペインの下段)の最小の高さ。 */
export const MIN_TREE_AREA_HEIGHT = 120;
/** 左ペインの最小の幅。 */
export const MIN_SIDEBAR_WIDTH = 180;
/** 右ペイン(ターミナル領域)の最小の幅。 */
export const MIN_TERMINAL_WIDTH = 320;

/** 保存するペインの寸法。 */
export interface PaneSizes {
  readonly sidebarWidth: number;
  readonly sidebarCollapsed: boolean;
  readonly cardAreaHeight: number;
}

/** 初期値。 */
export const DEFAULT_PANE_SIZES: PaneSizes = {
  sidebarWidth: 260,
  sidebarCollapsed: false,
  cardAreaHeight: 200,
};

/** 分ける領域の大きさ。`width` はウィンドウの幅、`height` は左ペインの上段と下段を合わせた高さ。 */
export interface PaneArea {
  readonly width: number;
  readonly height: number;
}

/** 表示する寸法。折りたたみ中の `sidebarWidth` は 0。 */
export interface PaneLayout {
  readonly sidebarWidth: number;
  readonly terminalWidth: number;
  readonly cardAreaHeight: number;
  readonly treeAreaHeight: number;
}

/**
 * `total` を 2 つに分けるときの、手前の側の大きさ。手前の最小と、奥の最小を残す上限のあいだに留める。
 * 両方の最小が入らないほど `total` が小さいときは手前の最小(奥は呼び出し側が最小にする)。
 */
function clampLeading(value: number, total: number, leadingMin: number, trailingMin: number): number {
  const max = Math.max(leadingMin, total - trailingMin);
  return Math.min(Math.max(value, leadingMin), max);
}

/** ウィンドウの大きさと保存値から、表示する寸法を決める。どのペインも最小寸法を下回らない。 */
export function computePaneLayout(area: PaneArea, sizes: PaneSizes): PaneLayout {
  const sidebarWidth = sizes.sidebarCollapsed
    ? 0
    : clampLeading(sizes.sidebarWidth, area.width, MIN_SIDEBAR_WIDTH, MIN_TERMINAL_WIDTH);
  const cardAreaHeight = clampLeading(
    sizes.cardAreaHeight,
    area.height,
    MIN_CARD_AREA_HEIGHT,
    MIN_TREE_AREA_HEIGHT,
  );
  return {
    sidebarWidth,
    terminalWidth: Math.max(MIN_TERMINAL_WIDTH, area.width - sidebarWidth),
    cardAreaHeight,
    treeAreaHeight: Math.max(MIN_TREE_AREA_HEIGHT, area.height - cardAreaHeight),
  };
}

/** 境界のドラッグの始まり。`startSize` は始めたときの表示値(`computePaneLayout` の結果)。 */
export interface DividerDrag {
  readonly startPointer: number;
  readonly startSize: number;
}

/** 境界のドラッグを始める。`startPointer` は押したときのポインタの位置(左右なら x、上下なら y)。 */
export function beginDividerDrag(startPointer: number, startSize: number): DividerDrag {
  return { startPointer, startSize };
}

/** ドラッグで動かした後の大きさ。ポインタの位置は小数になりうるので、整数のピクセルに四捨五入する。 */
function draggedSize(drag: DividerDrag, pointer: number): number {
  return Math.round(drag.startSize + pointer - drag.startPointer);
}

/** 左右の境界のドラッグ中の、左ペインの幅。左ペインの最小と右ペインの最小のあいだで止まる。 */
export function sidebarWidthDuringDrag(drag: DividerDrag, pointerX: number, windowWidth: number): number {
  return clampLeading(draggedSize(drag, pointerX), windowWidth, MIN_SIDEBAR_WIDTH, MIN_TERMINAL_WIDTH);
}

/** 上下の境界のドラッグ中の、カード領域の高さ。カード領域の最小とツリー領域の最小のあいだで止まる。 */
export function cardAreaHeightDuringDrag(
  drag: DividerDrag,
  pointerY: number,
  leftPaneHeight: number,
): number {
  return clampLeading(
    draggedSize(drag, pointerY),
    leftPaneHeight,
    MIN_CARD_AREA_HEIGHT,
    MIN_TREE_AREA_HEIGHT,
  );
}
