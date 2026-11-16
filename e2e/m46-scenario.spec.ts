// M4.6 のシナリオの画面の自動テスト(T71。ADR-013 の AC-F02・F04 の通し)。
//
// 設定画面で文字の大きさを変え、閉じたあとも、ターミナル・サイドバー・タブ・ツリーが新しい大きさのまま操作できることを、
// 機能をまたいだ流れで通す。部品ごとの細かい振る舞い(端・既定に戻す・読み上げ・画像など)は font-settings.spec.ts が見るので、
// ここでは繰り返さない。
//
// 偽の Rust 側(fixtures/tauriMock.ts)は写しを送り返さない。step_font_size が届いたら、本物の Rust が返す写しを
// withSteppedFontSize(Rust の規則の写し)で作って app-snapshot で流す。Rust が続けて送った写しは emitEventsInOneTurn で
// 1 回の実行の中に流して代える。保存と再起動で残ること(AC-F08)は実アプリの node scripts/smoke.mjs --fonts が見る。

import { expect, type Locator, type Page, test } from "@playwright/test";

import type { AppSnapshot } from "../src/ipc/generated/AppSnapshot";
import { interfaceMetrics } from "../src/logic/appearance/interfaceMetrics";
import {
  emitEvent,
  emitEventsInOneTurn,
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
  withFontSizes,
  withSteppedFontSize,
  withTileDisplay,
} from "./fixtures/tauriMock";

const APP: SampleProject = { id: "p-app", name: "app", color: "green", tabs: ["ターミナル 1"] };
const LOG: SampleProject = { id: "p-log", name: "ribbitlog", color: "orange", tabs: ["ターミナル 1"] };
const TERMINAL_ID = `${APP.id}-t1`;

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

type Size = { cols: number; rows: number };

const dialog = (page: Page) => page.getByRole("dialog", { name: "設定" });
const terminalStepper = (page: Page) => page.getByRole("spinbutton", { name: "ターミナルの文字の大きさ" });
const interfaceStepper = (page: Page) => page.getByRole("spinbutton", { name: "画面の文字の大きさ" });
const resetButton = (page: Page) => dialog(page).getByRole("button", { name: "既定に戻す" });
const doneButton = (page: Page) => dialog(page).getByRole("button", { name: "完了" });
const terminalInput = (page: Page, id: string) =>
  page.locator(`[data-testid="terminal-host"][data-terminal-id="${id}"] .xterm-helper-textarea`);
const xtermRows = (page: Page, id: string) =>
  page.locator(`[data-testid="terminal-host"][data-terminal-id="${id}"] .xterm-rows`);
const treeRow = (page: Page, path: string) => page.locator(`[data-testid="tree-row"][data-path=${JSON.stringify(path)}]`);
const projectCard = (page: Page, name: string) => page.getByTestId("project-card").filter({ hasText: name });
const newTabButton = (page: Page) => page.getByRole("button", { name: "新規ターミナル", exact: true });

const fontSizeOf = (locator: Locator) => locator.evaluate((el) => getComputedStyle(el).fontSize);
const heightOf = (locator: Locator) => locator.evaluate((el) => el.getBoundingClientRect().height);

async function open(page: Page, snapshot: AppSnapshot): Promise<void> {
  await installTauriMock(page, { get_app_snapshot: { ok: snapshot } }, LISTINGS);
  await page.goto("/");
  await expect
    .poll(async () => (await readRecordedCalls(page)).map((call) => call.command))
    .toContain("notify_app_ready");
}

async function openSettings(page: Page): Promise<void> {
  await emitEvent(page, "menu-command", "open-settings");
  await expect(dialog(page)).toBeVisible();
}

/** ターミナルに最後に送った寸法(接続の寸法か、そのあとの寸法の変更)。 */
async function lastSize(page: Page, terminalId: string): Promise<Size | null> {
  const resized = (await readCallArgs(page, "resize_terminal")).filter((r) => r.terminalId === terminalId);
  const last = resized.at(-1);
  if (last !== undefined) return { cols: last.cols, rows: last.rows };
  const connected = (await readCallArgs(page, "connect_terminal")).find((c) => c.terminalId === terminalId);
  return connected === undefined ? null : { cols: connected.cols, rows: connected.rows };
}

async function connectCount(page: Page, terminalId: string): Promise<number> {
  return (await readCallArgs(page, "connect_terminal")).filter((c) => c.terminalId === terminalId).length;
}

async function callCount(page: Page, command: string): Promise<number> {
  return (await readRecordedCalls(page)).filter((c) => c.command === command).length;
}

/** ターミナルに書いた文字(write_terminal の中身をつないだもの)。 */
async function typedIntoTerminal(page: Page): Promise<string> {
  return (await readCallArgs(page, "write_terminal")).map((w) => w.data).join("");
}

