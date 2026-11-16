// ターミナル領域(T34d): タブバー、タブ 0 枚、右ペインの空の状態、終了の確認。
//
// 偽の Rust 側(fixtures/tauriMock.ts)に写しを渡して画面を開き、部品が正しいコマンドを呼ぶこと、
// 写しのとおりに描くこと、レイアウトが崩れていないこと(要素の位置と大きさ)を見る。
// コマンドの結果は写しで届く(偽物は写しを送り返さない)ので、結果の表示を見るテストは写しのイベントを自分で流す。
// ⌘T・⌘W のキーそのものは OS のメニューが受ける(自動テストには無い)ので、写しのイベントで代える。

import { expect, type Locator, type Page, test } from "@playwright/test";

import type { AppSnapshot } from "../src/ipc/generated/AppSnapshot";
import {
  emitEvent,
  installTauriMock,
  parseChannelId,
  projectsSnapshot,
  readCallArgs,
  readRecordedCalls,
  sampleRunningTerminal,
  type SampleProject,
  sendChannelBytes,
  type TauriMockConfig,
} from "./fixtures/tauriMock";

const APP: SampleProject = { id: "p-app", name: "app", color: "green", tabs: ["ターミナル 1", "ターミナル 2"] };
const LOG: SampleProject = { id: "p-log", name: "ribbitlog", color: "orange", tabs: ["ターミナル 1"] };

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

const tabList = (page: Page) => page.getByRole("tablist", { name: "ターミナルのタブ" });
const tabs = (page: Page) => tabList(page).getByRole("tab");
const tab = (page: Page, name: string) => tabList(page).getByRole("tab", { name, exact: true });
/** タブ 1 つぶんの枠(タブと閉じるボタンを囲む。表示中の明るい地はここに付く)。 */
const tabItem = (page: Page, name: string) =>
  tabList(page).getByTestId("tab-item").filter({ has: page.getByRole("tab", { name, exact: true }) });
const background = (locator: Locator) => locator.evaluate((el) => getComputedStyle(el).backgroundColor);
/** プロジェクト色の印(カード・タブの名前の前の小さな丸)の色。 */
const dotColor = (locator: Locator) => background(locator.locator(".project-dot"));

/** 指定のターミナルの接続(connect_terminal の引数)。届くまで待つ。 */
async function connectOf(page: Page, terminalId: string) {
  await expect.poll(async () => (await readCallArgs(page, "connect_terminal")).some((c) => c.terminalId === terminalId)).toBe(true);
  const found = (await readCallArgs(page, "connect_terminal")).find((c) => c.terminalId === terminalId);
  if (found === undefined) throw new Error(`${terminalId} の接続が無い`);
  return found;
}

async function ackedBytes(page: Page, terminalId: string): Promise<number> {
  const acks = await readCallArgs(page, "ack_terminal_output");
  return acks.filter((a) => a.terminalId === terminalId).reduce((sum, a) => sum + a.bytes, 0);
}

// 各テストの終わりに、画面の失敗(例外、通信の失敗の報告)が無かったことを見る。
test.afterEach(async ({ page }) => {
  const reported = await readCallArgs(page, "report_frontend_error");
  expect(reported.map((r) => r.message), "画面の失敗が Rust へ報告された").toEqual([]);
});

