// タイル表示(T47f): タイルのグリッド、タイトルバー、ページ送り、表示モードの切替の部品。
//
// 偽の Rust 側(fixtures/tauriMock.ts)に写しを渡して画面を開き、部品が正しいコマンドを呼ぶこと、写しのとおりに描くこと、
// レイアウトが崩れていないこと(要素の位置と大きさ)を見る。偽物は写しを送り返さないので、コマンドの結果は
// テストが withTileDisplay(Rust の規則の写し)で作った写しを流して代える。
// 拡大・アテンションの印・入力のフォーカスの移し替え・挿入の宛先は T47g。

import { expect, type Locator, type Page, test } from "@playwright/test";

import type { AppSnapshot } from "../src/ipc/generated/AppSnapshot";
import type { ProjectColor } from "../src/ipc/generated/ProjectColor";
import type { ShortcutBinding } from "../src/ipc/generated/ShortcutBinding";
import { DEFAULT_SHORTCUT_TABLE } from "../src/logic/keys/defaultShortcuts";
import {
  emitEvent,
  installTauriMock,
  parseChannelId,
  projectsSnapshot,
  readCallArgs,
  readRecordedCalls,
  type SampleProject,
  sendChannelBytes,
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
async function open(page: Page, snapshot: AppSnapshot, overrides: Partial<TauriMockConfig> = {}): Promise<void> {
  await installTauriMock(page, { get_app_snapshot: { ok: snapshot }, ...overrides });
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