/** 表示中のタイルの寸法がすべて揃うまで待ち、ID → 寸法を返す。 */
async function settledSizes(page: Page, ids: readonly string[]): Promise<Map<string, Size>> {
  const sizes = new Map<string, Size>();
  for (const id of ids) {
    await expect.poll(() => lastSize(page, id), id).not.toBeNull();
    // 置き場所が決まるまでの寸法の変更が落ち着くのを待つ(続けて 2 回読んで同じ)
    let previous = await lastSize(page, id);
    await expect
      .poll(async () => {
        const now = await lastSize(page, id);
        const same = JSON.stringify(now) === JSON.stringify(previous);
        previous = now;
        return same;
      }, id)
      .toBe(true);
    const size = await lastSize(page, id);
    if (size === null) throw new Error(`${id} の寸法が無い`);
    sizes.set(id, size);
  }
  return sizes;
}

/**
 * 設定画面のステッパーにフォーカスがある状態で key を押し、step_font_size が届いたら Rust の返す写しを流す。
 * 流した写しを返す(次の刻みの元にする)。
 */
async function stepWithKey(
  page: Page,
  snapshot: AppSnapshot,
  target: "terminal" | "interface",
  key: "ArrowUp" | "ArrowDown",
): Promise<AppSnapshot> {
  const before = await callCount(page, "step_font_size");
  await page.keyboard.press(key);
  await expect.poll(() => callCount(page, "step_font_size")).toBe(before + 1);
  const next = withSteppedFontSize(snapshot, target, key === "ArrowUp" ? 1 : -1);
  await emitEvent(page, "app-snapshot", next);
  return next;
}

// 各テストの終わりに、画面の失敗(例外、通信の失敗の報告)が無かったことを見る。
test.afterEach(async ({ page }) => {
  const reported = await readCallArgs(page, "report_frontend_error");
  expect(reported.map((r) => r.message), "画面の失敗が Rust へ報告された").toEqual([]);
});

