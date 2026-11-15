// タイル表示(T47f): タイルのグリッド、タイトルバー、ページ送り、表示モードの切替の部品。
//
// 偽の Rust 側(fixtures/tauriMock.ts)に写しを渡して画面を開き、部品が正しいコマンドを呼ぶこと、写しのとおりに描くこと、
// レイアウトが崩れていないこと(要素の位置と大きさ)を見る。偽物は写しを送り返さないので、コマンドの結果は
// テストが withTileDisplay(Rust の規則の写し)で作った写しを流して代える。
// T47g: 拡大と復帰(FR-054)、アテンションの印(FR-055)、入力のフォーカスの移し替えと奪わない条件、
// パスの挿入の宛先(ツリーのメニューとドラッグ。M3 の申し送り)。

import { expect, type Locator, type Page, test } from "@playwright/test";

import type { AppSnapshot } from "../src/ipc/generated/AppSnapshot";
import type { ProjectColor } from "../src/ipc/generated/ProjectColor";
import type { ShortcutBinding } from "../src/ipc/generated/ShortcutBinding";
import { DEFAULT_SHORTCUT_TABLE } from "../src/logic/keys/defaultShortcuts";
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
  type SampleProject,
  sendChannelBytes,
  setMockListing,
  type TauriMockConfig,
  withTileDisplay,
} from "./fixtures/tauriMock";

const A: SampleProject = { id: "p-a", name: "app", color: "green", tabs: ["ターミナル 1", "ターミナル 2"] };
const B: SampleProject = { id: "p-b", name: "ribbitlog", color: "orange", tabs: ["ターミナル 1"] };

const COLORS: readonly ProjectColor[] = ["blue", "green", "orange", "purple", "teal", "pink", "red", "yellow"];

/** 1 枚ずつタブを持つプロジェクトを n 件(色は順に替える)。ターミナルの ID は `p<番号>-t1`。 */
function projects(n: number): SampleProject[] {
  return Array.from({ length: n }, (_, i) => ({
    id: `p${i + 1}`,
    name: `proj${i + 1}`,
    color: COLORS[i % COLORS.length] ?? "blue",
    tabs: ["ターミナル 1"],
  }));
}

/** 写しを渡して画面を開き、準備完了まで待つ。 */
async function open(
  page: Page,
  snapshot: AppSnapshot,
  overrides: Partial<TauriMockConfig> = {},
  listings: Record<string, MockListing> = {},
): Promise<void> {
  await installTauriMock(page, { get_app_snapshot: { ok: snapshot }, ...overrides }, listings);
  await page.goto("/");
  await expect
    .poll(async () => (await readRecordedCalls(page)).map((call) => call.command))
    .toContain("notify_app_ready");
}

/** 要素の位置と大きさ。無ければ失敗にする。 */
async function box(locator: Locator) {
  const b = await locator.boundingBox();
  if (b === null) throw new Error("要素の位置と大きさが取れない(表示されていない)");
  return b;
}

const modeSwitch = (page: Page) => page.getByTestId("display-mode-switch");
const tabsButton = (page: Page) => modeSwitch(page).getByRole("button", { name: "タブ表示" });
const tilesButton = (page: Page) => modeSwitch(page).getByRole("button", { name: "タイル表示" });
const grid = (page: Page) => page.getByTestId("tile-grid");
const tiles = (page: Page) => page.getByTestId("tile");
const tile = (page: Page, title: string) => page.getByRole("group", { name: title, exact: true });
const paging = (page: Page) => page.getByTestId("tile-paging");
const previousButton = (page: Page) => paging(page).getByRole("button", { name: "前のページ" });
const nextButton = (page: Page) => paging(page).getByRole("button", { name: "次のページ" });
const card = (page: Page, name: string) => page.getByTestId("project-card").filter({ hasText: name });
/** プロジェクト色の印(カード・タイルの名前の前の小さな丸)の色。 */
const dotColor = (locator: Locator) =>
  locator.locator(".project-dot").first().evaluate((el) => getComputedStyle(el).backgroundColor);

/** 表示中のページのタイルのターミナルの ID(描いた順)。 */
async function tileIds(page: Page): Promise<string[]> {
  return tiles(page).evaluateAll((els) => els.map((el) => el.getAttribute("data-tile-id") ?? ""));
}

/** 指定のターミナルの接続(connect_terminal の引数)。届くまで待つ。 */
async function connectOf(page: Page, terminalId: string) {
  await expect
    .poll(async () => (await readCallArgs(page, "connect_terminal")).some((c) => c.terminalId === terminalId))
    .toBe(true);
  const found = (await readCallArgs(page, "connect_terminal")).find((c) => c.terminalId === terminalId);
  if (found === undefined) throw new Error(`${terminalId} の接続が無い`);
  return found;
}

async function ackedBytes(page: Page, terminalId: string): Promise<number> {
  const acks = await readCallArgs(page, "ack_terminal_output");
  return acks.filter((a) => a.terminalId === terminalId).reduce((sum, a) => sum + a.bytes, 0);
}

/** ターミナルに最後に送った寸法(接続の寸法か、そのあとの寸法の変更)。 */
async function lastSize(page: Page, terminalId: string): Promise<{ cols: number; rows: number } | null> {
  const resized = (await readCallArgs(page, "resize_terminal")).filter((r) => r.terminalId === terminalId);
  const last = resized.at(-1);
  if (last !== undefined) return { cols: last.cols, rows: last.rows };
  const connected = (await readCallArgs(page, "connect_terminal")).find((c) => c.terminalId === terminalId);
  return connected === undefined ? null : { cols: connected.cols, rows: connected.rows };
}

// 各テストの終わりに、画面の失敗(例外、通信の失敗の報告)が無かったことを見る。
test.afterEach(async ({ page }) => {
  const reported = await readCallArgs(page, "report_frontend_error");
  expect(reported.map((r) => r.message), "画面の失敗が Rust へ報告された").toEqual([]);
});

