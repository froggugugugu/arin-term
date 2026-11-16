// M4.7 の画面側(ADR-014 の 2〜5。AC-G01〜G03・G05・G07・G09 のうち画面で見えるもの)。
//
// 偽の Rust 側(fixtures/tauriMock.ts)は写しを送り返さないので、step_translucency・reset_appearance の結果は、
// テストが withTranslucency・withSteppedTranslucency で作った写しを流して代える。⌘1〜9 は Rust がメニューで受けて写しを
// 送るだけなので(画面へのイベントは無い)、ここでは番号の札と、札がクリックを邪魔しないことだけを見る。
// 見た目は、設定画面とタイルの札をライトとダークで testreport/e2e/m47/ に撮る(人が目で確かめる)。

import { expect, type Locator, type Page, test } from "@playwright/test";

import type { AppSnapshot } from "../src/ipc/generated/AppSnapshot";
import {
  emitEvent,
  installTauriMock,
  projectsSnapshot,
  readCallArgs,
  readRecordedCalls,
  type SampleProject,
  withFontSizes,
  withSteppedTranslucency,
  withTileDisplay,
  withTranslucency,
} from "./fixtures/tauriMock";

const APP: SampleProject = { id: "p-app", name: "app", color: "green", tabs: ["ターミナル 1"] };

/** 1 枚ずつタブを持つプロジェクトを n 件。ターミナルの ID は `p<番号>-t1`。 */
function projects(n: number): SampleProject[] {
  return Array.from({ length: n }, (_, i) => ({ id: `p${i + 1}`, name: `proj${i + 1}`, tabs: ["ターミナル 1"] }));
}

async function open(page: Page, snapshot: AppSnapshot): Promise<void> {
  await installTauriMock(page, { get_app_snapshot: { ok: snapshot } });
  await page.goto("/");
  await expect
    .poll(async () => (await readRecordedCalls(page)).map((call) => call.command))
    .toContain("notify_app_ready");
}

/** 画像の置き場所(testreport/ は .gitignore 済み)。 */
function shotPath(name: string): string {
  const configFile = test.info().config.configFile;
  if (configFile === undefined) throw new Error("Playwright の設定ファイルの場所が分からない");
  return `${configFile.slice(0, configFile.lastIndexOf("/"))}/testreport/e2e/m47/${name}.png`;
}

const dialog = (page: Page) => page.getByRole("dialog", { name: "設定" });
const row = (page: Page, target: "window" | "terminal") => page.getByTestId(`translucency-row-${target}`);
const stepper = (page: Page, target: "window" | "terminal") => row(page, target).getByRole("spinbutton");
const upButton = (page: Page, target: "window" | "terminal") =>
  row(page, target).getByRole("button", { name: "透過を強くする" });
const downButton = (page: Page, target: "window" | "terminal") =>
  row(page, target).getByRole("button", { name: "透過を弱くする" });
const resetButton = (page: Page) => dialog(page).getByRole("button", { name: "既定に戻す" });
const hostOf = (page: Page, id: string) => page.locator(`[data-testid="terminal-host"][data-terminal-id="${id}"]`);
const terminalInput = (page: Page, id: string) => hostOf(page, id).locator(".xterm-helper-textarea");
const tile = (page: Page, title: string) => page.getByRole("group", { name: title, exact: true });
const background = (locator: Locator) => locator.evaluate((el) => getComputedStyle(el).backgroundColor);

async function openSettings(page: Page): Promise<void> {
  await emitEvent(page, "menu-command", "open-settings");
  await expect(dialog(page)).toBeVisible();
}

/**
 * xterm.js の要素(置き場所の中)のうち、地を塗っているもの。カーソル(色付きのセルと同じく xterm.js が塗る)、
 * スクロールバーのつまみ、見えない要素(IME の変換の表示、透明で大きさ 0 の入力欄)は除く。
 */
async function paintedXtermElements(host: Locator): Promise<string[]> {
  return host.evaluate((root) => {
    const painted: string[] = [];
    for (const el of [root, ...root.querySelectorAll("*")]) {
      const style = getComputedStyle(el);
      const rect = el.getBoundingClientRect();
      const invisible = style.display === "none" || style.opacity === "0" || rect.width === 0 || rect.height === 0;
      if (invisible || el.closest("[class*='xterm-cursor'], .scrollbar") !== null) continue;
      if (style.backgroundColor !== "rgba(0, 0, 0, 0)" || style.backgroundImage !== "none") {
        painted.push(`${el.tagName.toLowerCase()}.${[...el.classList].join(".")} ${style.backgroundColor}`);
      }
    }
    return painted;
  });
}

