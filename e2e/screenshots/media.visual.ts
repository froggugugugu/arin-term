// README と紹介ページの画像を撮る(docs/media/)。通常の `npm run e2e` の対象外(playwright.config.ts の説明)。
//
// 実行: `npm run screenshots`(一部だけなら `npm run screenshots -- -g タイル`)。
//
// 画面は本物の画面のソースで、Rust の側だけを e2e/fixtures/tauriMock.ts の偽物に替える。写しとターミナルの出力は
// ここで作った見本で、プロジェクトの名前・パス・出力はすべて架空のもの(実在の人・会社・製品を入れない)。
// 撮る画面はライトとダークの 2 枚ずつ:
//   tabs-*.png      タブ表示(サイドバーのプロジェクトとツリー、ターミナルの出力)
//   tiles-*.png     タイル表示(6 枚。番号の札 #1〜#6、アテンションの印)
//   editor-*.png    Markdown のエディタ(編集とプレビューを並べて表示)
//   settings-*.png  設定画面
//
// 窓は 1280×800(Desktop Safari の倍率 2 で 2560×1600 の PNG)。ブラウザには窓の向こうが無いので、
// 実アプリのすりガラスの透け感は写らない(半透明の地が白・黒に重なった色になる)。

import { expect, type Page, test } from "@playwright/test";

import type { AppSnapshot } from "../../src/ipc/generated/AppSnapshot";
import {
  emitEvent,
  installTauriMock,
  listed,
  listingKey,
  type MockListing,
  parseChannelId,
  projectsSnapshot,
  readCallArgs,
  readRecordedCalls,
  sampleEditorTab,
  type SampleProject,
  sendChannelBytes,
  type TauriMockConfig,
  withEditors,
  withFontSizes,
  withTileDisplay,
} from "../fixtures/tauriMock";

const VIEWPORT = { width: 1280, height: 800 } as const;
const SCHEMES = ["light", "dark"] as const;

/** 画像の置き場所(リポジトリの docs/media/)。この設定ファイルは e2e/screenshots/ にある。 */
function mediaPath(name: string): string {
  const configFile = test.info().config.configFile;
  if (configFile === undefined) throw new Error("Playwright の設定ファイルの場所が分からない");
  const root = configFile.slice(0, configFile.lastIndexOf("/e2e/screenshots/"));
  return `${root}/docs/media/${name}.png`;
}

// ── 見本のプロジェクト(架空) ──

const LILYPAD: SampleProject = {
  id: "p-lilypad",
  name: "lilypad-web",
  color: "green",
  displayPath: "~/dev/lilypad-web",
  tabs: ["ターミナル 1", "ターミナル 2"],
};
const RIBBIT: SampleProject = { id: "p-ribbit", name: "ribbit-api", color: "orange", displayPath: "~/dev/ribbit-api" };
const POND: SampleProject = { id: "p-pond", name: "pond-docs", color: "blue", displayPath: "~/dev/pond-docs" };
const TADPOLE: SampleProject = { id: "p-tadpole", name: "tadpole-cli", color: "purple", displayPath: "~/dev/tadpole-cli" };
const CROAK: SampleProject = { id: "p-croak", name: "croak-bot", color: "pink", displayPath: "~/dev/croak-bot" };
const MARSH: SampleProject = { id: "p-marsh", name: "marsh-infra", color: "teal", displayPath: "~/dev/marsh-infra" };

const ALL = [LILYPAD, RIBBIT, POND, TADPOLE, CROAK, MARSH] as const;

/** 写しの共通の手直し(版の表示を 0.1.0 に、ターミナルの文字を 13 に)。 */
function finish(snapshot: AppSnapshot): AppSnapshot {
  return withFontSizes({ ...snapshot, appVersion: "0.1.0" }, { terminal: 13 });
}