test.describe("表示モードの切替(FR-048)", () => {
  test("切替のボタンでタイル表示になり全プロジェクトのターミナルが並ぶ", async ({ page }) => {
    await open(page, projectsSnapshot([A, B]));
    await expect(tabsButton(page)).toHaveAttribute("aria-pressed", "true");
    await expect(tilesButton(page)).toHaveAttribute("aria-pressed", "false");

    await tilesButton(page).click();
    await expect.poll(() => readCallArgs(page, "set_display_mode")).toEqual([{ mode: "tiles" }]);

    // Rust が返す写し: タイル表示で、全プロジェクトのターミナルが写しの順(プロジェクト順 → タブ順)に並ぶ(FR-051)。
    await emitEvent(page, "app-snapshot", withTileDisplay(projectsSnapshot([A, B])));
    await expect(grid(page)).toBeVisible();
    await expect.poll(() => tileIds(page)).toEqual(["p-a-t1", "p-a-t2", "p-b-t1"]);
    await expect(page.getByTestId("terminal-host")).toHaveCount(3);
    await expect(page.getByTestId("tab-bar")).toHaveCount(0);
    await expect(tilesButton(page)).toHaveAttribute("aria-pressed", "true");
    await expect(tabsButton(page)).toHaveAttribute("aria-pressed", "false");
  });

  test("もう一度でタブ表示に戻り表示中のタブ_1_枚だけが描かれる", async ({ page }) => {
    await open(page, withTileDisplay(projectsSnapshot([A, B])));
    await expect(tiles(page)).toHaveCount(3);

    await tabsButton(page).click();
    await expect.poll(() => readCallArgs(page, "set_display_mode")).toEqual([{ mode: "tabs" }]);

    await emitEvent(page, "app-snapshot", withTileDisplay(projectsSnapshot([A, B]), { mode: "tabs" }));
    await expect(grid(page)).toHaveCount(0);
    await expect(page.getByTestId("tab-bar")).toBeVisible();
    await expect(page.getByTestId("terminal-host")).toHaveCount(1);
    await expect(page.getByTestId("terminal-host")).toHaveAttribute("data-terminal-id", "p-a-t1");
  });

  test("いまのモードのボタンを押してもコマンドを呼ばない", async ({ page }) => {
    await open(page, withTileDisplay(projectsSnapshot([A, B])));
    await tilesButton(page).click();
    await tabsButton(page).click();
    await expect.poll(() => readCallArgs(page, "set_display_mode")).toEqual([{ mode: "tabs" }]);
  });

  test("切替のボタンのツールチップに表のキーが出る", async ({ page }) => {
    // 表の既定のキーを差し替えても、ツールチップはそれに従う(部品にキーの文字列を書いていない)。
    const table: ShortcutBinding[] = DEFAULT_SHORTCUT_TABLE.map((b) =>
      b.command === "view.toggle-display-mode" ? { ...b, accelerator: "Cmd+Alt+T" } : { ...b },
    );
    await open(page, projectsSnapshot([A, B]), { get_shortcuts: { ok: table } });
    await expect(tilesButton(page)).toHaveAttribute("title", /⌥⌘T/);
    await expect(tabsButton(page)).toHaveAttribute("title", /⌥⌘T/);
  });

  test("切替のボタンは上部の帯の右側にあり帯の高さに収まる", async ({ page }) => {
    await open(page, projectsSnapshot([A, B]));
    const header = await box(page.locator(".app-shell__header"));
    const control = await box(modeSwitch(page));
    expect(control.x).toBeGreaterThan(header.x + header.width / 2);
    expect(control.y).toBeGreaterThanOrEqual(header.y);
    expect(control.y + control.height).toBeLessThanOrEqual(header.y + header.height);
  });
});

test.describe("タイルのグリッド(FR-051)", () => {
  const cases: ReadonlyArray<readonly [number, number, number]> = [
    [1, 1, 1],
    [2, 2, 1],
    [4, 2, 2],
    [6, 3, 2],
    [9, 3, 3],
  ];
  for (const [count, cols, rows] of cases) {
    test(`枚数に応じたグリッド_${count}_枚は_${cols}×${rows}`, async ({ page }) => {
      await open(page, withTileDisplay(projectsSnapshot(projects(count))));
      await expect(tiles(page)).toHaveCount(count);
      await expect(grid(page)).toHaveAttribute("data-cols", String(cols));
      await expect(grid(page)).toHaveAttribute("data-rows", String(rows));
      // 見た目でも列×行に並ぶ(左端の位置の種類が列の数、上端の位置の種類が行の数)。
      const boxes = await tiles(page).evaluateAll((els) =>
        els.map((el) => {
          const r = el.getBoundingClientRect();
          return { x: Math.round(r.x), y: Math.round(r.y), w: r.width, h: r.height };
        }),
      );
      expect(new Set(boxes.map((b) => b.x)).size).toBe(cols);
      expect(new Set(boxes.map((b) => b.y)).size).toBe(rows);
      // タイルは同じ大きさで、グリッドを埋める。
      const area = await box(grid(page));
      for (const b of boxes) {
        expect(Math.abs(b.w - area.width / cols)).toBeLessThan(2);
        expect(Math.abs(b.h - area.height / rows)).toBeLessThan(2);
      }
    });
  }

  test("最後の行が埋まらないときは空きを残し_タイルは同じ大きさ", async ({ page }) => {
    await open(page, withTileDisplay(projectsSnapshot(projects(3))));
    await expect(grid(page)).toHaveAttribute("data-cols", "2");
    await expect(grid(page)).toHaveAttribute("data-rows", "2");
    const [first, , third] = await tiles(page).evaluateAll((els) => els.map((el) => el.getBoundingClientRect().width));
    expect(third).toBe(first);
  });

  test("タイルのタイトルはプロジェクト名:_タブ名", async ({ page }) => {
    await open(page, withTileDisplay(projectsSnapshot([A, B])));
    await expect(page.getByTestId("tile-title")).toHaveText(["app: ターミナル 1", "app: ターミナル 2", "ribbitlog: ターミナル 1"]);
    await expect(tile(page, "ribbitlog: ターミナル 1")).toBeVisible();
    // タイトルバーは控えめな高さで、その下にターミナルの置き場所がある。
    const title = await box(tile(page, "app: ターミナル 1").getByTestId("tile-title"));
    const host = await box(tile(page, "app: ターミナル 1").getByTestId("terminal-host"));
    expect(title.height).toBeGreaterThanOrEqual(20);
    expect(title.height).toBeLessThanOrEqual(26);
    expect(host.y).toBeGreaterThanOrEqual(title.y + title.height - 1);
  });

  test("a_の全タイルとカードの色の印は同じ色相で_b_のタイルとは違う", async ({ page }) => {
    await open(page, withTileDisplay(projectsSnapshot([A, B])));
    const cardA = await dotColor(card(page, "app"));
    expect(await dotColor(tile(page, "app: ターミナル 1"))).toBe(cardA);
    expect(await dotColor(tile(page, "app: ターミナル 2"))).toBe(cardA);
    const tileB = await dotColor(tile(page, "ribbitlog: ターミナル 1"));
    expect(tileB).toBe(await dotColor(card(page, "ribbitlog")));
    expect(tileB).not.toBe(cardA);
    // タイトルバーは色で塗らない(色は印だけ)。
    const fill = await tile(page, "app: ターミナル 1")
      .getByTestId("tile-title")
      .evaluate((el) => getComputedStyle(el).backgroundColor);
    expect(fill).not.toBe(cardA);
  });

  test("フォーカス中のタイルが読み上げと見た目で分かる", async ({ page }) => {
    await open(page, withTileDisplay(projectsSnapshot([A, B])));
    await expect(tile(page, "app: ターミナル 1")).toHaveAttribute("aria-current", "true");
    await expect(tile(page, "app: ターミナル 2")).not.toHaveAttribute("aria-current", "true");
    const weight = (title: string) =>
      tile(page, title)
        .getByTestId("tile-title")
        .locator(".tile-title__label")
        .evaluate((el) => getComputedStyle(el).fontWeight);
    // 色だけに頼らない(太字にもなる)。
    expect(await weight("app: ターミナル 1")).not.toBe(await weight("app: ターミナル 2"));
  });

  test("html_のような名前もそのまま文字で出る", async ({ page }) => {
    const odd: SampleProject = { id: "p-x", name: "<b>x</b>", color: "purple", tabs: ['<img src=x onerror="alert(1)">'] };
    await open(page, withTileDisplay(projectsSnapshot([odd, B])));
    const title = page.getByTestId("tile-title").first();
    await expect(title).toHaveText('<b>x</b>: <img src=x onerror="alert(1)">');
    await expect(title.locator("b, img")).toHaveCount(0);
  });

  test("長い名前は末尾を省略してタイトルバーからはみ出さない", async ({ page }) => {
    const long: SampleProject = { id: "p-l", name: "とても長いプロジェクトの名前".repeat(4), color: "teal", tabs: ["ターミナル 1"] };
    await open(page, withTileDisplay(projectsSnapshot([long, ...projects(3)])));
    const bar = tiles(page).first().getByTestId("tile-title");
    const label = bar.locator(".tile-title__label");
    const [barBox, labelBox] = [await box(bar), await box(label)];
    expect(labelBox.x + labelBox.width).toBeLessThanOrEqual(barBox.x + barBox.width);
    expect(await label.evaluate((el) => getComputedStyle(el).textOverflow)).toBe("ellipsis");
  });

  test("タイル表示で_0_枚ならターミナルがありませんだけを出す", async ({ page }) => {
    const empty: SampleProject = { id: "p-e", name: "empty", tabs: [] };
    await open(page, withTileDisplay(projectsSnapshot([empty])));
    await expect(page.getByTestId("terminals-empty")).toContainText("ターミナルがありません");
    await expect(grid(page)).toHaveCount(0);
    await expect(page.getByTestId("terminal-host")).toHaveCount(0);
  });

  test("ウィンドウの大きさを変えると各タイルの寸法が送られる", async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 800 });
    await open(page, withTileDisplay(projectsSnapshot(projects(4))));
    const ids = ["p1-t1", "p2-t1", "p3-t1", "p4-t1"];
    for (const id of ids) await connectOf(page, id);
    const before = await Promise.all(ids.map((id) => lastSize(page, id)));

    await page.setViewportSize({ width: 900, height: 560 });
    for (const [i, id] of ids.entries()) {
      await expect.poll(async () => JSON.stringify(await lastSize(page, id))).not.toBe(JSON.stringify(before[i]));
      const size = await lastSize(page, id);
      expect(size?.cols ?? 0).toBeGreaterThan(0);
      expect(size?.rows ?? 0).toBeGreaterThan(0);
    }
  });
});