test.describe("M4.6 のシナリオ(文字の大きさの設定)", () => {
  test("設定画面で両方を変えて閉じると全部が新しい大きさのまま操作できる", async ({ page }) => {
    let snapshot = projectsSnapshot([APP, LOG]);
    await open(page, snapshot);
    await expect.poll(() => connectCount(page, TERMINAL_ID)).toBe(1);
    const [connect] = await readCallArgs(page, "connect_terminal");
    if (connect === undefined) throw new Error("connect_terminal が届いていない");
    const channelId = parseChannelId(connect.onOutput);
    await expect(treeRow(page, "README.md")).toBeVisible();
    const before = (await settledSizes(page, [TERMINAL_ID])).get(TERMINAL_ID);
    await expect(xtermRows(page, TERMINAL_ID)).toHaveCSS("font-size", "15px");

    // ターミナルにフォーカスしてから ⌘, で開き、キーボードだけで両方を 2 つずつ大きくする
    await terminalInput(page, TERMINAL_ID).focus();
    await openSettings(page);
    await expect(terminalStepper(page)).toBeFocused();
    snapshot = await stepWithKey(page, snapshot, "terminal", "ArrowUp");
    snapshot = await stepWithKey(page, snapshot, "terminal", "ArrowUp");
    await expect(terminalStepper(page)).toHaveAttribute("aria-valuenow", "17");
    await page.keyboard.press("Tab");
    await expect(interfaceStepper(page)).toBeFocused();
    snapshot = await stepWithKey(page, snapshot, "interface", "ArrowUp");
    snapshot = await stepWithKey(page, snapshot, "interface", "ArrowUp");
    await expect(interfaceStepper(page)).toHaveAttribute("aria-valuenow", "15");
    expect(await readCallArgs(page, "step_font_size")).toEqual([
      { target: "terminal", delta: 1 },
      { target: "terminal", delta: 1 },
      { target: "interface", delta: 1 },
      { target: "interface", delta: 1 },
    ]);

    // 閉じるとターミナルにフォーカスが戻る
    await page.keyboard.press("Escape");
    await expect(dialog(page)).toHaveCount(0);
    await expect(terminalInput(page, TERMINAL_ID)).toBeFocused();

    // AC-F02: ターミナルは 17 で、寸法が小さくなって PTY に送られ、つなぎ直さない
    await expect(xtermRows(page, TERMINAL_ID)).toHaveCSS("font-size", "17px");
    await expect
      .poll(async () => {
        const size = await lastSize(page, TERMINAL_ID);
        return size !== null && before !== undefined && size.cols < before.cols && size.rows < before.rows;
      })
      .toBe(true);
    expect(await connectCount(page, TERMINAL_ID), "つなぎ直さない").toBe(1);

    // AC-F04: サイドバー・タブ・ツリーの文字と行の高さは画面の大きさ 15 のもの。ターミナルの文字は画面の大きさに追従しない
    const m = interfaceMetrics(15);
    await expect.poll(() => heightOf(page.getByTestId("tab-bar"))).toBe(m.tokens["--tab-bar-height"]);
    await expect.poll(() => heightOf(projectCard(page, "ribbitlog"))).toBe(m.tokens["--sidebar-item-height"]);
    await expect.poll(() => heightOf(treeRow(page, "README.md"))).toBe(m.treeRowHeight);
    expect(await fontSizeOf(treeRow(page, "README.md"))).toBe(`${m.tokens["--font-size-l"]}px`);
    expect(await fontSizeOf(xtermRows(page, TERMINAL_ID))).toBe("17px");

    // 操作できる: ターミナルへの入力と出力
    await page.keyboard.type("echo ok");
    await expect.poll(() => typedIntoTerminal(page)).toBe("echo ok");
    expect((await readCallArgs(page, "write_terminal")).every((w) => w.terminalId === TERMINAL_ID)).toBe(true);
    await sendChannelBytes(page, channelId, ["still-running"]);
    await expect(xtermRows(page, TERMINAL_ID).locator("> div").first()).toHaveText("still-running");

    // 操作できる: ツリーのキー(⌘O で開く)、タブバーの新規ターミナル、サイドバーのカード
    await treeRow(page, "README.md").click();
    await expect(page.getByTestId("file-tree")).toBeFocused();
    await page.keyboard.press("Meta+O");
    await expect.poll(async () => (await readCallArgs(page, "open_entry")).length).toBe(1);
    await newTabButton(page).click();
    await expect.poll(() => callCount(page, "new_terminal")).toBe(1);
    await projectCard(page, "ribbitlog").click();
    await expect.poll(async () => (await readCallArgs(page, "select_project")).map((c) => c.projectId)).toEqual([LOG.id]);

    // 開き直すと新しい値のまま(既定に戻すは押せる)。完了で閉じる
    await openSettings(page);
    await expect(terminalStepper(page)).toHaveAttribute("aria-valuenow", "17");
    await expect(interfaceStepper(page)).toHaveAttribute("aria-valuenow", "15");
    await expect(resetButton(page)).toBeEnabled();
    await doneButton(page).click();
    await expect(dialog(page)).toHaveCount(0);

    // シェルは続いている(閉じる・既定に戻すは送っていない)
    expect(await callCount(page, "close_terminal")).toBe(0);
    expect(await callCount(page, "reset_font_sizes")).toBe(0);
    expect(await connectCount(page, TERMINAL_ID)).toBe(1);
  });

  test("タイル表示で大きさを続けて変えても行と桁が最後の値に落ち着く", async ({ page }) => {
    const tiles = withTileDisplay(projectsSnapshot(projects(9)));
    const ids = tiles.display.tileIds;
    expect(ids).toHaveLength(9);

    // 初めから 20 で開いたときの 9 枚の行・桁(同じ置き場所)
    await open(page, withFontSizes(tiles, { terminal: 20 }));
    const fresh20 = await settledSizes(page, ids);

    // 15 で開き、Rust が続けて送った写し(16〜22 と、戻って 21・20)を同じフレームで受ける
    await open(page, tiles);
    const fresh15 = await settledSizes(page, ids);
    const resizesBefore = await callCount(page, "resize_terminal");
    const burst = [16, 17, 18, 19, 20, 21, 22, 21, 20].map((terminal) => withFontSizes(tiles, { terminal }));
    await emitEventsInOneTurn(page, "app-snapshot", burst);

    for (const id of ids) {
      await expect(xtermRows(page, id), id).toHaveCSS("font-size", "20px");
      await expect.poll(() => lastSize(page, id), id).toEqual(fresh20.get(id));
      await expect(xtermRows(page, id).locator("> div"), id).toHaveCount(fresh20.get(id)?.rows ?? -1);
      expect(await connectCount(page, id), `${id} はつなぎ直さない`).toBe(1);
    }
    // 最後の値だけを当てる(1 枚につき寸法の変更は多くても 1 回)
    expect(await callCount(page, "resize_terminal")).toBeLessThanOrEqual(resizesBefore + ids.length);

    // 設定画面でキーを続けて押し、届いた写しを待たずに流しても、最後の値(15)の行・桁に戻る
    let snapshot = burst.at(-1) ?? tiles;
    await openSettings(page);
    await expect(terminalStepper(page)).toBeFocused();
    for (let i = 0; i < 5; i += 1) {
      await page.keyboard.press("ArrowDown");
      snapshot = withSteppedFontSize(snapshot, "terminal", -1);
      await emitEvent(page, "app-snapshot", snapshot);
    }
    await expect.poll(async () => (await readCallArgs(page, "step_font_size")).length).toBe(5);
    expect((await readCallArgs(page, "step_font_size")).every((c) => c.target === "terminal" && c.delta === -1)).toBe(true);
    await expect(terminalStepper(page)).toHaveAttribute("aria-valuenow", "15");
    await page.keyboard.press("Escape");
    await expect(dialog(page)).toHaveCount(0);

    for (const id of ids) {
      await expect(xtermRows(page, id), id).toHaveCSS("font-size", "15px");
      await expect.poll(() => lastSize(page, id), id).toEqual(fresh15.get(id));
      expect(await connectCount(page, id), `${id} はつなぎ直さない`).toBe(1);
    }
    expect(await callCount(page, "close_terminal")).toBe(0);
  });
});