test.describe("タブバー(FR-039・FR-044・FR-061)", () => {
  test("タブバーに選択中のプロジェクトのタブが並び表示中が強調される", async ({ page }) => {
    await open(page, projectsSnapshot([APP, LOG]));

    await expect(tabs(page)).toHaveText(["ターミナル 1", "ターミナル 2"]);
    await expect(tab(page, "ターミナル 1")).toHaveAttribute("aria-selected", "true");
    await expect(tab(page, "ターミナル 2")).toHaveAttribute("aria-selected", "false");
    // 文字の色だけでなく、太字と明るい地でも分かる。
    const style = async (name: string) => ({
      weight: await tab(page, name).evaluate((el) => getComputedStyle(el).fontWeight),
      fill: await background(tabItem(page, name)),
    });
    const [shown, other] = [await style("ターミナル 1"), await style("ターミナル 2")];
    expect(shown.weight).not.toBe(other.weight);
    expect(shown.fill).not.toBe(other.fill);
    // 表示中のタブとターミナルの置き場所がつながっている(tabpanel の名前は表示中のタブ)。
    await expect(page.getByRole("tabpanel", { name: "ターミナル 1" })).toBeVisible();
    await expect(page.getByTestId("terminal-host")).toHaveCount(1);
  });

  test("タブをクリックすると切り替わる", async ({ page }) => {
    await open(page, projectsSnapshot([APP, LOG]));

    await tab(page, "ターミナル 2").click();
    await expect.poll(() => readCallArgs(page, "select_terminal")).toEqual([{ terminalId: "p-app-t2" }]);

    // Rust が写しを返す: 2 枚目が表示中になり、そのターミナルにフォーカスが移る。
    const next = projectsSnapshot([APP, LOG]);
    next.projects[0] = { ...next.projects[0]!, activeTerminalId: "p-app-t2" };
    await emitEvent(page, "app-snapshot", next);
    await expect(tab(page, "ターミナル 2")).toHaveAttribute("aria-selected", "true");
    await expect(tab(page, "ターミナル 1")).toHaveAttribute("aria-selected", "false");
    await expect(page.getByRole("tabpanel", { name: "ターミナル 2" })).toBeVisible();
    await expect(page.getByTestId("terminal-host").locator(".xterm-helper-textarea")).toBeFocused();
  });

  test("表示中のタブをクリックしても選択のコマンドを呼ばない", async ({ page }) => {
    await open(page, projectsSnapshot([APP]));
    await tab(page, "ターミナル 1").click();
    await tab(page, "ターミナル 2").click();
    await expect.poll(() => readCallArgs(page, "select_terminal")).toEqual([{ terminalId: "p-app-t2" }]);
  });

  test("閉じるボタンで閉じるコマンドが呼ばれ_選択は呼ばない", async ({ page }) => {
    await open(page, projectsSnapshot([APP]));

    await page.getByRole("button", { name: "「ターミナル 2」を閉じる" }).click();

    await expect.poll(() => readCallArgs(page, "close_terminal")).toEqual([{ terminalId: "p-app-t2" }]);
    expect(await readCallArgs(page, "select_terminal")).toEqual([]);
  });

  test("プラスのボタンで新しいタブのコマンドが呼ばれる", async ({ page }) => {
    await open(page, projectsSnapshot([APP]));
    await page.getByRole("button", { name: "新規ターミナル", exact: true }).click();
    await expect.poll(() => readCallArgs(page, "new_terminal")).toHaveLength(1);
  });

  test("写しでタブが増えると新しいタブが表示されフォーカスが移る", async ({ page }) => {
    await open(page, projectsSnapshot([APP]));
    await connectOf(page, "p-app-t1");

    // ⌘T の結果(Rust が新しいタブを足し、表示中にした写し)。
    const added = projectsSnapshot([{ ...APP, tabs: ["ターミナル 1", "ターミナル 2", "ターミナル 3"] }]);
    added.projects[0] = { ...added.projects[0]!, activeTerminalId: "p-app-t3" };
    await emitEvent(page, "app-snapshot", added);

    await expect(tabs(page)).toHaveText(["ターミナル 1", "ターミナル 2", "ターミナル 3"]);
    await expect(tab(page, "ターミナル 3")).toHaveAttribute("aria-selected", "true");
    await expect(page.getByTestId("terminal-host").locator(".xterm-helper-textarea")).toBeFocused();
    // 新しいタブのターミナルは表示中のターミナルと同じ寸法で接続する。
    const first = await connectOf(page, "p-app-t1");
    const third = await connectOf(page, "p-app-t3");
    expect([third.rows, third.cols]).toEqual([first.rows, first.cols]);
  });

  test("隠したまま起動したターミナルの最初の寸法が表示中と同じ", async ({ page }) => {
    await open(page, projectsSnapshot([APP, LOG]));
    const shown = await connectOf(page, "p-app-t1");
    const hiddenTab = await connectOf(page, "p-app-t2");
    const hiddenProject = await connectOf(page, "p-log-t1");
    expect(shown.rows).toBeGreaterThan(0);
    expect(shown.cols).toBeGreaterThan(0);
    expect([hiddenTab.rows, hiddenTab.cols]).toEqual([shown.rows, shown.cols]);
    expect([hiddenProject.rows, hiddenProject.cols]).toEqual([shown.rows, shown.cols]);
  });

  test("隠したまま起動したターミナルへ切り替えると_出力が描かれ_行数と大きさが表示中だったものと同じ", async ({ page }) => {
    await open(page, projectsSnapshot([APP, LOG]));
    const shown = await connectOf(page, "p-app-t1");
    const rows = page.getByTestId("terminal-host").locator(".xterm-rows > div");
    const screen = page.getByTestId("terminal-host").locator(".xterm-screen");
    await expect(rows).toHaveCount(shown.rows);
    const firstBox = await box(screen);

    // 隠したまま作ったタブ(同じプロジェクト)と、隠したまま作った別のプロジェクトのターミナルの両方を見る。
    const targets = [
      { id: "p-app-t2", snapshot: () => {
        const next = projectsSnapshot([APP, LOG]);
        next.projects[0] = { ...next.projects[0]!, activeTerminalId: "p-app-t2" };
        return next;
      } },
      { id: "p-log-t1", snapshot: () => projectsSnapshot([APP, LOG], { selectedProjectId: LOG.id }) },
    ];
    for (const target of targets) {
      const channel = parseChannelId((await connectOf(page, target.id)).onOutput);
      // 隠れている間に届いた出力と、表示してから届いた出力。
      await sendChannelBytes(page, channel, [`hidden ${target.id}\r\n`]);
      await emitEvent(page, "app-snapshot", target.snapshot());
      await expect(rows.nth(0), target.id).toHaveText(`hidden ${target.id}`);
      await sendChannelBytes(page, channel, [`shown ${target.id}\r\n`]);
      await expect(rows.nth(1), target.id).toHaveText(`shown ${target.id}`);
      await expect(rows, target.id).toHaveCount(shown.rows);
      const b = await box(screen);
      expect([Math.round(b.width), Math.round(b.height)], target.id).toEqual([
        Math.round(firstBox.width),
        Math.round(firstBox.height),
      ]);
      // 表示したあとに寸法を送り直していても、行・桁は最初の接続と同じ。
      const resized = (await readCallArgs(page, "resize_terminal")).filter((r) => r.terminalId === target.id);
      for (const r of resized) expect([r.rows, r.cols], target.id).toEqual([shown.rows, shown.cols]);
    }
  });

  for (const colorScheme of ["light", "dark"] as const) {
    test(`タブにプロジェクト色の印がありタブ名の文字がある(${colorScheme})`, async ({ page }) => {
      await page.emulateMedia({ colorScheme });
      await open(page, projectsSnapshot([APP, LOG]));
      const card = page.getByTestId("project-card").filter({ hasText: "app" });
      const cardColor = await dotColor(card);
      expect(cardColor).not.toBe("rgba(0, 0, 0, 0)");
      for (const name of ["ターミナル 1", "ターミナル 2"]) {
        expect(await dotColor(tabItem(page, name)), name).toBe(cardColor);
        await expect(tab(page, name)).toHaveText(name);
      }
      // ツールチップで「プロジェクト名: タブ名」も読める。
      await expect(tab(page, "ターミナル 1")).toHaveAttribute("title", "app: ターミナル 1");
    });
  }

  test("キーボードでタブを移り_Enter_で選び_Delete_で閉じられる", async ({ page }) => {
    await open(page, projectsSnapshot([{ ...APP, tabs: ["ターミナル 1", "ターミナル 2", "ターミナル 3"] }]));

    // 表示中のタブだけが Tab で止まる(ほかは矢印で移る)。
    await expect(tab(page, "ターミナル 1")).toHaveAttribute("tabindex", "0");
    await expect(tab(page, "ターミナル 2")).toHaveAttribute("tabindex", "-1");
    await tab(page, "ターミナル 1").focus();
    await page.keyboard.press("ArrowRight");
    await expect(tab(page, "ターミナル 2")).toBeFocused();
    await page.keyboard.press("End");
    await expect(tab(page, "ターミナル 3")).toBeFocused();
    await page.keyboard.press("ArrowRight");
    await expect(tab(page, "ターミナル 1")).toBeFocused();
    await page.keyboard.press("ArrowLeft");
    await expect(tab(page, "ターミナル 3")).toBeFocused();
    await page.keyboard.press("Home");
    await expect(tab(page, "ターミナル 1")).toBeFocused();
    await page.keyboard.press("ArrowRight");

    await page.keyboard.press("Enter");
    await expect.poll(() => readCallArgs(page, "select_terminal")).toEqual([{ terminalId: "p-app-t2" }]);
    await page.keyboard.press("Delete");
    await expect.poll(() => readCallArgs(page, "close_terminal")).toEqual([{ terminalId: "p-app-t2" }]);
    // 閉じるタブのボタンは写しで消えるので、フォーカスは先に隣のタブへ移っている(body に落ちない)。
    await expect(tab(page, "ターミナル 3")).toBeFocused();
  });

  test("プロジェクトを切り替えるとタブ群が切り替わる", async ({ page }) => {
    await open(page, projectsSnapshot([APP, { ...LOG, tabs: ["ターミナル 1", "ターミナル 2", "ターミナル 3"] }]));
    await expect(tabs(page)).toHaveCount(2);

    await emitEvent(
      page,
      "app-snapshot",
      projectsSnapshot([APP, { ...LOG, tabs: ["ターミナル 1", "ターミナル 2", "ターミナル 3"] }], { selectedProjectId: LOG.id }),
    );

    await expect(tabs(page)).toHaveText(["ターミナル 1", "ターミナル 2", "ターミナル 3"]);
    await expect(tab(page, "ターミナル 1")).toHaveAttribute("title", "ribbitlog: ターミナル 1");
    const card = page.getByTestId("project-card").filter({ hasText: "ribbitlog" });
    // タブのプロジェクト色の印も、切り替えたプロジェクトの色になる。
    expect(await dotColor(tabItem(page, "ターミナル 1"))).toBe(await dotColor(card));
  });

  test("プロジェクトを切り替えても前のプロジェクトのターミナルに出力が取り込まれ続ける", async ({ page }) => {
    await open(page, projectsSnapshot([APP, LOG]));
    const appChannel = parseChannelId((await connectOf(page, "p-app-t1")).onOutput);
    await connectOf(page, "p-log-t1");

    await emitEvent(page, "app-snapshot", projectsSnapshot([APP, LOG], { selectedProjectId: LOG.id }));
    await expect(tab(page, "ターミナル 1")).toHaveAttribute("title", "ribbitlog: ターミナル 1");

    const before = await ackedBytes(page, "p-app-t1");
    const sent = await sendChannelBytes(page, appChannel, ["while hidden\r\n"]);
    await expect.poll(() => ackedBytes(page, "p-app-t1")).toBe(before + sent);

    // 戻ると、隠していた間の出力が画面にある。接続は作り直さない。
    await emitEvent(page, "app-snapshot", projectsSnapshot([APP, LOG]));
    await expect(page.getByTestId("terminal-host").locator(".xterm-rows > div").nth(0)).toHaveText("while hidden");
    expect((await readCallArgs(page, "connect_terminal")).filter((c) => c.terminalId === "p-app-t1")).toHaveLength(1);
  });
});

