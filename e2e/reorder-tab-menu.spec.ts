// 並べ替えとタブの右クリックのメニュー(ADR-019): カードのドラッグでプロジェクトの並びを変える、タブ表示のタブを
// 同じ種類の中だけでドラッグで並べ替える、タブの右クリックの「タブを閉じる」「ほかのタブを閉じる」「これより右を閉じる」、
// 閉じるタブに動作中か未保存があるときの確認。
//
// ドラッグは pointer のイベントで作っている(src/ui/shell/useReorderDrag.ts)ので、page.mouse の down・move・up で再現できる。
// 偽の Rust 側(fixtures/tauriMock.ts)は写しを送り返さないので、並びが変わった結果はテストが写しを流して確かめる。

import { expect, type Locator, type Page, test } from "@playwright/test";

import type { AppSnapshot } from "../src/ipc/generated/AppSnapshot";
import {
  emitEvent,
  installTauriMock,
  projectsSnapshot,
  readCallArgs,
  readRecordedCalls,
  sampleEditorTab,
  sampleRunningTerminal,
  type SampleProject,
  setMockReply,
  withEditors,
} from "./fixtures/tauriMock";

const APP: SampleProject = {
  id: "p-app",
  name: "app",
  color: "green",
  tabs: ["ターミナル 1", "ターミナル 2", "ターミナル 3"],
};
const LOG: SampleProject = { id: "p-log", name: "ribbitlog", color: "orange" };
const WEB: SampleProject = { id: "p-web", name: "web", color: "purple" };

/** app にターミナル 3 枚とエディタ 2 枚(表示中はターミナル 1)。 */
function withTwoEditors(snapshot: AppSnapshot): AppSnapshot {
  return withEditors(
    snapshot,
    APP.id,
    [sampleEditorTab("e1", APP.id, "docs/a.md"), sampleEditorTab("e2", APP.id, "b.md")],
    null,
  );
}

async function open(page: Page, snapshot: AppSnapshot): Promise<void> {
  await installTauriMock(page, { get_app_snapshot: { ok: snapshot } });
  await page.goto("/");
  await expect
    .poll(async () => (await readRecordedCalls(page)).map((call) => call.command))
    .toContain("notify_app_ready");
}

async function center(locator: Locator): Promise<{ x: number; y: number }> {
  const b = await locator.boundingBox();
  if (b === null) throw new Error("要素の位置と大きさが取れない");
  return { x: b.x + b.width / 2, y: b.y + b.height / 2 };
}

/** source を押して、to まで少しずつ動かす(離さない)。 */
async function dragTo(page: Page, source: Locator, to: { x: number; y: number }): Promise<void> {
  const from = await center(source);
  await page.mouse.move(from.x, from.y);
  await page.mouse.down();
  await page.mouse.move(to.x, to.y, { steps: 8 });
}

const cards = (page: Page) => page.getByTestId("project-card");
const card = (page: Page, name: string) => cards(page).filter({ hasText: name });
const cardItem = (page: Page, id: string) => page.locator(`.card-list__item[data-reorder-id="${id}"]`);
const tabList = (page: Page) => page.getByRole("tablist", { name: "ターミナルのタブ" });
const terminalItem = (page: Page, id: string) => tabList(page).locator(`[data-reorder-group="terminal"][data-reorder-id="${id}"]`);
const editorItem = (page: Page, id: string) => tabList(page).locator(`[data-reorder-group="editor"][data-reorder-id="${id}"]`);
const menu = (page: Page) => page.getByRole("menu", { name: "タブ" });
const menuItem = (page: Page, name: string) => menu(page).getByRole("menuitem", { name, exact: true });

function shotPath(name: string): string {
  const configFile = test.info().config.configFile;
  if (configFile === undefined) throw new Error("Playwright の設定ファイルの場所が分からない");
  return `${configFile.slice(0, configFile.lastIndexOf("/"))}/testreport/e2e/reorder/${name}.png`;
}

test.afterEach(async ({ page }) => {
  const reported = await readCallArgs(page, "report_frontend_error");
  expect(reported.map((r) => r.message), "画面の失敗が Rust へ報告された").toEqual([]);
});