test.describe("タイルへのフォーカス(FR-053、AC-M15)", () => {
  test("別のプロジェクトのタイルをクリックするとカードが選ばれツリーのルートが替わる", async ({ page }) => {
    await open(page, withTileDisplay(projectsSnapshot([A, B])));
    await expect(page.getByTestId("tree-header")).toHaveText("app");

    await tile(page, "ribbitlog: ターミナル 1").getByTestId("tile-title").click();
    await expect.poll(() => readCallArgs(page, "focus_terminal")).toEqual([{ terminalId: "p-b-t1" }]);

    // Rust が返す写し: b が選ばれ、そのタブがフォーカス中のタイルになる。
    await emitEvent(page, "app-snapshot", withTileDisplay(projectsSnapshot([A, B]), { focusedTerminalId: "p-b-t1" }));
    await expect(card(page, "ribbitlog")).toHaveAttribute("aria-current", "true");
    await expect(page.getByTestId("tree-header")).toHaveText("ribbitlog");
    await expect(tile(page, "ribbitlog: ターミナル 1")).toHaveAttribute("aria-current", "true");
    await expect(tile(page, "app: ターミナル 1")).not.toHaveAttribute("aria-current", "true");
  });

  test("ターミナルの本文をクリックしてもフォーカスのコマンドが呼ばれる", async ({ page }) => {
    await open(page, withTileDisplay(projectsSnapshot([A, B])));
    await connectOf(page, "p-a-t2");
    await tile(page, "app: ターミナル 2").getByTestId("terminal-host").click();
    await expect.poll(() => readCallArgs(page, "focus_terminal")).toEqual([{ terminalId: "p-a-t2" }]);
  });

  test("フォーカス中のタイルをクリックしてもコマンドを呼ばない", async ({ page }) => {
    await open(page, withTileDisplay(projectsSnapshot([A, B])));
    await tile(page, "app: ターミナル 1").getByTestId("tile-title").click();
    await tile(page, "app: ターミナル 2").getByTestId("tile-title").click();
    await expect.poll(() => readCallArgs(page, "focus_terminal")).toEqual([{ terminalId: "p-a-t2" }]);
  });

  test("カードを選ぶとそのプロジェクトの最初のタイルのページへ移る", async ({ page }) => {
    // 10 件: 10 件目(proj10)のタイルは 2 ページ目。
    const list = projects(10);
    await open(page, withTileDisplay(projectsSnapshot(list)));
    await expect(page.getByTestId("tile-page-label")).toHaveText("1/2");

    await card(page, "proj10").click();
    await expect.poll(() => readCallArgs(page, "select_project")).toEqual([{ projectId: "p10" }]);

    await emitEvent(page, "app-snapshot", withTileDisplay(projectsSnapshot(list, { selectedProjectId: "p10" })));
    await expect(page.getByTestId("tile-page-label")).toHaveText("2/2");
    await expect.poll(() => tileIds(page)).toEqual(["p10-t1"]);
    await expect(tile(page, "proj10: ターミナル 1")).toHaveAttribute("aria-current", "true");
  });
});