test.describe("タブ 0 枚と右ペインの空の状態(C-21、設計書 §8)", () => {
  test("0_枚のときターミナルがありませんと新規ターミナルのボタンが出る", async ({ page }) => {
    await open(page, projectsSnapshot([{ ...APP, tabs: [] }]));

    const empty = page.getByTestId("terminals-empty");
    await expect(empty).toContainText("ターミナルがありません");
    await expect(page.getByTestId("terminal-host")).toHaveCount(0);
    await expect(tabs(page)).toHaveCount(0);
    await expect(page.getByRole("button", { name: "新規ターミナル", exact: true })).toBeEnabled();
    expect(await readCallArgs(page, "connect_terminal")).toEqual([]);
  });

  test("新規ターミナルのボタンで新しいタブのコマンドが呼ばれる", async ({ page }) => {
    await open(page, projectsSnapshot([{ ...APP, tabs: [] }]));
    await page.getByTestId("terminals-empty").getByRole("button", { name: /新規ターミナル/ }).click();
    await expect.poll(() => readCallArgs(page, "new_terminal")).toHaveLength(1);
  });

  test("最後の_1_枚が閉じると_0_枚になり_自動で開かない", async ({ page }) => {
    await open(page, projectsSnapshot([{ ...APP, tabs: ["ターミナル 1"] }]));
    await connectOf(page, "p-app-t1");

    await emitEvent(page, "app-snapshot", projectsSnapshot([{ ...APP, tabs: [] }]));

    await expect(page.getByTestId("terminals-empty")).toContainText("ターミナルがありません");
    await expect(page.getByTestId("terminal-host")).toHaveCount(0);
    expect(await readCallArgs(page, "new_terminal")).toEqual([]);
    // フォーカスは「新規ターミナル」のボタンへ移る(消えたターミナルと一緒に失われない)。
    await expect(page.getByTestId("terminals-empty").getByRole("button", { name: /新規ターミナル/ })).toBeFocused();
  });

  test("見つからないプロジェクトでは新しいタブを開けない", async ({ page }) => {
    await open(page, projectsSnapshot([{ id: "p-gone", name: "gone", missing: true }]));

    const empty = page.getByTestId("terminals-empty");
    await expect(empty).toContainText("フォルダが見つからないため");
    await expect(empty.getByRole("button")).toHaveCount(0);
    await expect(page.getByRole("button", { name: "新規ターミナル", exact: true })).toBeDisabled();
  });

  test("プロジェクトが_0_件なら右ペインに案内だけを出す", async ({ page }) => {
    await open(page, projectsSnapshot([]));
    await expect(page.getByTestId("terminals-empty")).toContainText("プロジェクトがありません");
    await expect(page.getByRole("tablist")).toHaveCount(0);
    // 「フォルダを追加」は左ペインの 1 つだけ。
    await expect(page.getByRole("button", { name: "フォルダを追加" })).toHaveCount(1);
  });

  test("選択中のプロジェクトが無いなら選ぶ案内を出す", async ({ page }) => {
    await open(page, projectsSnapshot([APP], { selectedProjectId: null }));
    await expect(page.getByTestId("terminals-empty")).toContainText("プロジェクトを選んでください");
    await expect(page.getByRole("tablist")).toHaveCount(0);
  });
});

