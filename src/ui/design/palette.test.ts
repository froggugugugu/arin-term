import { describe, expect, it } from "vitest";

import { PROJECT_COLOR_IDS } from "../../logic/menu/cardMenu";
import { contrastRatio, PROJECT_PALETTE, projectColorStyle } from "./palette";

const HEX = /^#[0-9a-f]{6}$/;

describe("PROJECT_PALETTE(プロジェクト色の値)", () => {
  it("8 つの識別子のすべてにライトとダークの値がある", () => {
    expect(Object.keys(PROJECT_PALETTE)).toEqual([...PROJECT_COLOR_IDS]);
    for (const id of PROJECT_COLOR_IDS) {
      const { light, dark } = PROJECT_PALETTE[id];
      for (const value of [light.tint, light.ink, light.accent, dark.tint, dark.ink, dark.accent]) {
        expect(value).toMatch(HEX);
      }
    }
  });

  it("着色した地の上の文字は、ライトでもダークでもコントラスト比 4.5 以上(FR-062)", () => {
    for (const id of PROJECT_COLOR_IDS) {
      for (const mode of ["light", "dark"] as const) {
        const { tint, ink } = PROJECT_PALETTE[id][mode];
        expect(contrastRatio(ink, tint), `${id} ${mode}`).toBeGreaterThanOrEqual(4.5);
      }
    }
  });

  it("目印の色(accent)は着色した地に対してコントラスト比 3 以上", () => {
    for (const id of PROJECT_COLOR_IDS) {
      for (const mode of ["light", "dark"] as const) {
        const { tint, accent } = PROJECT_PALETTE[id][mode];
        expect(contrastRatio(accent, tint), `${id} ${mode}`).toBeGreaterThanOrEqual(3);
      }
    }
  });

  it("同じモードの地の色は 8 色ともすべて違う", () => {
    for (const mode of ["light", "dark"] as const) {
      const tints = PROJECT_COLOR_IDS.map((id) => PROJECT_PALETTE[id][mode].tint);
      expect(new Set(tints).size).toBe(8);
    }
  });
});

describe("contrastRatio(WCAG のコントラスト比)", () => {
  it("白と黒は 21、同じ色は 1", () => {
    expect(contrastRatio("#ffffff", "#000000")).toBeCloseTo(21, 5);
    expect(contrastRatio("#777777", "#777777")).toBeCloseTo(1, 5);
  });
});

describe("projectColorStyle(部品に渡す CSS の変数)", () => {
  it("ライトとダークの値を CSS の変数にして返す", () => {
    const green = PROJECT_PALETTE.green;
    expect(projectColorStyle("green")).toEqual({
      "--project-tint-light": green.light.tint,
      "--project-ink-light": green.light.ink,
      "--project-accent-light": green.light.accent,
      "--project-tint-dark": green.dark.tint,
      "--project-ink-dark": green.dark.ink,
      "--project-accent-dark": green.dark.accent,
    });
  });
});
