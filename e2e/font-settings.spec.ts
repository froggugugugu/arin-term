// 設定画面(T70b。ADR-013 の 5、AC-F01〜F07・F10・F12・F13)。
//
// 偽の Rust 側(fixtures/tauriMock.ts)に写しを渡して画面を開き、⌘, とメニューの「設定…」の代わりに menu-command の
// "open-settings" を emitEvent で流す(本物の Rust はどちらもこの操作を送る)。偽物は写しを送り返さないので、
// step_font_size・reset_appearance の結果は、テストが withFontSizes で作った写しを流して代える。
// ターミナルへの当て方(T69a)は WebKit の実物の xterm.js で、行・桁と resize_terminal を見る(AC-F02・F03)。
// 見た目は、ライトとダーク × 既定の値・端の値(ステッパーが無効)・画面の大きさ 18 の画像を testreport/e2e/font-settings/ に撮る
// (人が目で確かめる。AC-F13)。

import { expect, type Locator, type Page, test } from "@playwright/test";

import type { AppSnapshot } from "../src/ipc/generated/AppSnapshot";
import { interfaceMetrics } from "../src/logic/appearance/interfaceMetrics";
import {
  emitEvent,
  installTauriMock,
  listed,
  listingKey,
  type MockListing,
  projectsSnapshot,
  readCallArgs,
  readRecordedCalls,
  type SampleProject,
  withFontSizes,
  withTileDisplay,
} from "./fixtures/tauriMock";

const APP: SampleProject = { id: "p-app", name: "app", color: "green", tabs: ["ターミナル 1", "ターミナル 2"] };
const LOG: SampleProject = { id: "p-log", name: "ribbitlog", color: "orange", tabs: ["ターミナル 1"] };

const LISTINGS: Record<string, MockListing> = {
  [listingKey(APP.id, "")]: listed([
    ["src", "directory"],
    ["package.json", "file"],
    ["README.md", "file"],
  ]),
};

/** 1 枚ずつタブを持つプロジェクトを n 件。ターミナルの ID は `p<番号>-t1`。 */
function projects(n: number): SampleProject[] {
  return Array.from({ length: n }, (_, i) => ({ id: `p${i + 1}`, name: `proj${i + 1}`, tabs: ["ターミナル 1"] }));
}

async function open(page: Page, snapshot: AppSnapshot): Promise<void> {
  await installTauriMock(page, { get_app_snapshot: { ok: snapshot } }, LISTINGS);
  await page.goto("/");
  await expect
    .poll(async () => (await readRecordedCalls(page)).map((call) => call.command))
    .toContain("notify_app_ready");
}

/** 画像の置き場所(testreport/ は .gitignore 済み)。 */
function shotPath(name: string): string {
  const configFile = test.info().config.configFile;
  if (configFile === undefined) throw new Error("Playwright の設定ファイルの場所が分からない");
  return `${configFile.slice(0, configFile.lastIndexOf("/"))}/testreport/e2e/font-settings/${name}.png`;
}

const dialog = (page: Page) => page.getByRole("dialog", { name: "設定" });
const terminalStepper = (page: Page) => page.getByRole("spinbutton", { name: "ターミナルの文字の大きさ" });
const interfaceStepper = (page: Page) => page.getByRole("spinbutton", { name: "画面の文字の大きさ" });
/** ステッパーの行(▲▼ のボタンを行の中で探す)。 */
const stepperRow = (page: Page, target: "terminal" | "interface") => page.getByTestId(`font-size-row-${target}`);
const upButton = (page: Page, target: "terminal" | "interface") =>
  stepperRow(page, target).getByRole("button", { name: "大きくする" });
const downButton = (page: Page, target: "terminal" | "interface") =>
  stepperRow(page, target).getByRole("button", { name: "小さくする" });