test.describe("終了の確認(FR-063)", () => {
  const running = [sampleRunningTerminal("p-app", "app", "p-app-t1", "ターミナル 1"), sampleRunningTerminal("p-log", "ribbitlog", "p-log-t1", "ターミナル 1")];

  test("終了の確認の要求で動作中の一覧が出て_終了で確定のコマンド", async ({ page }) => {
    await open(page, projectsSnapshot([APP, LOG]));

    await emitEvent(page, "quit-requested", { running });

    const dialog = page.getByRole("alertdialog", { name: "Arin を終了しますか?" });
    await expect(dialog).toBeVisible();
    await expect(dialog.getByRole("listitem")).toHaveText(["app: ターミナル 1", "ribbitlog: ターミナル 1"]);
    // 誤って確定しないよう、開いたときは取り消しにフォーカスがある。
    await expect(dialog.getByRole("button", { name: "取り消し" })).toBeFocused();

    await dialog.getByRole("button", { name: "終了" }).click();

    await expect.poll(() => readCallArgs(page, "confirm_quit")).toHaveLength(1);
    await expect(dialog).toHaveCount(0);
  });

  test("取り消しで何も呼ばずに閉じる_Esc_でも同じ", async ({ page }) => {
    await open(page, projectsSnapshot([APP, LOG]));
    const dialog = page.getByRole("alertdialog");

    await emitEvent(page, "quit-requested", { running });
    await dialog.getByRole("button", { name: "取り消し" }).click();
    await expect(dialog).toHaveCount(0);

    await emitEvent(page, "quit-requested", { running });
    await expect(dialog).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(dialog).toHaveCount(0);

    expect(await readCallArgs(page, "confirm_quit")).toEqual([]);
  });

  test("同じ要求が_2_回届いてもダイアログは_1_つで一覧は新しいもの", async ({ page }) => {
    await open(page, projectsSnapshot([APP, LOG]));

    await emitEvent(page, "quit-requested", { running });
    await emitEvent(page, "quit-requested", { running: running.slice(1) });

    await expect(page.getByRole("alertdialog")).toHaveCount(1);
    await expect(page.getByRole("alertdialog").getByRole("listitem")).toHaveText(["ribbitlog: ターミナル 1"]);
  });

  test("キーボードだけで確定できる", async ({ page }) => {
    await open(page, projectsSnapshot([APP]));
    await emitEvent(page, "quit-requested", { running: running.slice(0, 1) });
    const dialog = page.getByRole("alertdialog");
    await expect(dialog.getByRole("button", { name: "取り消し" })).toBeFocused();

    await page.keyboard.press("Tab");
    await expect(dialog.getByRole("button", { name: "終了" })).toBeFocused();
    await page.keyboard.press("Enter");

    await expect.poll(() => readCallArgs(page, "confirm_quit")).toHaveLength(1);
  });
});

