// 紹介ページ(.github/pages/index.html)を、公開されるときと同じ並び(ページの隣に docs/media/)で開き、
// 横にはみ出さないことを確かめて、画像を testreport/pages/ に撮る(人が目で確かめる)。
// 通常の `npm run e2e` の対象外(playwright.config.ts の説明)。実行: `npm run screenshots:pages`。
//
// ページはスクリプトを持たない静的な HTML なので、開発サーバーは使わない。架空の場所
// (https://arin-pages.test/arin-term/)への要求を、リポジトリのファイルで応える(.github/workflows/pages.yml が
// サイトに置く場所と同じ)。それ以外の場所への要求は止め、外へ読みに行っていないことも確かめる。

import { expect, type Page, test } from "@playwright/test";

const SITE = "https://arin-pages.test/arin-term/";

function repoRoot(): string {
  const configFile = test.info().config.configFile;
  if (configFile === undefined) throw new Error("Playwright の設定ファイルの場所が分からない");
  return configFile.slice(0, configFile.lastIndexOf("/e2e/screenshots/"));
}

/** サイトの場所 → リポジトリのファイル。ここに無いもの(README.html など)は 404。 */
function sourceFor(pathname: string): string | null {
  const root = repoRoot();
  const rel = pathname.replace(/^\/arin-term\//, "");
  if (rel === "" || rel === "index.html") return `${root}/.github/pages/index.html`;
  if (/^docs\/media\/[\w.-]+\.png$/.test(rel)) return `${root}/${rel}`;
  return null;
}

/** ページを開く。外の場所への要求を数える。 */
async function openSite(page: Page): Promise<string[]> {
  const outside: string[] = [];
  await page.route("**/*", async (route) => {
    const url = new URL(route.request().url());
    if (url.origin !== new URL(SITE).origin) {
      outside.push(url.href);
      await route.abort();
      return;
    }
    const source = sourceFor(url.pathname);
    if (source === null) await route.fulfill({ status: 404, body: "not found" });
    else await route.fulfill({ path: source });
  });
  await page.goto(SITE);
  // 遅延読み込みの画像も含めて、すべて読み終えるまで待つ
  await page.evaluate(async () => {
    for (const img of document.images) img.loading = "eager";
    await Promise.all([...document.images].map((img) => img.decode().catch(() => undefined)));
    await document.fonts.ready;
  });
  return outside;
}

for (const scheme of ["light", "dark"] as const) {
  for (const width of [390, 1280] as const) {
    test(`紹介ページ(${scheme}、幅 ${width})`, async ({ page }) => {
      await page.setViewportSize({ width, height: 844 });
      await page.emulateMedia({ colorScheme: scheme });
      const outside = await openSite(page);

      expect(outside, "外の場所へ読みに行った").toEqual([]);
      await expect(page.getByRole("heading", { level: 1 })).toHaveText("Arin");
      // 画像はすべて読めて、外観に合った方が出ている
      const images = await page.evaluate(() =>
        [...document.images].map((img) => ({ src: img.currentSrc, ok: img.complete && img.naturalWidth > 0 })),
      );
      expect(images.filter((img) => !img.ok)).toEqual([]);
      const shots = images.filter((img) => !img.src.endsWith("icon.png"));
      expect(shots.length).toBe(4);
      for (const img of shots) expect(img.src).toContain(`-${scheme}.png`);
      // 横にはみ出さない
      const overflow = await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);
      expect(overflow, "横スクロールが出る").toBeLessThanOrEqual(0);

      await page.screenshot({
        path: `${repoRoot()}/testreport/pages/landing-${scheme}-${width}.png`,
        fullPage: true,
      });
    });
  }
}