const resetButton = (page: Page) => dialog(page).getByRole("button", { name: "既定に戻す" });
const doneButton = (page: Page) => dialog(page).getByRole("button", { name: "完了" });
const openFileButton = (page: Page) => dialog(page).getByRole("button", { name: "設定ファイルを開く…" });
const terminalInput = (page: Page, id: string) =>
  page.locator(`[data-testid="terminal-host"][data-terminal-id="${id}"] .xterm-helper-textarea`);
const xtermRows = (page: Page, id: string) =>
  page.locator(`[data-testid="terminal-host"][data-terminal-id="${id}"] .xterm-rows`);

async function openSettings(page: Page): Promise<void> {
  await emitEvent(page, "menu-command", "open-settings");
  await expect(dialog(page)).toBeVisible();
}

/** ターミナルに最後に送った寸法(接続の寸法か、そのあとの寸法の変更)。 */
async function lastSize(page: Page, terminalId: string): Promise<{ cols: number; rows: number } | null> {
  const resized = (await readCallArgs(page, "resize_terminal")).filter((r) => r.terminalId === terminalId);
  const last = resized.at(-1);
  if (last !== undefined) return { cols: last.cols, rows: last.rows };
  const connected = (await readCallArgs(page, "connect_terminal")).find((c) => c.terminalId === terminalId);
  return connected === undefined ? null : { cols: connected.cols, rows: connected.rows };
}

async function connectCount(page: Page, terminalId: string): Promise<number> {
  return (await readCallArgs(page, "connect_terminal")).filter((c) => c.terminalId === terminalId).length;
}

const fontSizeOf = (locator: Locator) => locator.evaluate((el) => getComputedStyle(el).fontSize);
const heightOf = (locator: Locator) => locator.evaluate((el) => el.getBoundingClientRect().height);

// 各テストの終わりに、画面の失敗(例外、通信の失敗の報告)が無かったことを見る。
test.afterEach(async ({ page }) => {
  const reported = await readCallArgs(page, "report_frontend_error");
  expect(reported.map((r) => r.message), "画面の失敗が Rust へ報告された").toEqual([]);
});