test.describe("ページ送り(FR-052、AC-M14、NFR-009)", () => {
  test("10_枚で_1_ページ目に_9_枚_2_ページ目に_1_枚_表示は_1/2", async ({ page }) => {
    const list = projects(10);
    await open(page, withTileDisplay(projectsSnapshot(list)));
    await expect(tiles(page)).toHaveCount(9);
    await expect(grid(page)).toHaveAttribute("data-cols", "3");
    await expect(grid(page)).toHaveAttribute("data-rows", "3");
    await expect(page.getByTestId("tile-page-label")).toHaveText("1/2");

    await emitEvent(page, "app-snapshot", withTileDisplay(projectsSnapshot(list), { page: 1 }));
    await expect(tiles(page)).toHaveCount(1);
    await expect(grid(page)).toHaveAttribute("data-cols", "1");
    await expect(grid(page)).toHaveAttribute("data-rows", "1");
    await expect(page.getByTestId("tile-page-label")).toHaveText("2/2");
  });

  test("前後のボタンでページが替わり端のボタンは無効", async ({ page }) => {
    const list = projects(10);
    await open(page, withTileDisplay(projectsSnapshot(list)));
    await expect(previousButton(page)).toBeDisabled();
    await expect(nextButton(page)).toBeEnabled();

    await nextButton(page).click();
    await expect.poll(async () => (await readRecordedCalls(page)).filter((c) => c.command === "next_tile_page")).toHaveLength(1);
    await emitEvent(page, "app-snapshot", withTileDisplay(projectsSnapshot(list), { page: 1 }));
    await expect(nextButton(page)).toBeDisabled();
    await expect(previousButton(page)).toBeEnabled();

    await previousButton(page).click();
    await expect
      .poll(async () => (await readRecordedCalls(page)).filter((c) => c.command === "previous_tile_page"))
      .toHaveLength(1);
    // 無効のボタンを押してもコマンドは増えない。
    await nextButton(page).click({ force: true });
    expect((await readRecordedCalls(page)).filter((c) => c.command === "next_tile_page")).toHaveLength(1);
  });

  test("ページ送りのボタンのツールチップに表の文言とキーが出る", async ({ page }) => {
    await open(page, withTileDisplay(projectsSnapshot(projects(10))));
    await expect(previousButton(page)).toHaveAttribute("title", "前のページ(⌥⌘←)");
    await expect(nextButton(page)).toHaveAttribute("title", "次のページ(⌥⌘→)");
  });

  test("ページが_1_つのときはページ表示を出さない", async ({ page }) => {
    await open(page, withTileDisplay(projectsSnapshot(projects(9))));
    await expect(tiles(page)).toHaveCount(9);
    await expect(paging(page)).toHaveCount(0);
  });

  test("ページ表示は帯の右端にありタブバーと同じ高さ", async ({ page }) => {
    await open(page, withTileDisplay(projectsSnapshot(projects(10))));
    const strip = await box(page.getByTestId("tile-strip"));
    const control = await box(paging(page));
    expect(Math.abs(strip.x + strip.width - (control.x + control.width))).toBeLessThan(12);
    expect(control.y).toBeGreaterThanOrEqual(strip.y);
    expect(control.y + control.height).toBeLessThanOrEqual(strip.y + strip.height);
    // タブ表示のタブバーと同じ高さ(切り替えでターミナルの上端が跳ねない)。
    await emitEvent(page, "app-snapshot", withTileDisplay(projectsSnapshot(projects(10)), { mode: "tabs" }));
    const tabBar = await box(page.getByTestId("tab-bar"));
    expect(tabBar.height).toBe(strip.height);
    expect(tabBar.y).toBe(strip.y);
  });

  test("表示中のページのタイルの置き場所だけがある", async ({ page }) => {
    await open(page, withTileDisplay(projectsSnapshot(projects(10))));
    await expect(page.getByTestId("terminal-host")).toHaveCount(9);
    // ターミナルの要素も表示中のページの 9 枚だけ(隠したものは DOM の外。NFR-009)。
    await expect(page.locator(".xterm")).toHaveCount(9);
    const hosts = await page
      .getByTestId("terminal-host")
      .evaluateAll((els) => els.map((el) => el.getAttribute("data-terminal-id")));
    expect(hosts).toEqual(Array.from({ length: 9 }, (_, i) => `p${i + 1}-t1`));
  });

  test("表示していないページのターミナルにも出力が届き戻ると表示されている", async ({ page }) => {
    const list = projects(10);
    await open(page, withTileDisplay(projectsSnapshot(list)));
    const channel = parseChannelId((await connectOf(page, "p10-t1")).onOutput);

    const before = await ackedBytes(page, "p10-t1");
    const sent = await sendChannelBytes(page, channel, ["on page two\r\n"]);
    await expect.poll(() => ackedBytes(page, "p10-t1")).toBe(before + sent);

    await emitEvent(page, "app-snapshot", withTileDisplay(projectsSnapshot(list), { page: 1 }));
    await expect(
      tile(page, "proj10: ターミナル 1").getByTestId("terminal-host").locator(".xterm-rows > div").nth(0),
    ).toHaveText("on page two");
    // 接続は作り直さない。
    expect((await readCallArgs(page, "connect_terminal")).filter((c) => c.terminalId === "p10-t1")).toHaveLength(1);
  });
});

// ── T47g ──────────────────────────────────────────────────────────────────────────────────────────

const titleBar = (page: Page, title: string) => tile(page, title).getByTestId("tile-title");
/** タイルのターミナルの入力欄(xterm.js がキーを受ける要素)。 */
const tileInput = (page: Page, title: string) => tile(page, title).locator(".xterm-helper-textarea");
const maximizedStrip = (page: Page) => page.getByTestId("maximized-strip");
const restoreButton = (page: Page) => maximizedStrip(page).getByRole("button", { name: "タイル表示に戻す" });
const terminalHosts = (page: Page) => page.getByTestId("terminal-host");
const attentionMark = (locator: Locator) => locator.getByRole("img", { name: "新しい通知があります" });
/** ページ送りのボタンに重ねた印(飾り。読み上げはボタンの名前で確かめる)。 */
const pageMark = (button: Locator) => button.getByTestId("page-attention");

/** 指定のコマンドが呼ばれた回数。 */
async function countOf(page: Page, command: string): Promise<number> {
  return (await readRecordedCalls(page)).filter((c) => c.command === command).length;
}

/** 起動の直後に、フォーカス中のタイルへ入力のフォーカスが移り終わるのを待つ(以後の確かめが起動の動きと混ざらない)。 */
async function settleFocus(page: Page, title: string): Promise<void> {
  await expect(tileInput(page, title)).toBeFocused();
}

/** しばらく待つ(「起きない」ことを確かめるとき。次の描画のフレームでの移し替えが済む時間)。 */
const SETTLE_MS = 250;