// 各テストの終わりに、画面の失敗(例外、通信の失敗の報告)が無かったことを見る。
test.afterEach(async ({ page }) => {
  const reported = await readCallArgs(page, "report_frontend_error");
  expect(reported.map((r) => r.message), "画面の失敗が Rust へ報告された").toEqual([]);
});

test.describe("設定画面の透過(AC-G01・AC-G05)", () => {
  test("透過の_2_行が文字の大きさの下にあり_値は_45%_と_20%", async ({ page }) => {
    await open(page, projectsSnapshot([APP]));
    await openSettings(page);

    const groups = dialog(page).getByRole("heading", { level: 3 });
    await expect(groups).toHaveText(["文字の大きさ", "透過", "ショートカット"]);
    await expect(row(page, "window")).toContainText("ウィンドウ");
    await expect(row(page, "terminal")).toContainText("ターミナルの背景");
    await expect(stepper(page, "window")).toHaveText("45%");
    await expect(stepper(page, "terminal")).toHaveText("20%");
    await expect(stepper(page, "window")).toHaveAttribute("aria-valuetext", "45 パーセント");
    await expect(stepper(page, "window")).toHaveAccessibleName("ウィンドウの透過");
    await expect(stepper(page, "terminal")).toHaveAccessibleName("ターミナルの背景の透過");
    await expect(stepper(page, "terminal")).toHaveAttribute("aria-valuemax", "80");
  });

  test("▲▼_と_↑↓_は_5%_ずつ_Home・End_は範囲の幅ぶんの差分を送り_写しで値が変わる", async ({ page }) => {
    let snapshot = projectsSnapshot([APP]);
    await open(page, snapshot);
    await openSettings(page);

    await upButton(page, "window").click();
    await downButton(page, "terminal").click();
    await stepper(page, "window").focus();
    await page.keyboard.press("ArrowUp");
    await page.keyboard.press("ArrowDown");
    await page.keyboard.press("Home");
    await stepper(page, "terminal").focus();
    await page.keyboard.press("End");

    await expect
      .poll(() => readCallArgs(page, "step_translucency"))
      .toEqual([
        { target: "window", delta: 5 },
        { target: "terminal", delta: -5 },
        { target: "window", delta: 5 },
        { target: "window", delta: -5 },
        { target: "window", delta: -100 },
        { target: "terminal", delta: 80 },
      ]);
    // 値は写しが届くまで変えない
    await expect(stepper(page, "window")).toHaveText("45%");

    snapshot = withSteppedTranslucency(snapshot, "window", 5);
    await emitEvent(page, "app-snapshot", snapshot);
    await expect(stepper(page, "window")).toHaveText("50%");
    await expect(stepper(page, "window")).toHaveAttribute("aria-valuenow", "50");
    // ▲ を押してもフォーカスは値に残る
    await upButton(page, "terminal").click();
    await expect(stepper(page, "terminal")).toBeFocused();
  });

  test("範囲の端では_▲_か_▼_が無効になりキーでも送らない", async ({ page }) => {
    const base = projectsSnapshot([APP]);
    await open(page, withTranslucency(base, { window: 100, terminal: 0 }));
    await openSettings(page);

    await expect(stepper(page, "window")).toHaveText("100%");
    await expect(upButton(page, "window")).toBeDisabled();
    await expect(downButton(page, "window")).toBeEnabled();
    await expect(downButton(page, "terminal")).toBeDisabled();
    await expect(upButton(page, "terminal")).toBeEnabled();
    await stepper(page, "window").focus();
    await page.keyboard.press("ArrowUp");
    await page.keyboard.press("End");
    await stepper(page, "terminal").focus();
    await page.keyboard.press("ArrowDown");
    await page.keyboard.press("Home");
    expect(await readCallArgs(page, "step_translucency")).toEqual([]);

    // ターミナルの上限は 80
    await emitEvent(page, "app-snapshot", withTranslucency(base, { terminal: 80 }));
    await expect(stepper(page, "terminal")).toHaveText("80%");
    await expect(upButton(page, "terminal")).toBeDisabled();
  });

  test("既定に戻すは_4_つすべて既定のとき無効で_透過だけ変えても押せて_reset_appearance_を呼ぶ", async ({ page }) => {
    const base = projectsSnapshot([APP]);
    await open(page, base);
    await openSettings(page);
    await expect(resetButton(page)).toBeDisabled();

    for (const changed of [
      withTranslucency(base, { window: 50 }),
      withTranslucency(base, { terminal: 0 }),
      withFontSizes(base, { terminal: 16 }),
      withFontSizes(base, { interface: 12 }),
    ]) {
      await emitEvent(page, "app-snapshot", changed);
      await expect(resetButton(page)).toBeEnabled();
      await emitEvent(page, "app-snapshot", base);
      await expect(resetButton(page)).toBeDisabled();
    }

    await emitEvent(page, "app-snapshot", withFontSizes(withTranslucency(base, { window: 0, terminal: 80 }), { terminal: 20 }));
    await resetButton(page).click();
    await expect.poll(() => readCallArgs(page, "reset_appearance")).toEqual([{}]);
    expect(await readCallArgs(page, "step_translucency")).toEqual([]);

    // 既定の写しが届くと 4 つとも既定に戻り、無効になる
    await emitEvent(page, "app-snapshot", base);
    await expect(stepper(page, "window")).toHaveText("45%");
    await expect(stepper(page, "terminal")).toHaveText("20%");
    await expect(resetButton(page)).toBeDisabled();
  });
});