test.describe("開閉とキーボード(AC-F01・AC-F12)", () => {
  test("cmd_カンマのメニューの操作で設定画面が開きターミナルのステッパーにフォーカスがある", async ({ page }) => {
    await open(page, projectsSnapshot([APP]));
    await terminalInput(page, "p-app-t1").focus();
    await expect(dialog(page)).toHaveCount(0);

    await openSettings(page);
    await expect(terminalStepper(page)).toBeFocused();
    // 開いているあいだにもう一度届いても 1 つのまま(フォーカスも動かない)
    await interfaceStepper(page).focus();
    await emitEvent(page, "menu-command", "open-settings");
    await expect(page.getByRole("dialog")).toHaveCount(1);
    await expect(interfaceStepper(page)).toBeFocused();
    // ターミナルには何も書かない(⌘, はターミナルに渡らない)
    expect(await readCallArgs(page, "write_terminal")).toEqual([]);
  });

  test("上矢印で_step_font_size_が_1_で呼ばれ届いた写しで値が変わる", async ({ page }) => {
    const base = projectsSnapshot([APP]);
    await open(page, base);
    await openSettings(page);
    await expect(terminalStepper(page)).toHaveAttribute("aria-valuenow", "15");

    await page.keyboard.press("ArrowUp");
    await expect.poll(() => readCallArgs(page, "step_font_size")).toEqual([{ target: "terminal", delta: 1 }]);
    // 写しが届くまで値は変えない(正本は Rust)
    await expect(terminalStepper(page)).toHaveAttribute("aria-valuenow", "15");

    await emitEvent(page, "app-snapshot", withFontSizes(base, { terminal: 16 }));
    await expect(terminalStepper(page)).toHaveAttribute("aria-valuenow", "16");
    await expect(terminalStepper(page)).toHaveText("16 pt");

    // ↓・▲・▼・Home・End も差分で送る(Home・End は範囲の幅ぶんの差分。端に留めるのは Rust)
    await page.keyboard.press("ArrowDown");
    await upButton(page, "interface").click();
    await downButton(page, "interface").click();
    await terminalStepper(page).focus();
    await page.keyboard.press("Home");
    await page.keyboard.press("End");
    await expect
      .poll(() => readCallArgs(page, "step_font_size"))
      .toEqual([
        { target: "terminal", delta: 1 },
        { target: "terminal", delta: -1 },
        { target: "interface", delta: 1 },
        { target: "interface", delta: -1 },
        { target: "terminal", delta: -(28 - 9) },
        { target: "terminal", delta: 28 - 9 },
      ]);
    // ▲▼ のボタンを押してもフォーカスはステッパーに残る
    await upButton(page, "interface").click();
    await expect(interfaceStepper(page)).toBeFocused();
  });

  test("tab_が設定画面の中を回り_esc_で閉じてターミナルにフォーカスが戻る", async ({ page }) => {
    // 既定に戻すを押せる状態(既定でない値)で回る順を見る
    await open(page, withFontSizes(projectsSnapshot([APP]), { terminal: 18 }));
    await terminalInput(page, "p-app-t1").focus();
    await openSettings(page);

    // M4.7 で透過の 2 つのステッパーが文字の大きさの下に入った(ADR-014 の 3)
    const order = [
      terminalStepper(page),
      interfaceStepper(page),
      // テーマの切替(選ばれている 1 つだけが Tab に止まる。ADR-016)
      dialog(page).getByRole("radio", { name: "システム", exact: true }),
      page.getByRole("spinbutton", { name: "ウィンドウの透過" }),
      page.getByRole("spinbutton", { name: "ターミナルの背景の透過" }),
      // M4.8 で Vim のキー操作の切替が透過の下に入った(ADR-015 の 6)
      dialog(page).getByRole("switch", { name: "Vim のキー操作" }),
      openFileButton(page),
      resetButton(page),
      doneButton(page),
    ];
    for (const next of [...order.slice(1), order[0]]) {
      await page.keyboard.press("Tab");
      await expect(next as Locator).toBeFocused();
    }
    await page.keyboard.press("Shift+Tab");
    await expect(doneButton(page)).toBeFocused();
    await page.keyboard.press("Shift+Tab");
    await expect(resetButton(page)).toBeFocused();

    await page.keyboard.press("Escape");
    await expect(dialog(page)).toHaveCount(0);
    await expect(terminalInput(page, "p-app-t1")).toBeFocused();
  });

  test("return_と完了と_cmd_w_で閉じ_ボタンの上の_return_はそのボタンを押す", async ({ page }) => {
    await open(page, projectsSnapshot([APP]));
    await terminalInput(page, "p-app-t1").focus();

    // ステッパーの上の Return で閉じる
    await openSettings(page);
    await page.keyboard.press("Enter");
    await expect(dialog(page)).toHaveCount(0);
    await expect(terminalInput(page, "p-app-t1")).toBeFocused();

    // 「完了」で閉じる
    await openSettings(page);
    await doneButton(page).click();
    await expect(dialog(page)).toHaveCount(0);
    await expect(terminalInput(page, "p-app-t1")).toBeFocused();

    // ⌘W(表の terminal.close-tab のキー)は設定画面だけを閉じる。画面の keydown は取り消す(タブは閉じない)
    await openSettings(page);
    const prevented = await dialog(page).evaluate((el) => {
      const event = new KeyboardEvent("keydown", { key: "w", code: "KeyW", metaKey: true, bubbles: true, cancelable: true });
      (el.querySelector("[role=spinbutton]") ?? el).dispatchEvent(event);
      return event.defaultPrevented;
    });
    expect(prevented).toBe(true);
    await expect(dialog(page)).toHaveCount(0);
    expect(await readCallArgs(page, "close_terminal")).toEqual([]);

    // ボタンにフォーカスがあるときの Return はそのボタンを押す(閉じない)
    await openSettings(page);
    await openFileButton(page).focus();
    await page.keyboard.press("Enter");
    await expect.poll(() => readCallArgs(page, "open_settings_file")).toHaveLength(1);
    await expect(dialog(page)).toBeVisible();
  });

  test("設定ファイルを開くで_open_settings_file_が呼ばれる", async ({ page }) => {
    await open(page, projectsSnapshot([APP]));
    await openSettings(page);

    await openFileButton(page).click();

    // 引数なし(@tauri-apps/api は空の引数を {} で送る)
    await expect.poll(() => readCallArgs(page, "open_settings_file")).toEqual([{}]);
    // 設定画面は開いたまま(設定ファイルは別のアプリで開く)
    await expect(dialog(page)).toBeVisible();
    expect(await readCallArgs(page, "step_font_size")).toEqual([]);
  });

  test("ダイアログとステッパーの名前と値が読み上げ用に付く", async ({ page }) => {
    await open(page, withFontSizes(projectsSnapshot([APP]), { terminal: 20, interface: 14 }));
    await openSettings(page);

    await expect(dialog(page)).toHaveAttribute("aria-modal", "true");
    const expectations: ReadonlyArray<readonly [Locator, string, string, string]> = [
      [terminalStepper(page), "20", "9", "28"],
      [interfaceStepper(page), "14", "11", "18"],
    ];
    for (const [stepper, now, min, max] of expectations) {
      await expect(stepper).toHaveAttribute("aria-valuenow", now);
      await expect(stepper).toHaveAttribute("aria-valuemin", min);
      await expect(stepper).toHaveAttribute("aria-valuemax", max);
      await expect(stepper).toHaveAttribute("aria-valuetext", `${now} ポイント`);
      await expect(stepper).toHaveAttribute("tabindex", "0");
    }
    // ▲▼ はマウス用で Tab に止めない
    for (const target of ["terminal", "interface"] as const) {
      await expect(upButton(page, target)).toHaveAttribute("tabindex", "-1");
      await expect(downButton(page, target)).toHaveAttribute("tabindex", "-1");
    }
    await expect(stepperRow(page, "interface")).toContainText("サイドバー・タブ・ツリー・メニューの文字");
  });
});