test.describe("プロジェクトの並べ替え", () => {
  test("カードを下へドラッグすると_move_project_を送り_届いた写しの並びになる", async ({ page }) => {
    const snapshot = projectsSnapshot([APP, LOG, WEB]);
    await open(page, snapshot);
    await expect(cards(page)).toHaveText(["app", "ribbitlog", "web"]);

    const last = await cardItem(page, WEB.id).boundingBox();
    if (last === null) throw new Error("カードが無い");
    await dragTo(page, card(page, "app"), { x: last.x + last.width / 2, y: last.y + last.height - 2 });
    // ドラッグ中: 掴んだカードが浮き、あいだのカードが上へ滑って最後に隙間が空く
    await expect(cardItem(page, APP.id)).toHaveAttribute("data-reorder-dragging", "true");
    await expect(cardItem(page, LOG.id)).toHaveAttribute("data-reorder-shift", "-1");
    await expect(cardItem(page, WEB.id)).toHaveAttribute("data-reorder-shift", "-1");
    await page.mouse.up();

    await expect.poll(() => readCallArgs(page, "move_project")).toEqual([{ projectId: APP.id, toIndex: 2 }]);
    // 離したときの click でカードを選ばない(app は選択中のまま、ほかも選ばない)
    expect(await readCallArgs(page, "select_project")).toEqual([]);
    await expect(page.locator("[data-reorder-shift], [data-reorder-dragging]")).toHaveCount(0);

    const [app, log, web] = snapshot.projects;
    if (app === undefined || log === undefined || web === undefined) throw new Error("写しが足りない");
    await emitEvent(page, "app-snapshot", { ...snapshot, projects: [log, web, app] });
    await expect(cards(page)).toHaveText(["ribbitlog", "web", "app"]);
  });

  test("しきい値を超えないなら選ぶだけで_Esc_では取り消す", async ({ page }) => {
    await open(page, projectsSnapshot([APP, LOG, WEB]));

    await card(page, "web").click();
    await expect.poll(() => readCallArgs(page, "select_project")).toEqual([{ projectId: WEB.id }]);

    const first = await center(card(page, "app"));
    await dragTo(page, card(page, "web"), { x: first.x, y: first.y - 6 });
    await expect(cardItem(page, APP.id)).toHaveAttribute("data-reorder-shift", "1");
    await expect(cardItem(page, LOG.id)).toHaveAttribute("data-reorder-shift", "1");
    await page.keyboard.press("Escape");
    await expect(page.locator("[data-reorder-shift], [data-reorder-dragging]")).toHaveCount(0);
    await page.mouse.up();
    expect(await readCallArgs(page, "move_project")).toEqual([]);
    expect(await readCallArgs(page, "select_project")).toHaveLength(1);
  });
});