/** ツリーの見本(フォルダごと)。並びは Rust と同じく、フォルダが先で名前の順。 */
function listings(): Record<string, MockListing> {
  return {
    [listingKey(LILYPAD.id, "")]: listed([
      ["docs", "directory"],
      ["public", "directory"],
      ["src", "directory"],
      ["tests", "directory"],
      ["index.html", "file"],
      ["package.json", "file"],
      ["README.md", "file"],
      ["tsconfig.json", "file"],
      ["vite.config.ts", "file"],
    ]),
    [listingKey(LILYPAD.id, "src")]: listed([
      ["components", "directory"],
      ["lib", "directory"],
      ["App.tsx", "file"],
      ["main.tsx", "file"],
      ["styles.css", "file"],
    ]),
    [listingKey(LILYPAD.id, "src/components")]: listed([
      ["LeafCard.tsx", "file"],
      ["Pond.tsx", "file"],
      ["Ripple.tsx", "file"],
    ]),
    [listingKey(RIBBIT.id, "")]: listed([
      ["migrations", "directory"],
      ["src", "directory"],
      ["tests", "directory"],
      ["Cargo.lock", "file"],
      ["Cargo.toml", "file"],
      ["README.md", "file"],
    ]),
    [listingKey(POND.id, "")]: listed([
      ["docs", "directory"],
      ["scripts", "directory"],
      ["package.json", "file"],
      ["README.md", "file"],
    ]),
    [listingKey(POND.id, "docs")]: listed([
      ["media", "directory"],
      ["guide.md", "file"],
      ["release-notes.md", "file"],
      ["style.md", "file"],
    ]),
  };
}

// ── ターミナルの出力(ANSI の色つき。架空) ──

const R = "\x1b[0m";
const B = "\x1b[1m";
const DIM = "\x1b[2m";
const GREEN = "\x1b[32m";
const YELLOW = "\x1b[33m";
const BLUE = "\x1b[34m";
const MAGENTA = "\x1b[35m";
const CYAN = "\x1b[36m";
const RED = "\x1b[31m";

/** 2 行のプロンプト(場所とブランチ、続けて ❯)。 */
function prompt(dir: string, branch: string, command = ""): string {
  return `${CYAN}${B}${dir}${R} ${MAGENTA}${branch}${R}\r\n${GREEN}❯${R} ${command}`;
}

const lines = (...rows: string[]) => `${rows.join("\r\n")}\r\n`;

const OUTPUT: Record<string, string> = {
  [`${LILYPAD.id}-t1`]:
    lines(
      prompt("~/dev/lilypad-web", "main", "npm test"),
      "",
      `${DIM}> lilypad-web@0.4.2 test${R}`,
      `${DIM}> vitest run${R}`,
      "",
      ` ${GREEN}✓${R} src/lib/date.test.ts ${DIM}(12 tests) 18ms${R}`,
      ` ${GREEN}✓${R} src/lib/format.test.ts ${DIM}(8 tests) 9ms${R}`,
      ` ${GREEN}✓${R} src/components/Pond.test.tsx ${DIM}(5 tests) 41ms${R}`,
      ` ${GREEN}✓${R} src/components/Ripple.test.tsx ${DIM}(3 tests) 22ms${R}`,
      "",
      ` ${DIM}Test Files${R}  ${GREEN}${B}4 passed${R} ${DIM}(4)${R}`,
      `      ${DIM}Tests${R}  ${GREEN}${B}28 passed${R} ${DIM}(28)${R}`,
      `   ${DIM}Duration${R}  640ms`,
      "",
      prompt("~/dev/lilypad-web", "main", "git status --short"),
      ` ${YELLOW}M${R} src/components/Pond.tsx`,
      ` ${YELLOW}M${R} src/components/Ripple.tsx`,
      `${RED}??${R} src/lib/splash.ts`,
      "",
    ) + prompt("~/dev/lilypad-web", "main"),
  [`${RIBBIT.id}-t1`]:
    lines(
      prompt("~/dev/ribbit-api", "feat/queue", "cargo test"),
      `   ${GREEN}${B}Compiling${R} ribbit-api v0.9.0`,
      `    ${GREEN}${B}Finished${R} test profile in 4.21s`,
      `     ${GREEN}${B}Running${R} unittests src/lib.rs`,
      "",
      "running 14 tests",
      `test queue::push_and_pop ... ${GREEN}ok${R}`,
      `test queue::drains_in_order ... ${GREEN}ok${R}`,
      `test routes::health ... ${GREEN}ok${R}`,
      "",
      `test result: ${GREEN}ok${R}. 14 passed; 0 failed`,
      "",
    ) + prompt("~/dev/ribbit-api", "feat/queue"),
  [`${POND.id}-t1`]:
    lines(
      prompt("~/dev/pond-docs", "main", "git log --oneline -5"),
      `${YELLOW}9f3c2a1${R} docs: 決めごとの表を足す`,
      `${YELLOW}4b81e07${R} docs: はじめにの手順を直す`,
      `${YELLOW}e02d9c4${R} chore: 画像を docs/media へ移す`,
      `${YELLOW}71aa3f0${R} docs: リリースノート 0.3`,
      `${YELLOW}c5d1b28${R} docs: 書き方のメモを追加`,
      "",
    ) + prompt("~/dev/pond-docs", "main"),
  [`${TADPOLE.id}-t1`]:
    lines(
      prompt("~/dev/tadpole-cli", "main", "ls"),
      `${BLUE}${B}bin${R}   ${BLUE}${B}docs${R}   ${BLUE}${B}src${R}   ${BLUE}${B}tests${R}`,
      "Cargo.toml   LICENSE   README.md",
      "",
      prompt("~/dev/tadpole-cli", "main", "tadpole --version"),
      "tadpole 0.2.0",
      "",
    ) + prompt("~/dev/tadpole-cli", "main"),
  [`${CROAK.id}-t1`]:
    lines(
      prompt("~/dev/croak-bot", "main", "npm run dev"),
      "",
      `  ${GREEN}${B}ready${R} in 312 ms`,
      "",
      `  ${GREEN}➜${R}  Local:   ${CYAN}http://localhost:5173/${R}`,
      `  ${DIM}➜  press h + enter to show help${R}`,
      "",
      `${DIM}10:42:07${R} ${CYAN}[hmr]${R} update /src/reply.ts`,
      `${DIM}10:42:31${R} ${CYAN}[hmr]${R} update /src/schedule.ts`,
    ),
  [`${MARSH.id}-t1`]:
    lines(
      prompt("~/dev/marsh-infra", "main", "make check"),
      `${DIM}format${R}   ${GREEN}ok${R}`,
      `${DIM}lint${R}     ${GREEN}ok${R}`,
      `${DIM}plan${R}     3 to add, 1 to change`,
      "",
    ) + prompt("~/dev/marsh-infra", "main"),
};