test.describe("テーマの選び方(ADR-016)", () => {
  const themeSwitch = (page: Page) => page.getByTestId("theme-switch");
  const themeRadio = (page: Page, name: string) => themeSwitch(page).getByRole("radio", { name, exact: true });

  test("システム_ライト_ダークが並び_選ぶと_set_theme_が呼ばれ届いた写しで選択が替わる", async ({ page }) => {
    const base = projectsSnapshot([APP]);
    await open(page, base);
    await openSettings(page);

    await expect(themeSwitch(page)).toHaveAccessibleName("テーマ");
    await expect(themeSwitch(page).getByRole("radio")).toHaveText(["システム", "ライト", "ダーク"]);
    await expect(themeRadio(page, "システム")).toHaveAttribute("aria-checked", "true");

    await themeRadio(page, "ダーク").click();
    await expect.poll(() => readCallArgs(page, "set_theme")).toEqual([{ theme: "dark" }]);
    // 表示は写しに従う(偽の Rust は写しを送り返さないので、届くまでは替わらない)。
    await expect(themeRadio(page, "システム")).toHaveAttribute("aria-checked", "true");
    await emitEvent(page, "app-snapshot", { ...base, theme: "dark" });
    await expect(themeRadio(page, "ダーク")).toHaveAttribute("aria-checked", "true");
    await expect(themeRadio(page, "システム")).toHaveAttribute("aria-checked", "false");

    // 選ばれているものをもう一度押しても送らない。
    await themeRadio(page, "ダーク").click();
    expect(await readCallArgs(page, "set_theme")).toHaveLength(1);
  });

  test("左右の矢印で選び替えられ_テーマだけが既定と違っても既定に戻すは無効のまま", async ({ page }) => {
    const base = projectsSnapshot([APP]);
    await open(page, { ...base, theme: "light" });
    await openSettings(page);
    await expect(themeRadio(page, "ライト")).toHaveAttribute("aria-checked", "true");
    await expect(resetButton(page)).toBeDisabled();

    await themeRadio(page, "ライト").focus();
    await page.keyboard.press("ArrowRight");
    await expect.poll(() => readCallArgs(page, "set_theme")).toEqual([{ theme: "dark" }]);
    await page.keyboard.press("ArrowLeft");
    // 写しが届く前でも、フォーカスのある項目(ダーク)を基準に 1 つずつ動く。
    await expect.poll(() => readCallArgs(page, "set_theme")).toEqual([{ theme: "dark" }, { theme: "light" }]);
    await page.keyboard.press("ArrowLeft");
    await expect
      .poll(() => readCallArgs(page, "set_theme"))
      .toEqual([{ theme: "dark" }, { theme: "light" }, { theme: "system" }]);
    // 矢印では設定画面が閉じない。
    await expect(dialog(page)).toBeVisible();
  });
});

