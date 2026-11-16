import { describe, expect, it } from "vitest";

import { INTERFACE_BASE_SIZE, INTERFACE_TOKEN_DEFAULTS, interfaceMetrics, type InterfaceToken } from "./interfaceMetrics";

/** 画面の大きさの範囲(Rust の `layout/font_sizes.rs` の 11〜18。ADR-013 の 1)。 */
const SIZES = [11, 12, 13, 14, 15, 16, 17, 18] as const;

const TOKENS = Object.keys(INTERFACE_TOKEN_DEFAULTS) as InterfaceToken[];

/** tokens.css の :root(ライトの既定。最初の :root の塊)から `--名前: 数px;` を読む。 */
async function readTokensCss(): Promise<Map<string, string>> {
  // src/logic は node の型を持たない(画面と同じ型の環境)ので、テストの中だけ node の部品を型なしで読み込み、使う形だけを書く。
  const load = (name: string) => import(/* @vite-ignore */ name);
  const fs = (await load("node:fs")) as { readFileSync(path: string, encoding: "utf8"): string };
  const path = (await load("node:path")) as { resolve(...parts: string[]): string; dirname(p: string): string };
  const { fileURLToPath } = (await load("node:url")) as { fileURLToPath(url: string): string };
  const here = path.dirname(fileURLToPath((import.meta as unknown as { url: string }).url));
  const css = fs.readFileSync(path.resolve(here, "../../ui/design/tokens.css"), "utf8");
  const start = css.indexOf(":root {");
  const block = css.slice(start, css.indexOf("\n}", start));
  const values = new Map<string, string>();
  for (const match of block.matchAll(/(--[a-z0-9-]+):\s*([^;]+);/g)) {
    const [, name, value] = match;
    if (name !== undefined && value !== undefined) values.set(name, value.trim());
  }
  return values;
}

describe("interfaceMetrics(画面の大きさから文字と行の高さのトークンを決める。ADR-013 の 4)", () => {
  it("既定の_13_では_tokens_css_の値と同じ", async () => {
    const css = await readTokensCss();
    const metrics = interfaceMetrics(INTERFACE_BASE_SIZE);

    expect(INTERFACE_BASE_SIZE).toBe(13);
    for (const token of TOKENS) {
      expect(css.get(token), token).toBe(`${INTERFACE_TOKEN_DEFAULTS[token]}px`);
      expect(metrics.tokens[token], token).toBe(INTERFACE_TOKEN_DEFAULTS[token]);
    }
    expect(metrics.treeRowHeight).toBe(22);
  });

  it("どの大きさでも整数の_px", () => {
    for (const size of SIZES) {
      const metrics = interfaceMetrics(size);
      for (const token of TOKENS) {
        expect(Number.isInteger(metrics.tokens[token]), `${size} ${token}`).toBe(true);
        expect(metrics.tokens[token], `${size} ${token}`).toBeGreaterThan(0);
      }
      expect(metrics.treeRowHeight).toBe(metrics.tokens["--tree-row-height"]);
    }
  });

  it.each(SIZES.slice(1).map((size, i) => [SIZES[i] as number, size]))(
    "大きさに比例し単調に増える(%i → %i)",
    (smaller, larger) => {
      const a = interfaceMetrics(smaller);
      const b = interfaceMetrics(larger);
      for (const token of TOKENS) {
        const base = INTERFACE_TOKEN_DEFAULTS[token];
        // 既定の値 × 大きさ / 13 を四捨五入した値(比例。ずれは 0.5px まで)。
        expect(Math.abs(b.tokens[token] - (base * larger) / 13), `${larger} ${token}`).toBeLessThanOrEqual(0.5);
        expect(b.tokens[token], `${smaller}→${larger} ${token}`).toBeGreaterThanOrEqual(a.tokens[token]);
      }
      // 基準の文字(--font-size-l)は大きさそのもの。
      expect(b.tokens["--font-size-l"]).toBe(larger);
    },
  );

  it("範囲の端どうしでは、どのトークンも大きくなる", () => {
    const small = interfaceMetrics(11);
    const large = interfaceMetrics(18);
    for (const token of TOKENS) expect(large.tokens[token], token).toBeGreaterThan(small.tokens[token]);
  });

  it("追従させないトークンは返さない", () => {
    expect([...TOKENS].sort()).toEqual(
      [
        "--font-size-s",
        "--font-size-m",
        "--font-size-l",
        "--font-size-xl",
        "--header-height",
        "--tab-bar-height",
        "--tile-title-height",
        "--sidebar-item-height",
        "--sidebar-section-height",
        "--button-height",
        "--tree-row-height",
        "--tree-toolbar-height",
        "--menu-item-height",
        "--tree-input-height",
      ].sort(),
    );
    const returned = Object.keys(interfaceMetrics(16).tokens);
    expect(returned.sort()).toEqual([...TOKENS].sort());
    for (const fixed of ["--space-1", "--radius-m", "--divider-hit", "--icon-button-width", "--segmented-height", "--tree-indent", "--project-dot-size", "--menu-padding"]) {
      expect(returned, fixed).not.toContain(fixed);
    }
  });

  it("数でない・0_以下の大きさは既定として扱う", () => {
    for (const bad of [Number.NaN, Number.POSITIVE_INFINITY, 0, -3]) {
      expect(interfaceMetrics(bad).tokens).toEqual(interfaceMetrics(INTERFACE_BASE_SIZE).tokens);
    }
  });
});
