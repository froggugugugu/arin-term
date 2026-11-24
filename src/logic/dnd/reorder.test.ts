import { describe, expect, it } from "vitest";

import { reorderIndex, shiftOf } from "./reorder";

// 幅 100 の項目が 4 つ並ぶ(中心は 50・150・250・350)。
const CENTERS = [50, 150, 250, 350];

describe("reorderIndex", () => {
  it("掴んだ項目を除いて、中心がポインタより手前の数が動かしたあとの位置", () => {
    expect(reorderIndex(CENTERS, 0, 10)).toBe(0);
    expect(reorderIndex(CENTERS, 0, 160)).toBe(1);
    expect(reorderIndex(CENTERS, 0, 999)).toBe(3);
    expect(reorderIndex(CENTERS, 3, 0)).toBe(0);
    expect(reorderIndex(CENTERS, 3, 200)).toBe(2);
  });

  it("中心をまだ越えていなければ元の位置のまま", () => {
    expect(reorderIndex(CENTERS, 1, 140)).toBe(1);
    expect(reorderIndex(CENTERS, 1, 240)).toBe(1);
  });

  it("項目が 1 つなら常に 0", () => {
    expect(reorderIndex([50], 0, 500)).toBe(0);
  });
});

describe("shiftOf", () => {
  const shifts = (from: number, to: number) => [0, 1, 2, 3].map((i) => shiftOf(i, from, to));

  it("後ろへ動かすと、あいだの項目を前へずらす", () => {
    expect(shifts(0, 2)).toEqual([0, -1, -1, 0]);
    expect(shifts(1, 3)).toEqual([0, 0, -1, -1]);
  });

  it("前へ動かすと、あいだの項目を後ろへずらす", () => {
    expect(shifts(3, 0)).toEqual([1, 1, 1, 0]);
    expect(shifts(2, 1)).toEqual([0, 1, 0, 0]);
  });

  it("動かないときは何もずらさない", () => {
    expect(shifts(2, 2)).toEqual([0, 0, 0, 0]);
  });
});