test.describe("端と既定(AC-F06・AC-F07)", () => {
  test("端ではボタンが無効で値が変わらない", async ({ page }) => {
    await open(page, withFontSizes(projectsSnapshot([APP]), { terminal: 28, interface: 11 }));
    await openSettings(page);

    await expect(upButton(page, "terminal")).toBeDisabled();
    await expect(downButton(page, "terminal")).toBeEnabled();
    await expect(downButton(page, "interface")).toBeDisabled();
    await expect(upButton(page, "interface")).toBeEnabled();

    await terminalStepper(page).focus();
    await page.keyboard.press("ArrowUp");
    await page.keyboard.press("End");
    await interfaceStepper(page).focus();
    await page.keyboard.press("ArrowDown");
    await page.keyboard.press("Home");
    await upButton(page, "terminal").click({ force: true });
    await downButton(page, "interface").click({ force: true });

    expect(await readCallArgs(page, "step_font_size")).toEqual([]);
    await expect(terminalStepper(page)).toHaveAttribute("aria-valuenow", "28");
    await expect(interfaceStepper(page)).toHaveAttribute("aria-valuenow", "11");
  });

  test("既定に戻すが既定のときは無効", async ({ page }) => {
    const base = projectsSnapshot([APP]);
    await open(page, base);
    await openSettings(page);
    await expect(resetButton(page)).toBeDisabled();

    await emitEvent(page, "app-snapshot", withFontSizes(base, { interface: 15 }));
    await expect(resetButton(page)).toBeEnabled();
    // キーボードで押す(フォーカスがボタンにある状態で無効になる流れを見る)
    await resetButton(page).focus();
    await page.keyboard.press("Space");
    await expect.poll(() => readCallArgs(page, "reset_appearance")).toEqual([{}]);
    await resetButton(page).click();
    await expect.poll(() => readCallArgs(page, "reset_appearance")).toEqual([{}, {}]);

    // 既定の写しが届くと無効に戻り、フォーカスは「完了」へ移る(設定画面の外へ落とさない)
    await emitEvent(page, "app-snapshot", base);
    await expect(resetButton(page)).toBeDisabled();
    await expect(doneButton(page)).toBeFocused();
    await page.keyboard.press("Escape");
    await expect(dialog(page)).toHaveCount(0);
  });
});

