// ツリーの仮想スクロールの窓(ADR-007 の (c)、NFR-005、計画 T41a)。DOM を使わない純粋な関数。単位は CSS ピクセル。
//
// 行の高さを 1 つに固定するので、描く範囲は割り算だけで決まる。計算は行の数(数)だけを受け取り、
// 行の配列を作らない(10 万行でも描く行の数に比例する)。行の高さの値は画面(`src/ui/design/tokens.css`。
// 【仮定】5 で 22px)が渡す。

/** 窓の計算に要る値。 */
export interface RowWindowInput {
  /** スクロールの位置(上端からの距離)。 */
  readonly scrollTop: number;
  /** 見える高さ。 */
  readonly viewportHeight: number;
  /** 1 行の高さ。 */
  readonly rowHeight: number;
  /** 全体の行の数。 */
  readonly rowCount: number;
  /** 見える範囲の上下に余分に描く行の数。 */
  readonly overscan: number;
}

/** 描く範囲。`start` から `end` の手前までの行を描き、上下に余白を置く。 */
export interface RowWindow {
  readonly start: number;
  readonly end: number;
  readonly paddingTop: number;
  readonly paddingBottom: number;
}

const EMPTY_WINDOW: RowWindow = { start: 0, end: 0, paddingTop: 0, paddingBottom: 0 };

/** 有限で 0 以上の数ならそのまま、そうでなければ 0。 */
function nonNegative(value: number): number {
  return Number.isFinite(value) && value > 0 ? value : 0;
}

/** スクロールの位置と見える高さから、描く行の範囲と上下の余白を決める。 */
export function computeRowWindow(input: RowWindowInput): RowWindow {
  const rowHeight = nonNegative(input.rowHeight);
  const rowCount = Math.floor(nonNegative(input.rowCount));
  if (rowHeight === 0 || rowCount === 0) {
    return EMPTY_WINDOW;
  }
  const viewportHeight = nonNegative(input.viewportHeight);
  const overscan = Math.floor(nonNegative(input.overscan));
  // 行が減った直後など、中身より下まで進んだ位置は最後の画面に留める
  const maxScrollTop = Math.max(0, rowCount * rowHeight - viewportHeight);
  const scrollTop = Math.min(nonNegative(input.scrollTop), maxScrollTop);

  const firstVisible = Math.floor(scrollTop / rowHeight);
  const endVisible = Math.ceil((scrollTop + viewportHeight) / rowHeight);
  const start = Math.max(0, Math.min(rowCount, firstVisible - overscan));
  const end = Math.max(start, Math.min(rowCount, endVisible + overscan));
  return { start, end, paddingTop: start * rowHeight, paddingBottom: (rowCount - end) * rowHeight };
}

/** 行を見える位置へ動かす計算に要る値。 */
export interface RevealInput {
  readonly scrollTop: number;
  readonly viewportHeight: number;
  readonly rowHeight: number;
  /** 見せたい行の番号(0 始まり)。 */
  readonly index: number;
}

/**
 * 行の全体が見えるように `scrollTop` を動かす量(正なら下へ、負なら上へ、見えていれば 0)。
 * 見える高さが 1 行より低いときは行の上端に合わせる。不正な値では動かさない。
 */
export function revealRowOffset(input: RevealInput): number {
  const rowHeight = nonNegative(input.rowHeight);
  if (rowHeight === 0 || !Number.isInteger(input.index) || input.index < 0) {
    return 0;
  }
  const scrollTop = nonNegative(input.scrollTop);
  const viewportHeight = nonNegative(input.viewportHeight);
  const rowTop = input.index * rowHeight;
  const rowBottom = rowTop + rowHeight;
  if (rowTop < scrollTop || viewportHeight < rowHeight) {
    return rowTop - scrollTop;
  }
  if (rowBottom > scrollTop + viewportHeight) {
    return rowBottom - (scrollTop + viewportHeight);
  }
  return 0;
}
