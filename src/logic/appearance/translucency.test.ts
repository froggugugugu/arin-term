import { describe, expect, it } from "vitest";

import {
  TRANSLUCENCY_DEFAULTS,
  TRANSLUCENCY_TOKEN_DEFAULTS,
  type TranslucencyToken,
  translucencyTokens,
} from "./translucency";

const TOKENS = Object.keys(TRANSLUCENCY_TOKEN_DEFAULTS) as TranslucencyToken[];

/** tokens.css の中身(ファイル全体)。 */
async function readTokensCssText(): Promise<string> {
  // src/logic は node の型を持たない(画面と同じ型の環境)ので、テストの中だけ node の部品を型なしで読み込み、使う形だけを書く。
  const load = (name: string) => import(/* @vite-ignore */ name);
  const fs = (await load("node:fs")) as { readFileSync(path: string, encoding: "utf8"): string };
  const path = (await load("node:path")) as { resolve(...parts: string[]): string; dirname(p: string): string };
  const { fileURLToPath } = (await load("node:url")) as { fileURLToPath(url: string): string };
  const here = path.dirname(fileURLToPath((import.meta as unknown as { url: string }).url));
  return fs.readFileSync(path.resolve(here, "../../ui/design/tokens.css"), "utf8");
}

/** css の最初の `selector {` の塊から `--名前: 値;` を読む。 */
function declarations(css: string, from: number): Map<string, string> {
  const start = css.indexOf(":root {", from);
  const block = css.slice(start, css.indexOf("\n}", start));
  const values = new Map<string, string>();
  for (const match of block.matchAll(/(--[a-z0-9-]+):\s*([^;]+);/g)) {
    const [, name, value] = match;
    if (name !== undefined && value !== undefined) values.set(name, value.trim());
  }
  return values;
}

describe("translucencyTokens(透過の値から地の不透明度を決める。ADR-014 の 2)", () => {
  it("既定(ウィンドウ_45・ターミナル_20)では_M4_6_までの見た目と_tokens_css_の値と同じ", async () => {
    const css = await readTokensCssText();
    const light = declarations(css, 0);
    const tokens = translucencyTokens(TRANSLUCENCY_DEFAULTS.window, TRANSLUCENCY_DEFAULTS.terminal);

    expect(TRANSLUCENCY_DEFAULTS).toEqual({ window: 45, terminal: 20 });
    // M4.6 までのウィンドウの地(ライト 0.55・ダーク 0.5)と同じ
    expect(tokens).toEqual({
      "--window-bg-alpha-light": 0.55,
      "--window-bg-alpha-dark": 0.5,
      "--terminal-bg-alpha": 0.8,
    });
    for (const token of TOKENS) {
      expect(light.get(token), token).toBe(String(TRANSLUCENCY_TOKEN_DEFAULTS[token]));
      expect(tokens[token], token).toBe(TRANSLUCENCY_TOKEN_DEFAULTS[token]);
    }
  });

  it("tokens_css_の色は不透明度の変数で受け_RGB_は_M4_6_のまま", async () => {
    const css = await readTokensCssText();
    const light = declarations(css, 0);
    const dark = declarations(css, css.indexOf("@media (prefers-color-scheme: dark)"));

    expect(light.get("--color-window-bg")).toBe("rgba(238, 238, 240, var(--window-bg-alpha-light))");
    expect(dark.get("--color-window-bg")).toBe("rgba(30, 30, 32, var(--window-bg-alpha-dark))");
    expect(light.get("--color-terminal-bg")).toBe("rgba(0, 0, 0, var(--terminal-bg-alpha))");
    // 不透明度の変数はライトの :root にだけ置く(外観で値を替えるのは色の側)
    for (const token of TOKENS) expect(dark.has(token), token).toBe(false);
  });

  it("ライトは_1−t÷100_ダークは_1−t÷100−0_05_で_0_に留める", () => {
    expect(translucencyTokens(0, 0)).toEqual({
      "--window-bg-alpha-light": 1,
      "--window-bg-alpha-dark": 0.95,
      "--terminal-bg-alpha": 1,
    });
    expect(translucencyTokens(100, 80)).toEqual({
      "--window-bg-alpha-light": 0,
      "--window-bg-alpha-dark": 0,
      "--terminal-bg-alpha": 0.2,
    });
    expect(translucencyTokens(95, 5)).toEqual({
      "--window-bg-alpha-light": 0.05,
      "--window-bg-alpha-dark": 0,
      "--terminal-bg-alpha": 0.95,
    });
  });

  it("刻みの値はどれも小数_2_桁ちょうど(浮動小数の端数を出さない)", () => {
    for (let t = 0; t <= 100; t += 5) {
      const tokens = translucencyTokens(t, Math.min(t, 80));
      for (const token of TOKENS) {
        const value = tokens[token];
        expect(Math.round(value * 100) / 100, `${token} ${t}`).toBe(value);
        expect(value).toBeGreaterThanOrEqual(0);
        expect(value).toBeLessThanOrEqual(1);
      }
    }
    expect(translucencyTokens(35, 35)["--window-bg-alpha-light"]).toBe(0.65);
    expect(translucencyTokens(35, 35)["--window-bg-alpha-dark"]).toBe(0.6);
  });

  it("数でない値は既定として扱い_範囲の外は_0〜100_に留める(範囲の規則は_Rust_が持つ)", () => {
    expect(translucencyTokens(Number.NaN, Number.NaN)).toEqual(
      translucencyTokens(TRANSLUCENCY_DEFAULTS.window, TRANSLUCENCY_DEFAULTS.terminal),
    );
    expect(translucencyTokens(-10, 150)).toEqual({
      "--window-bg-alpha-light": 1,
      "--window-bg-alpha-dark": 0.95,
      "--terminal-bg-alpha": 0,
    });
  });
});