test.describe("ターミナルと画面への反映(AC-F02〜F04)", () => {
  test("ターミナルの大きさの写しでタブ表示とタイル_9_枚の行と桁が変わり_resize_terminal_が送られる", async ({ page }) => {
    // タブ表示
    const tabs = projectsSnapshot([APP]);
    await open(page, tabs);
    await expect.poll(() => lastSize(page, "p-app-t1")).not.toBeNull();
    const before = await lastSize(page, "p-app-t1");
    await expect(xtermRows(page, "p-app-t1")).toHaveCSS("font-size", "15px");

    await openSettings(page);
    await emitEvent(page, "app-snapshot", withFontSizes(tabs, { terminal: 22 }));
    await expect(xtermRows(page, "p-app-t1")).toHaveCSS("font-size", "22px");
    await expect
      .poll(async () => {
        const size = await lastSize(page, "p-app-t1");
        return size !== null && before !== null && size.cols < before.cols && size.rows < before.rows;
      })
      .toBe(true);
    expect(await connectCount(page, "p-app-t1"), "つなぎ直さない").toBe(1);

    // タイル表示の 9 枚
    const tiles = withTileDisplay(projectsSnapshot(projects(9)));
    await open(page, tiles);
    const ids = tiles.display.tileIds;
    expect(ids).toHaveLength(9);
    for (const id of ids) await expect.poll(() => lastSize(page, id), id).not.toBeNull();
    const sizes = new Map<string, { cols: number; rows: number } | null>();
    for (const id of ids) sizes.set(id, await lastSize(page, id));

    await emitEvent(page, "app-snapshot", withFontSizes(tiles, { terminal: 9 }));
    for (const id of ids) {
      await expect(xtermRows(page, id), id).toHaveCSS("font-size", "9px");
      await expect
        .poll(async () => {
          const size = await lastSize(page, id);
          const old = sizes.get(id);
          return size !== null && old != null && size.cols > old.cols && size.rows > old.rows;
        }, id)
        .toBe(true);
      expect(await connectCount(page, id), `${id} はつなぎ直さない`).toBe(1);
    }
  });

  test("隠していたタブを表示すると初めから新しい大きさだったものと同じ行と桁", async ({ page }) => {
    // 初めから 20 で開いたときの、表示中のタブの行・桁(同じ置き場所)
    const at20 = withFontSizes(projectsSnapshot([APP]), { terminal: 20 });
    await open(page, at20);
    await expect.poll(() => lastSize(page, "p-app-t1")).not.toBeNull();
    const fresh = await lastSize(page, "p-app-t1");

    // 15 で開き、ターミナル 2 を隠したまま 20 にしてから表示する
    const base = projectsSnapshot([APP]);
    await open(page, base);
    await expect.poll(() => lastSize(page, "p-app-t2")).not.toBeNull();
    await emitEvent(page, "app-snapshot", withFontSizes(base, { terminal: 20 }));
    await expect.poll(() => lastSize(page, "p-app-t1")).toEqual(fresh);

    const shown = withFontSizes(base, { terminal: 20 });
    await emitEvent(page, "app-snapshot", {
      ...shown,
      projects: shown.projects.map((p) => (p.id === APP.id ? { ...p, activeTerminalId: "p-app-t2" } : p)),
      display: { ...shown.display, focusedTerminalId: "p-app-t2" },
    });
    await expect(xtermRows(page, "p-app-t2")).toHaveCSS("font-size", "20px");
    await expect.poll(() => lastSize(page, "p-app-t2")).toEqual(fresh);
    await expect(xtermRows(page, "p-app-t2").locator("> div")).toHaveCount(fresh?.rows ?? -1);
    expect(await connectCount(page, "p-app-t2"), "つなぎ直さない").toBe(1);
  });

  test("画面の大きさの写しでサイドバーとタブとツリーの文字と行の高さが変わりターミナルの文字は変わらない", async ({ page }) => {
    const base = projectsSnapshot([APP, LOG]);
    await open(page, base);
    const card = page.getByTestId("project-card").filter({ hasText: "ribbitlog" });
    const tabBar = page.getByTestId("tab-bar");
    const treeRow = page.locator('[data-testid="tree-row"][data-path="README.md"]');
    await expect(treeRow).toBeVisible();
    const terminalFont = await fontSizeOf(xtermRows(page, "p-app-t1"));

    await openSettings(page);
    for (const size of [18, 11]) {
      const m = interfaceMetrics(size);
      await emitEvent(page, "app-snapshot", withFontSizes(base, { interface: size }));
      await expect.poll(() => heightOf(tabBar), `タブバー ${size}`).toBe(m.tokens["--tab-bar-height"]);
      await expect.poll(() => heightOf(treeRow), `ツリーの行 ${size}`).toBe(m.treeRowHeight);
      await expect.poll(() => heightOf(card), `カード ${size}`).toBe(m.tokens["--sidebar-item-height"]);
      expect(await fontSizeOf(treeRow), `ツリーの文字 ${size}`).toBe(`${m.tokens["--font-size-l"]}px`);
      // 設定画面の文字も同じトークンで追従する
      expect(await fontSizeOf(stepperRow(page, "terminal")), `設定画面 ${size}`).toBe(`${m.tokens["--font-size-l"]}px`);
      // ターミナルの中の文字は画面の大きさに追従しない
      expect(await fontSizeOf(xtermRows(page, "p-app-t1")), `ターミナル ${size}`).toBe(terminalFont);
    }
  });
});