/** タイルの 1 枚目は狭いので、タブ表示の出力のうち短い出力を流す。 */
const LILYPAD_TILE_OUTPUT =
  lines(
    prompt("~/dev/lilypad-web", "main", "git status --short"),
    ` ${YELLOW}M${R} src/components/Pond.tsx`,
    ` ${YELLOW}M${R} src/components/Ripple.tsx`,
    `${RED}??${R} src/lib/splash.ts`,
    "",
    prompt("~/dev/lilypad-web", "main", "npm run build"),
    `${GREEN}✓${R} 142 modules transformed.`,
    `${DIM}dist/${R}index.html          0.46 kB`,
    `${DIM}dist/assets/${R}index.js   148.20 kB`,
    `${GREEN}✓ built in 1.84s${R}`,
    "",
  ) + prompt("~/dev/lilypad-web", "main");

/** 画面を開き、起動の準備ができるまで待つ。 */
async function open(
  page: Page,
  scheme: (typeof SCHEMES)[number],
  snapshot: AppSnapshot,
  overrides: Partial<TauriMockConfig> = {},
): Promise<void> {
  await page.setViewportSize(VIEWPORT);
  await page.emulateMedia({ colorScheme: scheme });
  await installTauriMock(page, { get_app_snapshot: { ok: snapshot }, ...overrides }, listings());
  await page.goto("/");
  await expect
    .poll(async () => (await readRecordedCalls(page)).map((call) => call.command))
    .toContain("notify_app_ready");
}

/** つながったターミナルに見本の出力を流す。 */
async function writeOutput(page: Page, terminalId: string, text = OUTPUT[terminalId]): Promise<void> {
  if (text === undefined) return;
  await expect
    .poll(async () => (await readCallArgs(page, "connect_terminal")).some((c) => c.terminalId === terminalId))
    .toBe(true);
  const connect = (await readCallArgs(page, "connect_terminal")).find((c) => c.terminalId === terminalId);
  if (connect === undefined) throw new Error(`${terminalId} の接続が無い`);
  await sendChannelBytes(page, parseChannelId(connect.onOutput), [text]);
}

const hostOf = (page: Page, id: string) => page.locator(`[data-testid="terminal-host"][data-terminal-id="${id}"]`);
const treeRow = (page: Page, path: string) =>
  page.locator(`[data-testid="tree-row"][data-path=${JSON.stringify(path)}]`);

/** 画面のフォーカスを外す(撮る画像にフォーカスの輪とカーソルの点滅を残さない)。 */
async function blur(page: Page): Promise<void> {
  await page.evaluate(() => (document.activeElement as HTMLElement | null)?.blur());
}

/** 描き終わるのを待つ(フォントの読み込みと 2 フレーム)。 */
async function settle(page: Page): Promise<void> {
  await page.evaluate(async () => {
    await document.fonts.ready;
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
  });
}

// 各テストの終わりに、画面の失敗(例外、通信の失敗の報告)が無かったことを見る。
test.afterEach(async ({ page }) => {
  const reported = await readCallArgs(page, "report_frontend_error");
  expect(reported.map((r) => r.message), "画面の失敗が Rust へ報告された").toEqual([]);
});