test.describe("拡大と復帰(FR-054)", () => {
  test("タイトルバーをダブルクリックするとそのターミナルがタブ表示の形で出て表示モードはタイルのまま", async ({ page }) => {
    await open(page, withTileDisplay(projectsSnapshot([A, B])));
    await settleFocus(page, "app: ターミナル 1");

    await titleBar(page, "app: ターミナル 2").dblclick();
    // 押下でそのタイルにフォーカスし(写しが返る前の 2 回目の押下でも同じタイルを求める。Rust では何も変わらない)、
    // ダブルクリックで拡大を 1 回だけ求める。
    await expect
      .poll(async () => [...new Set((await readCallArgs(page, "focus_terminal")).map((c) => c.terminalId))])
      .toEqual(["p-a-t2"]);
    await expect.poll(() => countOf(page, "toggle_tile_maximize")).toBe(1);

    // Rust が返す写し: タイル表示のまま拡大中で、フォーカス中のタイル 1 枚だけが見える。
    await emitEvent(
      page,
      "app-snapshot",
      withTileDisplay(projectsSnapshot([A, B]), { focusedTerminalId: "p-a-t2", maximized: true }),
    );
    await expect(grid(page)).toHaveCount(0);
    await expect(terminalHosts(page)).toHaveCount(1);
    await expect(terminalHosts(page)).toHaveAttribute("data-terminal-id", "p-a-t2");
    // タブバーは出さず、上の帯に拡大しているタイルの名前(色の印つき)と元に戻すボタン。
    await expect(page.getByTestId("tab-bar")).toHaveCount(0);
    await expect(maximizedStrip(page).getByTestId("maximized-title")).toHaveText("app: ターミナル 2");
    expect(await dotColor(maximizedStrip(page))).toBe(await dotColor(card(page, "app")));
    await expect(restoreButton(page)).toBeVisible();
    await expect(tilesButton(page)).toHaveAttribute("aria-pressed", "true");
    // 拡大したターミナルに入力のフォーカスが移る。
    await expect(terminalHosts(page).locator(".xterm-helper-textarea")).toBeFocused();
    // 帯はタブバー・タイルの帯と同じ高さ(拡大と復帰でターミナルの上端が跳ねない)。
    const strip = await box(maximizedStrip(page));
    await emitEvent(page, "app-snapshot", withTileDisplay(projectsSnapshot([A, B]), { focusedTerminalId: "p-a-t2" }));
    const tileStrip = await box(page.getByTestId("tile-strip"));
    expect(strip.height).toBe(tileStrip.height);
    expect(strip.y).toBe(tileStrip.y);
  });

  test("元に戻すボタンでタイル表示に戻り前のページが出る", async ({ page }) => {
    const list = projects(10);
    await open(page, withTileDisplay(projectsSnapshot(list), { page: 1 }));
    await settleFocus(page, "proj10: ターミナル 1");
    await emitEvent(page, "app-snapshot", withTileDisplay(projectsSnapshot(list), { page: 1, maximized: true }));
    await expect(maximizedStrip(page)).toBeVisible();
    // 拡大中はページ送りを出さない。
    await expect(paging(page)).toHaveCount(0);

    await restoreButton(page).click();
    await expect.poll(() => countOf(page, "toggle_tile_maximize")).toBe(1);

    await emitEvent(page, "app-snapshot", withTileDisplay(projectsSnapshot(list), { page: 1 }));
    await expect(maximizedStrip(page)).toHaveCount(0);
    await expect(page.getByTestId("tile-page-label")).toHaveText("2/2");
    await expect.poll(() => tileIds(page)).toEqual(["p10-t1"]);
    // 置き場所が替わっても、フォーカス中のタイルに入力のフォーカスが戻る。
    await expect(tileInput(page, "proj10: ターミナル 1")).toBeFocused();
  });

  test("拡大中のタイトル帯のダブルクリックでも戻る", async ({ page }) => {
    await open(page, withTileDisplay(projectsSnapshot([A, B]), { maximized: true }));
    await expect(maximizedStrip(page)).toBeVisible();

    await maximizedStrip(page).getByTestId("maximized-title").dblclick();
    await expect.poll(() => countOf(page, "toggle_tile_maximize")).toBe(1);
    await page.waitForTimeout(SETTLE_MS);
    expect(await countOf(page, "toggle_tile_maximize")).toBe(1);
  });

  test("元に戻すボタンをダブルクリックしても_1_回だけ戻す", async ({ page }) => {
    await open(page, withTileDisplay(projectsSnapshot([A, B]), { maximized: true }));
    await restoreButton(page).dblclick();
    await expect.poll(() => countOf(page, "toggle_tile_maximize")).toBe(1);
    await page.waitForTimeout(SETTLE_MS);
    expect(await countOf(page, "toggle_tile_maximize")).toBe(1);
  });

  test("ターミナルの本文のダブルクリックでは拡大しない", async ({ page }) => {
    await open(page, withTileDisplay(projectsSnapshot([A, B])));
    await connectOf(page, "p-a-t1");
    await tile(page, "app: ターミナル 1").getByTestId("terminal-host").dblclick();
    await page.waitForTimeout(SETTLE_MS);
    expect(await countOf(page, "toggle_tile_maximize")).toBe(0);
  });

  test("拡大中にタブが替わると拡大したままそのタブを出す", async ({ page }) => {
    await open(page, withTileDisplay(projectsSnapshot([A, B]), { maximized: true }));
    await expect(maximizedStrip(page).getByTestId("maximized-title")).toHaveText("app: ターミナル 1");

    // カードの選択や ⌘⇧] の結果(Rust は拡大したまま、新しいフォーカス中のタイルを出す。計画【仮定】7)。
    await emitEvent(
      page,
      "app-snapshot",
      withTileDisplay(projectsSnapshot([A, B]), { focusedTerminalId: "p-b-t1", maximized: true }),
    );
    await expect(maximizedStrip(page).getByTestId("maximized-title")).toHaveText("ribbitlog: ターミナル 1");
    await expect(terminalHosts(page)).toHaveCount(1);
    await expect(terminalHosts(page)).toHaveAttribute("data-terminal-id", "p-b-t1");
    await expect(terminalHosts(page).locator(".xterm-helper-textarea")).toBeFocused();
  });

  test("元に戻すボタンのツールチップに表の文言とキーが出る", async ({ page }) => {
    await open(page, withTileDisplay(projectsSnapshot([A, B]), { maximized: true }));
    await expect(restoreButton(page)).toHaveAttribute("title", "タイル表示に戻す(⇧⌘↩)");
  });

  test("元に戻すボタンのツールチップのキーは差し替えた表に従う", async ({ page }) => {
    // 部品にキーの文字列を書いていない(前提 (f))。
    const table: ShortcutBinding[] = DEFAULT_SHORTCUT_TABLE.map((b) =>
      b.command === "view.toggle-maximize" ? { ...b, accelerator: "Cmd+Alt+M" } : { ...b },
    );
    await open(page, withTileDisplay(projectsSnapshot([A, B]), { maximized: true }), { get_shortcuts: { ok: table } });
    await expect(restoreButton(page)).toHaveAttribute("title", "タイル表示に戻す(⌥⌘M)");
  });
});