test.describe("ウィンドウの地(AC-G02)", () => {
  const cases = [
    { colorScheme: "light", rgb: "238, 238, 240", at: { 45: "0.55", 0: null, 60: "0.4", 100: "0" } },
    { colorScheme: "dark", rgb: "30, 30, 32", at: { 45: "0.5", 0: "0.95", 60: "0.35", 100: "0" } },
  ] as const;
  for (const { colorScheme, rgb, at } of cases) {
    test(`透過の写しで_--color-window-bg_の不透明度がすぐ変わる(${colorScheme})`, async ({ page }) => {
      await page.emulateMedia({ colorScheme });
      const base = projectsSnapshot([APP]);
      await open(page, base);
      const shell = page.getByTestId("app-shell");
      const expected = (alpha: string | null) => (alpha === null ? `rgb(${rgb})` : `rgba(${rgb}, ${alpha})`);

      // 既定 45 で M4.6 と同じ値(ライト 0.55・ダーク 0.5)
      await expect.poll(() => background(shell)).toBe(expected(at[45]));
      for (const value of [0, 60, 100] as const) {
        await emitEvent(page, "app-snapshot", withTranslucency(base, { window: value }));
        await expect.poll(() => background(shell)).toBe(expected(at[value]));
      }
    });
  }
});

test.describe("ターミナルの地(AC-G03)", () => {
  for (const colorScheme of ["light", "dark"] as const) {
    test(`タブ表示とタイル表示で_xterm_js_の地は透明で_置き場所の黒の不透明度が透過で変わる(${colorScheme})`, async ({
      page,
    }) => {
      await page.emulateMedia({ colorScheme });
      const tabs = projectsSnapshot([APP]);
      await open(page, tabs);
      const host = hostOf(page, "p-app-t1");
      await expect(host.locator(".xterm-rows")).toHaveCount(1);
      const panel = page.getByRole("tabpanel");

      // xterm.js は自分の要素に地を塗らない(実際の DOM で確かめる)
      expect(await paintedXtermElements(host)).toEqual([]);
      await expect.poll(() => background(panel)).toBe("rgba(0, 0, 0, 0.8)");
      await emitEvent(page, "app-snapshot", withTranslucency(tabs, { terminal: 0 }));
      await expect.poll(() => background(panel)).toBe("rgb(0, 0, 0)");
      await emitEvent(page, "app-snapshot", withTranslucency(tabs, { terminal: 80 }));
      await expect.poll(() => background(panel)).toBe("rgba(0, 0, 0, 0.2)");
      expect(await paintedXtermElements(host)).toEqual([]);

      // タイル表示でも同じ(地を塗るのはタイルの本文の 1 段だけ)
      const tiles = withTranslucency(withTileDisplay(projectsSnapshot(projects(4))), { terminal: 35 });
      await emitEvent(page, "app-snapshot", tiles);
      const body = tile(page, "proj2: ターミナル 1").locator(".tile__body");
      await expect(body).toBeVisible();
      await expect(hostOf(page, "p2-t1").locator(".xterm-rows")).toHaveCount(1);
      await expect.poll(() => background(body)).toBe("rgba(0, 0, 0, 0.65)");
      expect(await background(hostOf(page, "p2-t1"))).toBe("rgba(0, 0, 0, 0)");
      expect(await paintedXtermElements(hostOf(page, "p2-t1"))).toEqual([]);

      // 値を変えてもシェルは続く(つなぎ直さない・閉じない)
      const connects = (await readCallArgs(page, "connect_terminal")).filter((c) => c.terminalId === "p2-t1");
      expect(connects).toHaveLength(1);
      expect(await readCallArgs(page, "close_terminal")).toEqual([]);
    });
  }
});

