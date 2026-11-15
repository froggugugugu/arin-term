import { describe, expect, it } from "vitest";

import { computeRowWindow, revealRowOffset } from "./virtualWindow";

const ROW = 22;

describe("computeRowWindow(仮想スクロールの窓。ADR-007 の (c)、NFR-005)", () => {
  it("見える範囲は scrollTop と高さから決まり上下に余分を描く", () => {
    // 220px 下げて 110px の高さ: 10〜14 行目が見え、上下に 3 行ずつ足す
    expect(
      computeRowWindow({ scrollTop: 220, viewportHeight: 110, rowHeight: ROW, rowCount: 100, overscan: 3 }),
    ).toEqual({ start: 7, end: 18, paddingTop: 7 * ROW, paddingBottom: (100 - 18) * ROW });
  });

  it("行の途中で止まっていてもその行を描く", () => {
    expect(
      computeRowWindow({ scrollTop: 230, viewportHeight: 110, rowHeight: ROW, rowCount: 100, overscan: 0 }),
    ).toEqual({ start: 10, end: 16, paddingTop: 10 * ROW, paddingBottom: (100 - 16) * ROW });
  });

  it("10 万行でも計算は描く行の数だけで行の配列を作らない", () => {
    const rowCount = 100_000;
    const window = computeRowWindow({
      scrollTop: 50_000 * ROW,
      viewportHeight: 600,
      rowHeight: ROW,
      rowCount,
      overscan: 5,
    });

    expect(window.start).toBe(50_000 - 5);
    expect(window.end - window.start).toBeLessThanOrEqual(Math.ceil(600 / ROW) + 1 + 10);
    // 上下の余白と描く行の高さを足すと全体の高さになる
    expect(window.paddingTop + (window.end - window.start) * ROW + window.paddingBottom).toBe(rowCount * ROW);
  });

  it("端では範囲が行の数を超えない", () => {
    expect(
      computeRowWindow({ scrollTop: 0, viewportHeight: 110, rowHeight: ROW, rowCount: 100, overscan: 3 }),
    ).toEqual({ start: 0, end: 8, paddingTop: 0, paddingBottom: 92 * ROW });

    expect(
      computeRowWindow({ scrollTop: 100 * ROW - 110, viewportHeight: 110, rowHeight: ROW, rowCount: 100, overscan: 3 }),
    ).toMatchObject({ end: 100, paddingBottom: 0 });

    // 行が見える高さより少ない
    expect(
      computeRowWindow({ scrollTop: 0, viewportHeight: 600, rowHeight: ROW, rowCount: 4, overscan: 3 }),
    ).toEqual({ start: 0, end: 4, paddingTop: 0, paddingBottom: 0 });
  });

  it("中身より下まで scrollTop が進んでいても最後の画面を描く(行が減った直後)", () => {
    const window = computeRowWindow({ scrollTop: 10_000, viewportHeight: 110, rowHeight: ROW, rowCount: 20, overscan: 0 });

    expect(window).toEqual({ start: 15, end: 20, paddingTop: 15 * ROW, paddingBottom: 0 });
  });

  it("行が無いときと寸法が不正なときは何も描かない", () => {
    const empty = { start: 0, end: 0, paddingTop: 0, paddingBottom: 0 };

    expect(computeRowWindow({ scrollTop: 0, viewportHeight: 600, rowHeight: ROW, rowCount: 0, overscan: 3 })).toEqual(
      empty,
    );
    expect(computeRowWindow({ scrollTop: 0, viewportHeight: 600, rowHeight: 0, rowCount: 10, overscan: 3 })).toEqual(
      empty,
    );
    expect(
      computeRowWindow({ scrollTop: 0, viewportHeight: 600, rowHeight: Number.NaN, rowCount: 10, overscan: 3 }),
    ).toEqual(empty);
  });

  it("数でない値と負の値の scrollTop 高さ 余分は 0 とみなす", () => {
    expect(
      computeRowWindow({
        scrollTop: Number.NaN,
        viewportHeight: -5,
        rowHeight: ROW,
        rowCount: 10,
        overscan: Number.POSITIVE_INFINITY,
      }),
    ).toEqual({ start: 0, end: 0, paddingTop: 0, paddingBottom: 10 * ROW });
    expect(
      computeRowWindow({ scrollTop: -100, viewportHeight: 44, rowHeight: ROW, rowCount: 10, overscan: -2 }),
    ).toEqual({ start: 0, end: 2, paddingTop: 0, paddingBottom: 8 * ROW });
  });
});

describe("revealRowOffset(選択した行を見える位置へ動かす量)", () => {
  const view = { scrollTop: 220, viewportHeight: 110, rowHeight: ROW };

  it("選択した行が見えるように動かす量", () => {
    // 見えている(10〜14 行目)なら動かさない
    expect(revealRowOffset({ ...view, index: 12 })).toBe(0);
    // 上に隠れている行は、その行の上端まで上げる
    expect(revealRowOffset({ ...view, index: 5 })).toBe(5 * ROW - 220);
    // 下に隠れている行は、その行の下端まで下げる
    expect(revealRowOffset({ ...view, index: 20 })).toBe(21 * ROW - (220 + 110));
    // 半分だけ見えている行も全体が見えるまで動かす
    expect(revealRowOffset({ scrollTop: 230, viewportHeight: 110, rowHeight: ROW, index: 10 })).toBe(-10);
  });

  it("見える高さが 1 行より低いときは行の上端に合わせる", () => {
    expect(revealRowOffset({ scrollTop: 0, viewportHeight: 10, rowHeight: ROW, index: 3 })).toBe(3 * ROW);
  });

  it("不正な行と寸法では動かさない", () => {
    expect(revealRowOffset({ ...view, index: -1 })).toBe(0);
    expect(revealRowOffset({ ...view, index: Number.NaN })).toBe(0);
    expect(revealRowOffset({ ...view, rowHeight: 0, index: 3 })).toBe(0);
  });
});