test.describe("アテンションの印(FR-055)", () => {
  test("アテンションのあるタイルのタイトルに印が付く", async ({ page }) => {
    // Rust の規則(計画【仮定】14、ADR-011 の 7): いま見ているのはフォーカス中のタイル(app: ターミナル 1)だけ。
    // 同じページに見えているほかのタイルの契機には印が立つ。5 枚とも 1 ページ目で、すべてのタイルで契機があった。
    await open(
      page,
      withTileDisplay(projectsSnapshot([A, B, ...projects(2)]), {
        attention: ["p-a-t1", "p-a-t2", "p-b-t1", "p2-t1"],
      }),
    );
    await expect(attentionMark(titleBar(page, "ribbitlog: ターミナル 1"))).toBeVisible();
    await expect(attentionMark(titleBar(page, "proj2: ターミナル 1"))).toBeVisible();
    await expect(attentionMark(titleBar(page, "app: ターミナル 2"))).toBeVisible();
    // フォーカス中のタイルと、契機の無かったタイルには付かない。
    await expect(attentionMark(titleBar(page, "app: ターミナル 1"))).toHaveCount(0);
    await expect(attentionMark(titleBar(page, "proj1: ターミナル 1"))).toHaveCount(0);
    // 印はタイトルの右端(名前の後ろ)にあり、プロジェクト色の印とは色が違う。
    const bar = await box(titleBar(page, "ribbitlog: ターミナル 1"));
    const mark = await box(attentionMark(titleBar(page, "ribbitlog: ターミナル 1")));
    const label = await box(titleBar(page, "ribbitlog: ターミナル 1").locator(".tile-title__label"));
    expect(mark.x).toBeGreaterThanOrEqual(label.x + label.width - 1);
    expect(bar.x + bar.width - (mark.x + mark.width)).toBeLessThan(16);
    const markColor = await attentionMark(titleBar(page, "ribbitlog: ターミナル 1")).evaluate(
      (el) => getComputedStyle(el).backgroundColor,
    );
    expect(markColor).not.toBe(await dotColor(tile(page, "ribbitlog: ターミナル 1")));
    expect(markColor).not.toBe("rgba(0, 0, 0, 0)");
  });

  test("別のページのアテンションはページ表示の前後のボタンに印が付く", async ({ page }) => {
    const list = projects(19);
    // 1 ページ目を表示中に、2 ページ目(p10)と、同じページのフォーカスの無い p2 のタイルでベルが鳴った。
    // 表示中のページの印(p2)はタイトルに付き、前後のボタンには数えない。
    await open(page, withTileDisplay(projectsSnapshot(list), { attention: ["p10-t1", "p2-t1"] }));
    await expect(attentionMark(titleBar(page, "proj2: ターミナル 1"))).toBeVisible();
    await expect(pageMark(nextButton(page))).toBeVisible();
    await expect(pageMark(previousButton(page))).toHaveCount(0);
    await expect(nextButton(page)).toHaveAttribute("aria-label", /新しい通知があります/);
    await expect(previousButton(page)).toHaveAttribute("aria-label", "前のページ");

    // 3 ページ目を表示中に、1 ページ目(p1)で鳴った: 前のボタンにだけ付く。
    await emitEvent(page, "app-snapshot", withTileDisplay(projectsSnapshot(list), { page: 2, attention: ["p1-t1"] }));
    await expect(page.getByTestId("tile-page-label")).toHaveText("3/3");
    await expect(pageMark(previousButton(page))).toBeVisible();
    await expect(pageMark(nextButton(page))).toHaveCount(0);
  });

  test("フォーカスすると印が消え_ページを送って見えただけのタイルの印は残る", async ({ page }) => {
    // 12 枚: 2 ページ目は p10〜p12。p10 と p11 で契機があった
    const list = projects(12);
    const attention = ["p10-t1", "p11-t1"];
    await open(page, withTileDisplay(projectsSnapshot(list), { attention }));
    await expect(pageMark(nextButton(page))).toBeVisible();

    await nextButton(page).click();
    // Rust が返す写し: 2 ページ目を表示し、フォーカスの移った先頭の p10 の印だけを消している。
    await emitEvent(page, "app-snapshot", withTileDisplay(projectsSnapshot(list), { page: 1, attention }));
    await expect(page.getByTestId("tile-page-label")).toHaveText("2/2");
    await expect(attentionMark(titleBar(page, "proj10: ターミナル 1"))).toHaveCount(0);
    await expect(attentionMark(titleBar(page, "proj11: ターミナル 1"))).toBeVisible();
    await expect(pageMark(previousButton(page))).toHaveCount(0);
    await expect(pageMark(nextButton(page))).toHaveCount(0);

    // p11 のタイルをクリックしてフォーカスすると、Rust はその印を消して返す。
    await titleBar(page, "proj11: ターミナル 1").click();
    await expect.poll(() => readCallArgs(page, "focus_terminal")).toEqual([{ terminalId: "p11-t1" }]);
    await emitEvent(
      page,
      "app-snapshot",
      withTileDisplay(projectsSnapshot(list), { focusedTerminalId: "p11-t1", attention: ["p11-t1"] }),
    );
    await expect(attentionMark(titleBar(page, "proj11: ターミナル 1"))).toHaveCount(0);
  });
});