test.describe("タイルの番号の札(AC-G07)", () => {
  test("表示中のページの並び順で_#1〜_が出て_ページを送ると付け直る", async ({ page }) => {
    const base = projectsSnapshot(projects(11));
    await open(page, withTileDisplay(base));

    const numbers = page.getByTestId("tile-number");
    await expect(numbers).toHaveText(["#1", "#2", "#3", "#4", "#5", "#6", "#7", "#8", "#9"]);
    // 並びは写しの tileIds の順(左上が 1)
    await expect(tile(page, "proj1: ターミナル 1").getByTestId("tile-number")).toHaveText("#1");
    await expect(tile(page, "proj9: ターミナル 1").getByTestId("tile-number")).toHaveText("#9");
    // 読み上げない・押下を受けない
    await expect(numbers.first()).toHaveAttribute("aria-hidden", "true");
    expect(await numbers.first().evaluate((el) => getComputedStyle(el).pointerEvents)).toBe("none");
    // 札はターミナルの置き場所の右上(タイトルバーの下、本文の右端寄り)
    const badge = await numbers.first().boundingBox();
    const body = await tile(page, "proj1: ターミナル 1").locator(".tile__body").boundingBox();
    if (badge === null || body === null) throw new Error("寸法が取れない");
    expect(badge.y).toBeGreaterThanOrEqual(body.y);
    expect(badge.y + badge.height).toBeLessThan(body.y + body.height / 4);
    expect(body.x + body.width - (badge.x + badge.width)).toBeLessThan(16);

    // 2 ページ目は 1 から付け直す
    await emitEvent(page, "app-snapshot", withTileDisplay(base, { page: 1 }));
    await expect(numbers).toHaveText(["#1", "#2"]);
    await expect(tile(page, "proj10: ターミナル 1").getByTestId("tile-number")).toHaveText("#1");
  });

  test("札の上のクリックはターミナルに届きそのタイルにフォーカスする", async ({ page }) => {
    await open(page, withTileDisplay(projectsSnapshot(projects(4))));
    const target = tile(page, "proj3: ターミナル 1");
    const badge = target.getByTestId("tile-number");
    await expect(badge).toHaveText("#3");
    await expect(hostOf(page, "p3-t1").locator(".xterm-screen")).toHaveCount(1);
    const box = await badge.boundingBox();
    if (box === null) throw new Error("札の寸法が取れない");
    const center = { x: box.x + box.width / 2, y: box.y + box.height / 2 };

    // その点で押下を受けるのは札ではなくターミナル(xterm.js の要素)
    const hit = await page.evaluate(
      ({ x, y }) => {
        const el = document.elementFromPoint(x, y);
        return { badge: el?.closest("[data-testid='tile-number']") !== null, terminal: el?.closest(".xterm") !== null };
      },
      center,
    );
    expect(hit).toEqual({ badge: false, terminal: true });

    await page.mouse.click(center.x, center.y);
    await expect.poll(async () => (await readCallArgs(page, "focus_terminal")).map((c) => c.terminalId)).toEqual(["p3-t1"]);
    await expect(terminalInput(page, "p3-t1")).toBeFocused();
  });

  test("拡大中とタブ表示では札を出さない", async ({ page }) => {
    const base = projectsSnapshot(projects(4));
    await open(page, withTileDisplay(base));
    await expect(page.getByTestId("tile-number")).toHaveCount(4);

    await emitEvent(page, "app-snapshot", withTileDisplay(base, { maximized: true }));
    await expect(page.getByTestId("maximized-panel")).toBeVisible();
    await expect(page.getByTestId("tile-number")).toHaveCount(0);

    await emitEvent(page, "app-snapshot", withTileDisplay(base, { mode: "tabs" }));
    await expect(page.getByRole("tabpanel")).toBeVisible();
    await expect(page.getByTestId("tile-number")).toHaveCount(0);

    // タイル表示に戻すとまた出る
    await emitEvent(page, "app-snapshot", withTileDisplay(base));
    await expect(page.getByTestId("tile-number")).toHaveCount(4);
  });
});

