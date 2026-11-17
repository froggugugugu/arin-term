import { describe, expect, it } from "vitest";

import { translucencyTokens } from "../../logic/appearance/translucency";
import { applyTranslucency } from "./applyTranslucency";

describe("applyTranslucency(透過の不透明度を_:root_に当てる。ADR-014 の 2)", () => {
  it("3_つの不透明度を数の文字列で書く(単位なし)", () => {
    const written = new Map<string, string>();
    const root = { style: { setProperty: (name: string, value: string) => void written.set(name, value) } };

    applyTranslucency(root, translucencyTokens(60, 0));

    expect(Object.fromEntries(written)).toEqual({
      "--window-bg-alpha-light": "0.4",
      "--window-bg-alpha-dark": "0.35",
      "--terminal-bg-alpha": "1",
    });
  });
});