test.describe("レイアウトの崩れ(要素の位置と大きさ)", () => {
  test("タブバーとターミナルが重ならず_ターミナルの置き場所の幅と高さが_0_でない", async ({ page }) => {
    await open(page, projectsSnapshot([APP, LOG]));
    const pane = await box(page.getByTestId("pane-terminal"));
    const bar = await box(page.getByTestId("tab-bar"));
    const host = await box(page.getByTestId("terminal-host"));

    expect(bar.height).toBeGreaterThan(16);
    expect(host.width).toBeGreaterThan(0);
    expect(host.height).toBeGreaterThan(0);
    // タブバーは右ペインの上端、ターミナルはその下で、右ペインの下端まで。
    expect(Math.round(bar.y)).toBe(Math.round(pane.y));
    expect(host.y).toBeGreaterThanOrEqual(bar.y + bar.height - 0.5);
    expect(Math.round(host.y + host.height)).toBe(Math.round(pane.y + pane.height));
    for (const b of [bar, host]) {
      expect(b.x).toBeGreaterThanOrEqual(pane.x - 0.5);
      expect(b.x + b.width).toBeLessThanOrEqual(pane.x + pane.width + 0.5);
    }
    // タブは左から順に並び、重ならない。
    const [first, second] = await Promise.all([box(tab(page, "ターミナル 1")), box(tab(page, "ターミナル 2"))]);
    expect(second.x).toBeGreaterThanOrEqual(first.x + first.width - 0.5);
  });

  test("タブが多いときもタブバーが右ペインからはみ出さず_横にスクロールし_プラスは見えたまま", async ({ page }) => {
    const many = Array.from({ length: 30 }, (_, i) => `ターミナル ${i + 1}`);
    const snapshot = projectsSnapshot([{ ...APP, tabs: many }]);
    snapshot.projects[0] = { ...snapshot.projects[0]!, activeTerminalId: "p-app-t30" };
    await open(page, snapshot);
    const pane = await box(page.getByTestId("pane-terminal"));
    const bar = await box(page.getByTestId("tab-bar"));
    const list = tabList(page);

    expect(bar.x + bar.width).toBeLessThanOrEqual(pane.x + pane.width + 0.5);
    expect(await list.evaluate((el) => el.scrollWidth - el.clientWidth)).toBeGreaterThan(0);
    const listBox = await box(list);
    expect(listBox.x + listBox.width).toBeLessThanOrEqual(pane.x + pane.width + 0.5);
    // 「+」は右ペインの中に見えたまま。
    const plus = await box(page.getByRole("button", { name: "新規ターミナル", exact: true }));
    expect(plus.x).toBeGreaterThanOrEqual(pane.x);
    expect(plus.x + plus.width).toBeLessThanOrEqual(pane.x + pane.width + 0.5);
    // 表示中のタブ(最後)がスクロールで見えている。
    await expect(tab(page, "ターミナル 30")).toBeInViewport();
    // タブバーの高さは 1 行のまま(折り返さない)。
    expect(bar.height).toBeLessThan(48);
    const host = await box(page.getByTestId("terminal-host"));
    expect(host.y).toBeGreaterThanOrEqual(bar.y + bar.height - 0.5);
  });

  test("狭いウィンドウでもタブバーとターミナルが崩れない", async ({ page }) => {
    await page.setViewportSize({ width: 600, height: 420 });
    await open(
      page,
      projectsSnapshot([{ ...APP, tabs: ["ターミナル 1", "ターミナル 2", "ターミナル 3", "ターミナル 4"] }], {
        layout: { sidebarWidth: 400, sidebarCollapsed: false, cardAreaHeight: 200 },
      }),
    );
    const pane = await box(page.getByTestId("pane-terminal"));
    const bar = await box(page.getByTestId("tab-bar"));
    const host = await box(page.getByTestId("terminal-host"));

    expect(Math.round(pane.width)).toBe(320);
    expect(bar.x + bar.width).toBeLessThanOrEqual(pane.x + pane.width + 0.5);
    expect(host.width).toBeGreaterThan(0);
    expect(host.height).toBeGreaterThan(0);
    expect(host.y).toBeGreaterThanOrEqual(bar.y + bar.height - 0.5);
    expect(Math.round(host.y + host.height)).toBeLessThanOrEqual(420);
    const plus = await box(page.getByRole("button", { name: "新規ターミナル", exact: true }));
    expect(plus.x + plus.width).toBeLessThanOrEqual(pane.x + pane.width + 0.5);
  });

  for (const colorScheme of ["light", "dark"] as const) {
    test(`ターミナルの地は置き場所を囲む_1_段だけが塗り_xterm_js_と置き場所は透明(${colorScheme})`, async ({ page }) => {
      // 置き場所の大きさは行の高さの倍数とは限らず、下端に 1 行に満たない余りが出る。余りが違う色の帯に見えないよう、
      // 地はターミナルの下の 1 段(タブのパネル)だけが塗る。M4.7 で xterm.js の地を透明にした(ADR-014 の 2。AC-G10 の書き換え)。
      await page.emulateMedia({ colorScheme });
      await open(page, projectsSnapshot([APP]));
      const host = page.getByTestId("terminal-host");
      await expect(host.locator(".xterm-viewport")).toHaveCount(1);
      expect(await background(host.locator(".xterm-viewport"))).toBe("rgba(0, 0, 0, 0)");
      expect(await background(host)).toBe("rgba(0, 0, 0, 0)");
      // 既定の透過 20 → 黒の不透明度 0.8(外観で変えない)
      expect(await background(page.getByRole("tabpanel"))).toBe("rgba(0, 0, 0, 0.8)");
    });
  }
});