test.describe("タブの並べ替え(同じ種類の中だけ)", () => {
  test("ターミナルのタブを先頭へドラッグすると_move_terminal_tab_を送る", async ({ page }) => {
    await open(page, withTwoEditors(projectsSnapshot([APP, LOG])));

    const first = await terminalItem(page, "p-app-t1").boundingBox();
    if (first === null) throw new Error("タブが無い");
    await dragTo(page, terminalItem(page, "p-app-t3"), { x: first.x + 4, y: first.y + first.height / 2 });
    await expect(terminalItem(page, "p-app-t1")).toHaveAttribute("data-reorder-shift", "1");
    await expect(terminalItem(page, "p-app-t2")).toHaveAttribute("data-reorder-shift", "1");
    await page.mouse.up();

    await expect.poll(() => readCallArgs(page, "move_terminal_tab")).toEqual([{ terminalId: "p-app-t3", toIndex: 0 }]);
    // 離したときの click でタブを選ばない
    expect(await readCallArgs(page, "select_terminal")).toEqual([]);
  });

  test("ターミナルのタブをエディタの上まで動かしてもターミナルの末尾まで", async ({ page }) => {
    await open(page, withTwoEditors(projectsSnapshot([APP, LOG])));

    const far = await center(editorItem(page, "e2"));
    await dragTo(page, terminalItem(page, "p-app-t1"), far);
    // ほかのターミナルだけが滑り、エディタは動かない
    await expect(terminalItem(page, "p-app-t3")).toHaveAttribute("data-reorder-shift", "-1");
    await expect(editorItem(page, "e1")).not.toHaveAttribute("data-reorder-shift");
    await page.mouse.up();

    await expect.poll(() => readCallArgs(page, "move_terminal_tab")).toEqual([{ terminalId: "p-app-t1", toIndex: 2 }]);
    expect(await readCallArgs(page, "move_editor_tab")).toEqual([]);
  });

  test("エディタのタブはエディタの中で動き_ターミナルの左へは行かない", async ({ page }) => {
    await open(page, withTwoEditors(projectsSnapshot([APP, LOG])));

    const leftmost = await terminalItem(page, "p-app-t1").boundingBox();
    if (leftmost === null) throw new Error("タブが無い");
    await dragTo(page, editorItem(page, "e2"), { x: leftmost.x + 2, y: leftmost.y + leftmost.height / 2 });
    await expect(editorItem(page, "e1")).toHaveAttribute("data-reorder-shift", "1");
    await expect(terminalItem(page, "p-app-t3")).not.toHaveAttribute("data-reorder-shift");
    await page.mouse.up();

    await expect.poll(() => readCallArgs(page, "move_editor_tab")).toEqual([{ editorId: "e2", toIndex: 0 }]);
    expect(await readCallArgs(page, "move_terminal_tab")).toEqual([]);
  });

  test("閉じるボタンからはドラッグを始めない", async ({ page }) => {
    await open(page, projectsSnapshot([APP]));

    const close = terminalItem(page, "p-app-t1").locator(".tab-bar__close");
    const target = await center(terminalItem(page, "p-app-t3"));
    await dragTo(page, close, target);
    await expect(page.locator("[data-reorder-dragging]")).toHaveCount(0);
    await page.mouse.up();
    expect(await readCallArgs(page, "move_terminal_tab")).toEqual([]);
  });
});