for (const scheme of SCHEMES) {
  test(`タブ表示(${scheme})`, async ({ page }) => {
    await open(page, scheme, finish(projectsSnapshot(ALL)));
    const id = `${LILYPAD.id}-t1`;
    await expect(hostOf(page, id).locator(".xterm-rows")).toHaveCount(1);
    await writeOutput(page, id);
    await expect(hostOf(page, id)).toContainText("28 passed");

    // ツリーの src と src/components を開き、Pond.tsx を選ぶ
    await treeRow(page, "src").getByTestId("tree-row-toggle").click();
    await treeRow(page, "src/components").getByTestId("tree-row-toggle").click();
    await treeRow(page, "src/components/Pond.tsx").click();
    await blur(page);
    await settle(page);
    await page.screenshot({ path: mediaPath(`tabs-${scheme}`) });
  });

  test(`タイル表示(${scheme})`, async ({ page }) => {
    const tabsOnly = ALL.map((p) => ({ ...p, tabs: ["ターミナル 1"] }));
    const snapshot = withTileDisplay(finish(projectsSnapshot(tabsOnly)), {
      focusedTerminalId: `${RIBBIT.id}-t1`,
      attention: [`${CROAK.id}-t1`],
    });
    await open(page, scheme, snapshot);
    await expect(page.getByTestId("tile-number")).toHaveText(["#1", "#2", "#3", "#4", "#5", "#6"]);
    for (const project of tabsOnly) {
      const id = `${project.id}-t1`;
      await expect(hostOf(page, id).locator(".xterm-rows")).toHaveCount(1);
      await writeOutput(page, id, project.id === LILYPAD.id ? LILYPAD_TILE_OUTPUT : OUTPUT[id]);
    }
    await expect(hostOf(page, `${MARSH.id}-t1`)).toContainText("make check");
    await blur(page);
    await settle(page);
    await page.screenshot({ path: mediaPath(`tiles-${scheme}`) });
  });

  test(`Markdown のエディタ(${scheme})`, async ({ page }) => {
    const base = finish(projectsSnapshot(ALL, { selectedProjectId: POND.id }));
    await open(page, scheme, base, {
      open_document: { ok: { kind: "opened", editorId: "editor-1", text: GUIDE, lineEnding: "lf", readOnly: null } },
    });
    await treeRow(page, "docs").getByTestId("tree-row-toggle").click();
    await treeRow(page, "docs/guide.md").dblclick();
    await expect.poll(() => readCallArgs(page, "open_document")).toEqual([{ projectId: POND.id, path: "docs/guide.md" }]);
    await emitEvent(page, "app-snapshot", withEditors(base, POND.id, [sampleEditorTab("editor-1", POND.id, "docs/guide.md")]));
    const panel = page.getByTestId("editor-panel");
    await expect(panel.locator(".cm-content")).toBeVisible();
    await expect(panel.getByRole("heading", { name: "Pond の書き方" })).toBeVisible();
    // 2 行目の末尾にカーソルを置く(1 行目の見出しの強調の帯を避ける)
    await panel.locator(".cm-line").nth(2).click();
    await blur(page);
    await settle(page);
    await page.screenshot({ path: mediaPath(`editor-${scheme}`) });
  });

  test(`設定画面(${scheme})`, async ({ page }) => {
    await open(page, scheme, finish(projectsSnapshot(ALL)));
    const id = `${LILYPAD.id}-t1`;
    await expect(hostOf(page, id).locator(".xterm-rows")).toHaveCount(1);
    await writeOutput(page, id);
    await emitEvent(page, "menu-command", "open-settings");
    const dialog = page.getByTestId("settings-dialog");
    await expect(dialog).toBeVisible();
    await blur(page);
    await settle(page);
    await page.screenshot({ path: mediaPath(`settings-${scheme}`) });
  });
}

/** エディタの見本の文書(架空)。 */
const GUIDE = `# Pond の書き方

小さな静的サイトの、書き方の決めごとをまとめたメモです。

## はじめに

1. \`npm install\` で依存を入れる
2. \`npm run dev\` で開発サーバーを起動する
3. \`docs/\` の Markdown を書き換えると、すぐに反映される

## 決めごと

| 項目 | 決めごと |
| :-- | :-- |
| 見出し | 2 段まで |
| 画像 | \`docs/media/\` に置く |
| 文の長さ | 1 文は 60 字ほどまで |

- [x] 目次をつける
- [x] 用語をそろえる
- [ ] 英語版を用意する

> 迷ったら、短く書く。
`;