test.describe("見た目(AC-F13)", () => {
  for (const colorScheme of ["light", "dark"] as const) {
    test(`設定画面の画像を撮る_${colorScheme}`, async ({ page }) => {
      await page.setViewportSize({ width: 960, height: 600 });
      await page.emulateMedia({ colorScheme });
      const base = projectsSnapshot([APP, LOG]);
      await open(page, base);
      await expect(page.locator('[data-testid="tree-row"][data-path="README.md"]')).toBeVisible();
      await openSettings(page);

      // シートは上部の帯の下に、横の中央に付く。中身は枠からはみ出さない
      const sheet = await dialog(page).boundingBox();
      const header = await page.locator(".app-shell__header").boundingBox();
      if (sheet === null || header === null) throw new Error("位置が取れない");
      expect(sheet.y).toBeGreaterThanOrEqual(header.y + header.height);
      expect(Math.abs(sheet.x + sheet.width / 2 - 480)).toBeLessThan(2);
      expect(await dialog(page).evaluate((el) => el.scrollWidth <= el.clientWidth && el.scrollHeight <= el.clientHeight)).toBe(
        true,
      );
      // 値の右端は 2 行でそろう
      const right = async (l: Locator) => {
        const b = await l.boundingBox();
        return b === null ? -1 : Math.round(b.x + b.width);
      };
      expect(await right(terminalStepper(page))).toBe(await right(interfaceStepper(page)));
      await page.screenshot({ path: shotPath(`default-${colorScheme}`) });

      await emitEvent(page, "app-snapshot", withFontSizes(base, { terminal: 28, interface: 11 }));
      await expect(upButton(page, "terminal")).toBeDisabled();
      await expect(downButton(page, "interface")).toBeDisabled();
      await page.screenshot({ path: shotPath(`edges-${colorScheme}`) });

      // M4.8 でエディタの組が増え、画面の文字が 18 のときは高さ 600 では縦に送る(シートは overflow-y: auto)。
      // はみ出さないことは、既定のウィンドウより低い 720 の高さで確かめる(ADR-015 の 6)
      await page.setViewportSize({ width: 960, height: 720 });
      await emitEvent(page, "app-snapshot", withFontSizes(base, { terminal: 9, interface: 18 }));
      await expect(interfaceStepper(page)).toHaveAttribute("aria-valuenow", "18");
      await expect(upButton(page, "interface")).toBeDisabled();
      expect(await dialog(page).evaluate((el) => el.scrollWidth <= el.clientWidth && el.scrollHeight <= el.clientHeight)).toBe(
        true,
      );
      await page.screenshot({ path: shotPath(`interface-18-${colorScheme}`) });
    });
  }
});