test.describe("Shift+Enter の改行(AC-G09)", () => {
  /** そのターミナルへ送った文字(write_terminal の順)。 */
  async function written(page: Page, id: string): Promise<string[]> {
    return (await readCallArgs(page, "write_terminal")).filter((w) => w.terminalId === id).map((w) => w.data);
  }

  test("Shift+Enter_は_ESC_CR_を送り_Enter_は_CR_のまま", async ({ page }) => {
    await open(page, projectsSnapshot([APP]));
    await terminalInput(page, "p-app-t1").focus();

    await page.keyboard.press("Shift+Enter");
    await expect.poll(() => written(page, "p-app-t1")).toEqual(["\x1b\r"]);
    await page.keyboard.press("Enter");
    await expect.poll(() => written(page, "p-app-t1")).toEqual(["\x1b\r", "\r"]);
    await page.keyboard.type("a");
    await page.keyboard.press("Shift+Enter");
    await expect.poll(() => written(page, "p-app-t1")).toEqual(["\x1b\r", "\r", "a", "\x1b\r"]);
  });

  test("修飾キーつきの_Shift+Enter_は対象外(⌘⇧Enter_はメニューの項目に任せ_ターミナルに送らない)", async ({ page }) => {
    await open(page, projectsSnapshot([APP]));
    await terminalInput(page, "p-app-t1").focus();

    await page.keyboard.press("Meta+Shift+Enter");
    await page.keyboard.press("Control+Shift+Enter");
    await page.keyboard.press("Enter");
    // ⌘⇧Enter は何も送らず、⌃⇧Enter は xterm.js の既定(CR)のまま。ESC CR はどれも送らない
    await expect.poll(() => written(page, "p-app-t1")).toEqual(["\r", "\r"]);
  });

  test("タイル表示でもフォーカス中のタイルに_ESC_CR_を送る", async ({ page }) => {
    await open(page, withTileDisplay(projectsSnapshot(projects(2)), { focusedTerminalId: "p2-t1" }));
    await terminalInput(page, "p2-t1").focus();

    await page.keyboard.press("Shift+Enter");
    await expect.poll(() => written(page, "p2-t1")).toEqual(["\x1b\r"]);
    expect(await written(page, "p1-t1")).toEqual([]);
  });
});

test.describe("見た目(人が目で確かめる)", () => {
  for (const colorScheme of ["light", "dark"] as const) {
    test(`設定画面とタイルの札を撮る(${colorScheme})`, async ({ page }) => {
      await page.setViewportSize({ width: 1100, height: 720 });
      await page.emulateMedia({ colorScheme });
      const tiles = withTileDisplay(projectsSnapshot(projects(6)), { focusedTerminalId: "p2-t1" });
      await open(page, tiles);
      await expect(page.getByTestId("tile-number")).toHaveCount(6);
      await expect(hostOf(page, "p6-t1").locator(".xterm-rows")).toHaveCount(1);
      await page.screenshot({ path: shotPath(`tiles-${colorScheme}`) });

      await openSettings(page);
      await expect(stepper(page, "window")).toHaveText("45%");
      await page.screenshot({ path: shotPath(`settings-${colorScheme}`) });

      await emitEvent(page, "app-snapshot", withTranslucency(tiles, { window: 70, terminal: 60 }));
      await expect(stepper(page, "terminal")).toHaveText("60%");
      await page.screenshot({ path: shotPath(`settings-translucent-${colorScheme}`) });
    });
  }
});