test.describe("タブの右クリックのメニュー", () => {
  test("3 つの項目を出し_右端のタブでは「これより右を閉じる」を選べない", async ({ page }) => {
    await open(page, withTwoEditors(projectsSnapshot([APP])));

    await editorItem(page, "e2").click({ button: "right" });
    await expect(menu(page).getByRole("menuitem")).toHaveText(["タブを閉じる", "ほかのタブを閉じる", "これより右を閉じる"]);
    await expect(menuItem(page, "これより右を閉じる")).toHaveAttribute("aria-disabled", "true");
    await page.keyboard.press("Escape");
    await expect(menu(page)).toHaveCount(0);
  });

  test("これより右を閉じる: 見えている並びで右のタブ(エディタも)を 1 回でまとめて閉じる", async ({ page }) => {
    await open(page, withTwoEditors(projectsSnapshot([APP])));

    await terminalItem(page, "p-app-t2").click({ button: "right" });
    await menuItem(page, "これより右を閉じる").click();

    await expect
      .poll(() => readCallArgs(page, "close_tabs"))
      .toEqual([{ projectId: APP.id, terminalIds: ["p-app-t3"], editorIds: ["e1", "e2"], confirmed: false }]);
    await expect(menu(page)).toHaveCount(0);
    await expect(page.getByTestId("close-tabs-dialog")).toHaveCount(0);
  });

  test("ほかのタブを閉じる・タブを閉じる", async ({ page }) => {
    await open(page, withTwoEditors(projectsSnapshot([APP])));

    await editorItem(page, "e1").click({ button: "right" });
    await menuItem(page, "ほかのタブを閉じる").click();
    await terminalItem(page, "p-app-t1").click({ button: "right" });
    await menuItem(page, "タブを閉じる").click();

    await expect
      .poll(() => readCallArgs(page, "close_tabs"))
      .toEqual([
        { projectId: APP.id, terminalIds: ["p-app-t1", "p-app-t2", "p-app-t3"], editorIds: ["e2"], confirmed: false },
        { projectId: APP.id, terminalIds: ["p-app-t1"], editorIds: [], confirmed: false },
      ]);
  });

  test("動作中か未保存を含むと確認を出し_閉じるで確認済みで送り直す_キャンセルでは送らない", async ({ page }) => {
    await open(page, withTwoEditors(projectsSnapshot([APP])));
    await setMockReply(page, "close_tabs", {
      ok: {
        kind: "needsConfirmation",
        running: [sampleRunningTerminal(APP.id, APP.name, "p-app-t3", "ターミナル 3")],
        unsaved: [{ editorId: "e1", projectName: APP.name, path: "docs/a.md" }],
      },
    });

    await terminalItem(page, "p-app-t2").click({ button: "right" });
    await menuItem(page, "これより右を閉じる").click();
    const dialog = page.getByTestId("close-tabs-dialog");
    await expect(dialog).toBeVisible();
    await expect(dialog.getByRole("heading")).toHaveText("3 個のタブを閉じますか?");
    await expect(page.getByTestId("close-tabs-running-list")).toHaveText("ターミナル 3");
    await expect(page.getByTestId("close-tabs-unsaved-list")).toHaveText("docs/a.md");
    // 既定のフォーカスはキャンセル(誤って閉じない)
    await expect(dialog.getByRole("button", { name: "キャンセル" })).toBeFocused();
    await dialog.getByRole("button", { name: "キャンセル" }).click();
    await expect(dialog).toHaveCount(0);
    expect(await readCallArgs(page, "close_tabs")).toHaveLength(1);

    await terminalItem(page, "p-app-t2").click({ button: "right" });
    await menuItem(page, "これより右を閉じる").click();
    await setMockReply(page, "close_tabs", { ok: { kind: "closed" } });
    await dialog.getByRole("button", { name: "閉じる" }).click();
    await expect(dialog).toHaveCount(0);
    const calls = await readCallArgs(page, "close_tabs");
    expect(calls.map((c) => c.confirmed)).toEqual([false, false, true]);
    expect(calls[2]).toEqual({ projectId: APP.id, terminalIds: ["p-app-t3"], editorIds: ["e1", "e2"], confirmed: true });
  });
});

test.describe("画像(ライト・ダーク)", () => {
  for (const colorScheme of ["light", "dark"] as const) {
    test(`タブのメニューとドラッグ中の画像を撮る_${colorScheme}`, async ({ page }) => {
      // 隙間を空ける動きの途中を撮らないよう、動きを減らす設定で撮る(動きは止まり、ずれた位置にすぐ並ぶ)
      await page.emulateMedia({ colorScheme, reducedMotion: "reduce" });
      await open(page, withTwoEditors(projectsSnapshot([APP, LOG, WEB])));

      await terminalItem(page, "p-app-t2").click({ button: "right" });
      await expect(menu(page)).toBeVisible();
      await page.screenshot({ path: shotPath(`tab-menu-${colorScheme}`) });
      await page.keyboard.press("Escape");

      const first = await terminalItem(page, "p-app-t1").boundingBox();
      if (first === null) throw new Error("タブが無い");
      await dragTo(page, terminalItem(page, "p-app-t3"), { x: first.x + 30, y: first.y + first.height / 2 });
      await expect(terminalItem(page, "p-app-t1")).toHaveAttribute("data-reorder-shift", "1");
      await page.screenshot({ path: shotPath(`tab-drag-${colorScheme}`) });
      await page.keyboard.press("Escape");
      await page.mouse.up();

      const last = await cardItem(page, WEB.id).boundingBox();
      if (last === null) throw new Error("カードが無い");
      await dragTo(page, card(page, "app"), { x: last.x + last.width / 2, y: last.y + last.height - 2 });
      await expect(cardItem(page, WEB.id)).toHaveAttribute("data-reorder-shift", "-1");
      await page.screenshot({ path: shotPath(`card-drag-${colorScheme}`) });
      await page.keyboard.press("Escape");
      await page.mouse.up();
    });
  }
});