test.describe("入力のフォーカス(FR-053)", () => {
  test("タイルをクリックするとそのターミナルに入力のフォーカスが移る", async ({ page }) => {
    await open(page, withTileDisplay(projectsSnapshot([A, B])));
    await settleFocus(page, "app: ターミナル 1");

    await titleBar(page, "ribbitlog: ターミナル 1").click();
    await expect.poll(() => readCallArgs(page, "focus_terminal")).toEqual([{ terminalId: "p-b-t1" }]);
    await emitEvent(page, "app-snapshot", withTileDisplay(projectsSnapshot([A, B]), { focusedTerminalId: "p-b-t1" }));
    await expect(tileInput(page, "ribbitlog: ターミナル 1")).toBeFocused();

    // 本文のクリックでは xterm.js 自身がフォーカスを取り、写しが届いてもそのまま。
    await tile(page, "app: ターミナル 2").getByTestId("terminal-host").click();
    await expect(tileInput(page, "app: ターミナル 2")).toBeFocused();
    await emitEvent(page, "app-snapshot", withTileDisplay(projectsSnapshot([A, B]), { focusedTerminalId: "p-a-t2" }));
    await page.waitForTimeout(SETTLE_MS);
    await expect(tileInput(page, "app: ターミナル 2")).toBeFocused();
  });

  test("ページ送り_カードの選択_表示モードの切替でフォーカス中のタイルに入力のフォーカスが移る", async ({ page }) => {
    const list = projects(10);
    await open(page, projectsSnapshot(list));
    await expect(terminalHosts(page).locator(".xterm-helper-textarea")).toBeFocused();

    // 表示モードの切替(ボタン)。
    await tilesButton(page).click();
    await emitEvent(page, "app-snapshot", withTileDisplay(projectsSnapshot(list)));
    await expect(tileInput(page, "proj1: ターミナル 1")).toBeFocused();

    // ページ送り(ボタン)。
    await nextButton(page).click();
    await emitEvent(page, "app-snapshot", withTileDisplay(projectsSnapshot(list), { page: 1 }));
    await expect(tileInput(page, "proj10: ターミナル 1")).toBeFocused();

    // カードの選択。
    await card(page, "proj3").click();
    await emitEvent(page, "app-snapshot", withTileDisplay(projectsSnapshot(list, { selectedProjectId: "p3" })));
    await expect(tileInput(page, "proj3: ターミナル 1")).toBeFocused();

    // タブ表示へ戻す。
    await tabsButton(page).click();
    await emitEvent(page, "app-snapshot", withTileDisplay(projectsSnapshot(list, { selectedProjectId: "p3" }), { mode: "tabs" }));
    await expect(terminalHosts(page)).toHaveAttribute("data-terminal-id", "p3-t1");
    await expect(terminalHosts(page).locator(".xterm-helper-textarea")).toBeFocused();
  });

  test("フォーカス中のタイルが同じなら写しが変わってもフォーカスを動かさない", async ({ page }) => {
    await open(page, withTileDisplay(projectsSnapshot([A, B, ...projects(9)])));
    await settleFocus(page, "app: ターミナル 1");
    await card(page, "ribbitlog").focus();

    // アテンションの印だけが変わった写し(フォーカス中のタイルは同じ)。
    await emitEvent(page, "app-snapshot", withTileDisplay(projectsSnapshot([A, B, ...projects(9)]), { attention: ["p9-t1"] }));
    await expect(pageMark(nextButton(page))).toBeVisible();
    await page.waitForTimeout(SETTLE_MS);
    await expect(card(page, "ribbitlog")).toBeFocused();
  });
});

// ── ツリーと組み合わせる(名前の入力中・メニュー・ツリーのキー操作、パスの挿入の宛先) ──

const TREE_ENTRIES = [
  ["docs", "directory"],
  ["README.md", "file"],
  ["a b.ts", "file"],
] as const;

function treeListings(): Record<string, MockListing> {
  return {
    [listingKey(A.id, "")]: listed(TREE_ENTRIES),
    [listingKey(A.id, "docs")]: listed([["a.md", "file"]]),
    [listingKey(B.id, "")]: listed([["log.txt", "file"]]),
  };
}

const treeRow = (page: Page, path: string) =>
  page.locator(`[data-testid="tree-row"][data-path=${JSON.stringify(path)}]`);
const treeRowPaths = (page: Page) =>
  page.getByTestId("tree-row").evaluateAll((els) => els.map((el) => el.getAttribute("data-path") ?? ""));
const contextMenu = (page: Page) => page.getByTestId("context-menu");
const menuItem = (page: Page, label: string) =>
  contextMenu(page).locator('[role^="menuitem"]').filter({
    has: page.locator(".context-menu__label", { hasText: new RegExp(`^${label.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}$`) }),
  });
const nameInput = (page: Page) => page.getByTestId("tree-name-input");
const fileTree = (page: Page) => page.getByTestId("file-tree");
const dragGhost = (page: Page) => page.getByTestId("tree-drag-ghost");
const dropActive = (page: Page) => page.locator('[data-drop-active="true"]');

async function openWithTree(page: Page, snapshot: AppSnapshot): Promise<void> {
  await open(page, snapshot, {}, treeListings());
  await expect(page.getByTestId("tree-row").first()).toBeVisible();
}

async function centerOf(locator: Locator): Promise<{ x: number; y: number }> {
  const b = await box(locator);
  return { x: b.x + b.width / 2, y: b.y + b.height / 2 };
}

/** 項目の名前の上で押し、しきい値(4px)を超えて動かしてドラッグを始める。 */
async function startDrag(page: Page, source: Locator): Promise<void> {
  const from = await centerOf(source.locator(".tree-row__name"));
  await page.mouse.move(from.x, from.y);
  await page.mouse.down();
  await page.mouse.move(from.x + 6, from.y + 6, { steps: 3 });
  await expect(dragGhost(page)).toBeVisible();
}

async function hoverOver(page: Page, target: Locator): Promise<void> {
  const to = await centerOf(target);
  await page.mouse.move(to.x, to.y, { steps: 4 });
}

test.describe("フォーカスを奪わない(M2 のレビュー)", () => {
  test("ツリーの名前の入力中にフォーカス中のタイルが替わっても入力欄からフォーカスを奪わない", async ({ page }) => {
    await openWithTree(page, withTileDisplay(projectsSnapshot([A, B])));
    await settleFocus(page, "app: ターミナル 1");
    await treeRow(page, "README.md").click({ button: "right" });
    await menuItem(page, "名称変更…").click();
    await expect(nameInput(page)).toBeFocused();

    // 同じプロジェクトの別のタブへ(⌘⇧] など。ツリーはそのまま)。
    await emitEvent(page, "app-snapshot", withTileDisplay(projectsSnapshot([A, B]), { focusedTerminalId: "p-a-t2" }));
    await expect(tile(page, "app: ターミナル 2")).toHaveAttribute("aria-current", "true");
    await page.waitForTimeout(SETTLE_MS);
    await expect(nameInput(page)).toBeFocused();
  });

  test("メニューの表示中にフォーカス中のタイルが替わってもメニューからフォーカスを奪わない", async ({ page }) => {
    await openWithTree(page, withTileDisplay(projectsSnapshot([A, B])));
    await settleFocus(page, "app: ターミナル 1");
    await treeRow(page, "README.md").click({ button: "right" });
    await expect(contextMenu(page)).toBeVisible();

    await emitEvent(page, "app-snapshot", withTileDisplay(projectsSnapshot([A, B]), { focusedTerminalId: "p-a-t2" }));
    await expect(tile(page, "app: ターミナル 2")).toHaveAttribute("aria-current", "true");
    await page.waitForTimeout(SETTLE_MS);
    await expect(contextMenu(page)).toBeVisible();
    expect(await page.evaluate(() => document.activeElement?.closest('[data-testid="context-menu"]') !== null)).toBe(true);
  });

  test("ツリーにフォーカスがあるときはフォーカス中のタイルが替わってもツリーから奪わない", async ({ page }) => {
    await openWithTree(page, withTileDisplay(projectsSnapshot([A, B])));
    await settleFocus(page, "app: ターミナル 1");
    await treeRow(page, "README.md").click();
    await expect(fileTree(page)).toBeFocused();

    await emitEvent(page, "app-snapshot", withTileDisplay(projectsSnapshot([A, B]), { focusedTerminalId: "p-a-t2" }));
    await expect(tile(page, "app: ターミナル 2")).toHaveAttribute("aria-current", "true");
    await page.waitForTimeout(SETTLE_MS);
    await expect(fileTree(page)).toBeFocused();
  });
});

test.describe("パスの挿入の宛先(FR-016・FR-024。M3 の申し送り)", () => {
  test("ツリーのパスをターミナルに挿入の宛先はフォーカス中のタイル", async ({ page }) => {
    // a の 2 枚目のタイルにフォーカス中(ツリーは a)。
    await openWithTree(page, withTileDisplay(projectsSnapshot([A, B]), { focusedTerminalId: "p-a-t2" }));
    await treeRow(page, "a b.ts").click({ button: "right" });
    await menuItem(page, "パスをターミナルに挿入").click();
    await expect
      .poll(() => readCallArgs(page, "insert_entry_path"))
      .toEqual([{ projectId: A.id, paths: ["a b.ts"], terminalId: "p-a-t2" }]);

    // b のタイルへフォーカスを移すと、ツリーは b になり、宛先は b のタイル。
    await emitEvent(page, "app-snapshot", withTileDisplay(projectsSnapshot([A, B]), { focusedTerminalId: "p-b-t1" }));
    await expect(page.getByTestId("tree-header")).toHaveText("ribbitlog");
    await expect(treeRow(page, "log.txt")).toBeVisible();
    await treeRow(page, "log.txt").click({ button: "right" });
    await menuItem(page, "パスをターミナルに挿入").click();
    await expect
      .poll(() => readCallArgs(page, "insert_entry_path"))
      .toEqual([
        { projectId: A.id, paths: ["a b.ts"], terminalId: "p-a-t2" },
        { projectId: B.id, paths: ["log.txt"], terminalId: "p-b-t1" },
      ]);
  });

  test("ツリーの項目を別のプロジェクトのタイルへ落とすとそのターミナルへの挿入が呼ばれる", async ({ page }) => {
    await openWithTree(page, withTileDisplay(projectsSnapshot([A, B])));
    await settleFocus(page, "app: ターミナル 1");
    await connectOf(page, "p-b-t1");

    await startDrag(page, treeRow(page, "a b.ts"));
    // 落とせる先の強調はタイルごと(上にあるタイルの置き場所だけ)。
    await hoverOver(page, tile(page, "app: ターミナル 2").getByTestId("terminal-host"));
    await expect(dropActive(page)).toHaveCount(1);
    await expect(dropActive(page)).toHaveAttribute("data-terminal-id", "p-a-t2");
    await hoverOver(page, tile(page, "ribbitlog: ターミナル 1").locator(".xterm-screen"));
    await expect(dropActive(page)).toHaveCount(1);
    await expect(dropActive(page)).toHaveAttribute("data-terminal-id", "p-b-t1");
    await page.mouse.up();

    // 宛先は落とした先のタイル(別のプロジェクトなので Rust が絶対パスにする。T42d の Rust のテスト)。
    await expect
      .poll(() => readCallArgs(page, "insert_entry_path"))
      .toEqual([{ projectId: A.id, paths: ["a b.ts"], terminalId: "p-b-t1" }]);
    await expect(dropActive(page)).toHaveCount(0);
    // 続けて入力できるよう、落としたタイルのターミナルに入力のフォーカスが移る。
    await expect(tileInput(page, "ribbitlog: ターミナル 1")).toBeFocused();
    // 挿入のあと、落とした先のタイルをフォーカス中にする(タイルのクリックと同じ。キー入力の行き先とフォーカス中のタイルをそろえる)。
    await expect.poll(() => readCallArgs(page, "focus_terminal")).toEqual([{ terminalId: "p-b-t1" }]);
    const calls = (await readRecordedCalls(page)).map((c) => c.command);
    expect(calls.indexOf("insert_entry_path")).toBeLessThan(calls.indexOf("focus_terminal"));

    // Rust が返す写し: b を選び、b のタイルがフォーカス中。カード・ツリー・タイルの輪がそろい、入力のフォーカスはそのまま。
    await emitEvent(page, "app-snapshot", withTileDisplay(projectsSnapshot([A, B]), { focusedTerminalId: "p-b-t1" }));
    await expect(tile(page, "ribbitlog: ターミナル 1")).toHaveAttribute("aria-current", "true");
    await expect(tile(page, "app: ターミナル 1")).not.toHaveAttribute("aria-current", "true");
    await expect(card(page, "ribbitlog")).toHaveAttribute("aria-current", "true");
    await expect(page.getByTestId("tree-header")).toHaveText("ribbitlog");
    await expect(tileInput(page, "ribbitlog: ターミナル 1")).toBeFocused();
  });

  test("ツリーの項目をフォーカス中のタイルへ落としてもフォーカスのコマンドを呼ばない", async ({ page }) => {
    await openWithTree(page, withTileDisplay(projectsSnapshot([A, B])));
    await settleFocus(page, "app: ターミナル 1");
    await connectOf(page, "p-a-t1");

    await startDrag(page, treeRow(page, "README.md"));
    await hoverOver(page, tile(page, "app: ターミナル 1").locator(".xterm-screen"));
    await expect(dropActive(page)).toHaveAttribute("data-terminal-id", "p-a-t1");
    await page.mouse.up();

    await expect
      .poll(() => readCallArgs(page, "insert_entry_path"))
      .toEqual([{ projectId: A.id, paths: ["README.md"], terminalId: "p-a-t1" }]);
    await expect(tileInput(page, "app: ターミナル 1")).toBeFocused();
    await page.waitForTimeout(SETTLE_MS);
    expect(await readCallArgs(page, "focus_terminal")).toEqual([]);
  });

  test("ドラッグ中にタイルの上を通っても行が動かない", async ({ page }) => {
    await openWithTree(page, withTileDisplay(projectsSnapshot([A, B])));
    const before = await treeRowPaths(page);

    await startDrag(page, treeRow(page, "README.md"));
    await hoverOver(page, tile(page, "app: ターミナル 2").getByTestId("terminal-host"));
    await hoverOver(page, tile(page, "ribbitlog: ターミナル 1").getByTestId("terminal-host"));
    // ドラッグ中に外からルートへ 100 件が足された(AC-T25)。
    const added = Array.from({ length: 100 }, (_, i) => [`new-${String(i).padStart(3, "0")}.ts`, "file"] as const);
    await setMockListing(page, listingKey(A.id, ""), listed([...TREE_ENTRIES, ...added]));
    await emitEvent(page, "tree-changed", { projectId: A.id, dirs: [""], fullReload: false });
    await page.waitForTimeout(300);
    expect(await treeRowPaths(page)).toEqual(before);
    // タイルの上を通っただけではフォーカスのコマンドを呼ばない。
    expect(await readCallArgs(page, "focus_terminal")).toEqual([]);

    await page.mouse.up();
    await expect(treeRow(page, "new-000.ts")).toBeVisible();
  });
});
