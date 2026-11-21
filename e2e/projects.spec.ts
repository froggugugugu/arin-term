// 画面の枠(T34c): ペインの分割、カード一覧、ツリーのヘッダ、色、カードのメニュー、削除の確認、知らせ。
//
// 偽の Rust 側(fixtures/tauriMock.ts)に写しを渡して画面を開き、部品が正しいコマンドを呼ぶこと、
// 写しのとおりに描くこと、レイアウトが崩れていないこと(要素の位置と大きさ)を見る。
// コマンドの結果は写しで届く(偽物は写しを送り返さない)ので、結果の表示を見るテストは写しのイベントを自分で流す。

import { expect, type Locator, type Page, test } from "@playwright/test";

import {
  emitEvent,
  installTauriMock,
  projectsSnapshot,
  readCallArgs,
  readRecordedCalls,
  type SampleProject,
  type TauriMockConfig,
} from "./fixtures/tauriMock";
import type { AppSnapshot } from "../src/ipc/generated/AppSnapshot";

const APP: SampleProject = { id: "p-app", name: "app", color: "green", displayPath: "~/dev/app" };
const LOG: SampleProject = { id: "p-log", name: "ribbitlog", color: "orange", displayPath: "~/dev/ribbitlog" };
const GONE: SampleProject = { id: "p-gone", name: "gone", color: "purple", displayPath: "~/dev/gone", missing: true };

/** 画面のコンソールのエラーと、捕まえられなかった例外を集める。 */
function collectPageErrors(page: Page): string[] {
  const errors: string[] = [];
  page.on("console", (message) => {
    if (message.type() === "error") errors.push(message.text());
  });
  page.on("pageerror", (error) => errors.push(error.message));
  return errors;
}

/** 写しを渡して画面を開き、準備完了まで待つ。 */
async function open(page: Page, snapshot: AppSnapshot, overrides: Partial<TauriMockConfig> = {}): Promise<void> {
  await installTauriMock(page, { get_app_snapshot: { ok: snapshot }, ...overrides });
  await page.goto("/");
  await expect
    .poll(async () => (await readRecordedCalls(page)).map((call) => call.command))
    .toContain("notify_app_ready");
}

const cards = (page: Page) => page.getByTestId("project-card");
const card = (page: Page, name: string) => cards(page).filter({ hasText: name });

/** 要素の位置と大きさ。無ければ失敗にする。 */
async function box(locator: Locator) {
  const b = await locator.boundingBox();
  if (b === null) throw new Error("要素の位置と大きさが取れない(表示されていない)");
  return b;
}

/** 要素の中心から (dx, dy) だけマウスでドラッグする。離す前に届いたコマンドの数を返す。 */
async function drag(page: Page, handle: Locator, dx: number, dy: number, command: "set_pane_sizes"): Promise<number> {
  const b = await box(handle);
  const x = b.x + b.width / 2;
  const y = b.y + b.height / 2;
  await page.mouse.move(x, y);
  await page.mouse.down();
  await page.mouse.move(x + dx / 2, y + dy / 2, { steps: 4 });
  await page.mouse.move(x + dx, y + dy, { steps: 4 });
  const before = (await readCallArgs(page, command)).length;
  await page.mouse.up();
  return before;
}

/** 計算済みの背景色。 */
const background = (locator: Locator) => locator.evaluate((el) => getComputedStyle(el).backgroundColor);
/** プロジェクト色の印(カード・ツリーのヘッダ・タブの名前の前の小さな丸)の色。 */
const dotColor = (locator: Locator) => background(locator.locator(".project-dot"));

test.describe("カード一覧(FR-008)", () => {
  test("1_プロジェクトに_1_枚のカードが出て選択中が強調される", async ({ page }) => {
    const errors = collectPageErrors(page);
    await open(page, projectsSnapshot([APP, LOG], { selectedProjectId: LOG.id }));

    await expect(cards(page)).toHaveCount(2);
    await expect(cards(page).nth(0)).toContainText("app");
    await expect(cards(page).nth(1)).toContainText("ribbitlog");
    // 選択中は aria-current と、色以外の形(太字)でも分かる。
    await expect(card(page, "ribbitlog")).toHaveAttribute("aria-current", "true");
    await expect(card(page, "app")).not.toHaveAttribute("aria-current", "true");
    const style = (locator: Locator) =>
      locator.evaluate((el) => {
        const s = getComputedStyle(el);
        return { weight: s.fontWeight, outline: s.outlineStyle, border: s.borderLeftWidth };
      });
    expect(await style(card(page, "ribbitlog"))).not.toEqual(await style(card(page, "app")));
    await expect(page.getByTestId("tree-header")).toHaveText("ribbitlog");
    expect(errors).toEqual([]);
  });

  test("カードの合計の高さが上段を超えると上段だけがスクロールしツリーの位置と高さが変わらない", async ({ page }) => {
    const many = Array.from({ length: 15 }, (_, i) => ({ id: `p-${i}`, name: `project-${i}` }));
    await open(page, projectsSnapshot(many));
    const list = page.getByTestId("card-list");
    const tree = page.getByTestId("tree-area");

    const overflow = await list.evaluate((el) => el.scrollHeight - el.clientHeight);
    expect(overflow).toBeGreaterThan(0);
    const treeBefore = await box(tree);
    const cardArea = await box(page.getByTestId("card-area"));

    await list.evaluate((el) => {
      el.scrollTop = el.scrollHeight;
    });
    await expect(card(page, "project-14")).toBeInViewport();

    expect(await box(tree)).toEqual(treeBefore);
    expect(await box(page.getByTestId("card-area"))).toEqual(cardArea);
    // カード領域は写しの高さ(200px)のまま。
    expect(Math.round(cardArea.height)).toBe(200);
    // ツリーはカード領域のすぐ下から始まる(重ならない)。
    expect(treeBefore.y).toBeGreaterThanOrEqual(cardArea.y + cardArea.height - 1);
  });

  test("カードをクリックすると選択のコマンドが呼ばれツリーのヘッダが切り替わる", async ({ page }) => {
    await open(page, projectsSnapshot([APP, LOG]));
    await expect(page.getByTestId("tree-header")).toHaveText("app");

    await card(page, "ribbitlog").click();

    await expect.poll(() => readCallArgs(page, "select_project")).toEqual([{ projectId: LOG.id }]);
    await emitEvent(page, "app-snapshot", projectsSnapshot([APP, LOG], { selectedProjectId: LOG.id }));
    await expect(page.getByTestId("tree-header")).toHaveText("ribbitlog");
    await expect(card(page, "ribbitlog")).toHaveAttribute("aria-current", "true");
  });

  test("選択中のカードをクリックしても選択のコマンドを呼ばない", async ({ page }) => {
    await open(page, projectsSnapshot([APP, LOG]));
    await card(page, "app").click();
    await card(page, "ribbitlog").click();
    await expect.poll(() => readCallArgs(page, "select_project")).toEqual([{ projectId: LOG.id }]);
  });

  test("フォルダを追加のボタンで登録のコマンドが呼ばれる", async ({ page }) => {
    await open(page, projectsSnapshot([APP]));
    await page.getByRole("button", { name: "フォルダを追加" }).click();
    await expect.poll(() => readCallArgs(page, "add_project_by_dialog")).toHaveLength(1);
  });

  test("プロジェクトが_0_件ならフォルダを追加の案内が出る", async ({ page }) => {
    const errors = collectPageErrors(page);
    await open(page, projectsSnapshot([]));

    await expect(cards(page)).toHaveCount(0);
    await expect(page.getByTestId("projects-empty")).toContainText("プロジェクトがありません");
    await expect(page.getByTestId("tree-header")).toHaveCount(0);
    await page.getByRole("button", { name: "フォルダを追加" }).click();
    await expect.poll(() => readCallArgs(page, "add_project_by_dialog")).toHaveLength(1);
    expect(errors).toEqual([]);
  });

  test("ツールチップに_~_の表示用のパスが出る", async ({ page }) => {
    await open(page, projectsSnapshot([APP, LOG]));
    await expect(card(page, "app")).toHaveAttribute("title", "~/dev/app");
    await expect(card(page, "ribbitlog")).toHaveAttribute("title", "~/dev/ribbitlog");
    // パスはカードに常時は出さない(C-03)。
    await expect(card(page, "app")).not.toContainText("~/dev/app");
  });
});

test.describe("カードの右クリックのメニュー", () => {
  test("カードを右クリックすると絶対パスをコピーがあり_選ぶとコピーのコマンドが呼ばれ選択は変わらない", async ({
    page,
  }) => {
    await open(page, projectsSnapshot([APP, LOG]));

    await card(page, "ribbitlog").click({ button: "right" });
    const menu = page.getByRole("menu", { name: "ribbitlog" });
    await expect(menu).toBeVisible();
    await expect(menu.getByRole("menuitem")).toHaveText(["新規ターミナル", "絶対パスをコピー", "色", "一覧から削除"]);

    await menu.getByRole("menuitem", { name: "絶対パスをコピー" }).click();

    await expect.poll(() => readCallArgs(page, "copy_project_path")).toEqual([{ projectId: LOG.id }]);
    await expect(menu).toHaveCount(0);
    expect(await readCallArgs(page, "select_project")).toEqual([]);
    await expect(card(page, "app")).toHaveAttribute("aria-current", "true");
  });

  test("選んでいないカードの新規ターミナルは_そのプロジェクトを渡して新しいタブのコマンドを呼ぶ", async ({ page }) => {
    // ribbitlog は exit などで 0 枚になっている(選択中は app)。
    await open(page, projectsSnapshot([APP, { ...LOG, tabs: [] }]));

    await card(page, "ribbitlog").click({ button: "right" });
    const menu = page.getByRole("menu", { name: "ribbitlog" });
    await menu.getByRole("menuitem", { name: "新規ターミナル" }).click();

    await expect.poll(() => readCallArgs(page, "new_terminal")).toEqual([{ projectId: LOG.id }]);
    await expect(menu).toHaveCount(0);
    // 選ぶのは Rust(同じコマンドの中で選んでから開く)。画面は選択のコマンドを別に送らない。
    expect(await readCallArgs(page, "select_project")).toEqual([]);
  });

  test("色を選ぶと色の変更のコマンドが呼ばれる", async ({ page }) => {
    await open(page, projectsSnapshot([APP]));

    await card(page, "app").click({ button: "right" });
    await page.getByRole("menuitem", { name: "色" }).click();
    const colors = page.getByRole("menu", { name: "色" });
    await expect(colors.getByRole("menuitemradio")).toHaveCount(8);
    // いまの色(緑)に印が付く。
    await expect(colors.getByRole("menuitemradio", { name: "緑", exact: true })).toHaveAttribute("aria-checked", "true");
    await expect(colors.getByRole("menuitemradio", { name: "赤" })).toHaveAttribute("aria-checked", "false");

    await colors.getByRole("menuitemradio", { name: "青緑" }).click();

    await expect.poll(() => readCallArgs(page, "set_project_color")).toEqual([{ projectId: APP.id, color: "teal" }]);
    await expect(page.getByRole("menu")).toHaveCount(0);
  });

  test("メニューの外をクリックすると閉じ_何も呼ばない", async ({ page }) => {
    await open(page, projectsSnapshot([APP]));
    await card(page, "app").click({ button: "right" });
    await expect(page.getByRole("menu")).toBeVisible();

    await page.getByTestId("tree-area").click({ position: { x: 20, y: 60 } });

    await expect(page.getByRole("menu")).toHaveCount(0);
    const commands = (await readRecordedCalls(page)).map((c) => c.command);
    expect(commands.filter((c) => c.includes("project"))).toEqual([]);
  });

  test("メニューはウィンドウの中に収まる", async ({ page }) => {
    await open(page, projectsSnapshot([APP]));
    const viewport = page.viewportSize();
    if (viewport === null) throw new Error("ビューポートの大きさが取れない");
    const target = await box(card(page, "app"));
    // カードの右下の端で開いても、メニューはウィンドウからはみ出さない。
    await page.mouse.click(target.x + target.width - 2, target.y + target.height - 2, { button: "right" });
    await page.getByRole("menuitem", { name: "色" }).click();
    for (const menu of await page.getByRole("menu").all()) {
      const b = await box(menu);
      expect(b.x).toBeGreaterThanOrEqual(0);
      expect(b.y).toBeGreaterThanOrEqual(0);
      expect(b.x + b.width).toBeLessThanOrEqual(viewport.width);
      expect(b.y + b.height).toBeLessThanOrEqual(viewport.height);
    }
  });
});

/** 要素に右クリックのイベントを送り、既定の動作(WebKit の標準のメニュー)が止められたかを返す。 */
async function contextMenuPrevented(locator: Locator): Promise<boolean> {
  return locator.evaluate(
    (el) => !el.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 10, clientY: 10 })),
  );
}

test.describe("プロジェクト欄の余白の右クリックのメニュー", () => {
  test("一覧の余白を右クリックするとフォルダを追加…だけのメニューが出て_選ぶと登録のコマンドが呼ばれる", async ({
    page,
  }) => {
    await open(page, projectsSnapshot([APP]));

    const list = await box(page.getByTestId("card-list"));
    await page.mouse.click(list.x + list.width / 2, list.y + list.height - 4, { button: "right" });
    const menu = page.getByRole("menu", { name: "プロジェクト" });
    await expect(menu).toBeVisible();
    await expect(menu.getByRole("menuitem")).toHaveText(["フォルダを追加…"]);

    await menu.getByRole("menuitem", { name: "フォルダを追加…" }).click();

    await expect.poll(() => readCallArgs(page, "add_project_by_dialog")).toHaveLength(1);
    await expect(menu).toHaveCount(0);
  });

  test("見出しの右クリックでも同じメニューが出る_カードの右クリックはカードのメニューのまま", async ({ page }) => {
    await open(page, projectsSnapshot([APP]));

    await page.getByRole("heading", { name: "プロジェクト" }).click({ button: "right" });
    await expect(page.getByRole("menu", { name: "プロジェクト" })).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(page.getByRole("menu", { name: "プロジェクト" })).toHaveCount(0);

    await card(page, "app").click({ button: "right" });
    await expect(page.getByRole("menu", { name: "app" })).toBeVisible();
    await expect(page.getByRole("menu", { name: "プロジェクト" })).toHaveCount(0);
  });

  test("プロジェクトが_0_件の案内の上でも出る", async ({ page }) => {
    await open(page, projectsSnapshot([]));
    await page.getByTestId("projects-empty").click({ button: "right", position: { x: 4, y: 4 } });
    const menu = page.getByRole("menu", { name: "プロジェクト" });
    await menu.getByRole("menuitem", { name: "フォルダを追加…" }).click();
    await expect.poll(() => readCallArgs(page, "add_project_by_dialog")).toHaveLength(1);
  });
});

test.describe("標準の右クリックのメニュー(再読み込みを出さない)", () => {
  test("ターミナル・ツリーのヘッダ・画面の地では標準のメニューを止める", async ({ page }) => {
    await open(page, projectsSnapshot([APP]));
    await expect(page.getByTestId("terminal-host")).toBeVisible();

    expect(await contextMenuPrevented(page.getByTestId("terminal-host"))).toBe(true);
    expect(await contextMenuPrevented(page.getByTestId("tree-header"))).toBe(true);
    expect(await contextMenuPrevented(page.locator("body"))).toBe(true);
    // xterm.js は右クリックで入力用の textarea をポインタの下へ動かす。そこでも止める。
    expect(await contextMenuPrevented(page.locator(".xterm-helper-textarea"))).toBe(true);
  });

  test("文字の入力欄では標準のメニューを残す(カット・コピー・ペーストのため)", async ({ page }) => {
    await open(page, projectsSnapshot([APP]));
    const input = await page.evaluate(() => {
      const el = document.createElement("input");
      el.id = "e2e-plain-input";
      document.body.append(el);
      return el.id;
    });
    expect(await contextMenuPrevented(page.locator(`#${input}`))).toBe(false);
  });
});

test.describe("見つからないプロジェクト(FR-007)", () => {
  test("見つからないカードは表示が変わり再リンクと一覧から削除を選べる", async ({ page }) => {
    await open(page, projectsSnapshot([APP, GONE]));

    // 色だけでなく文字でも分かる。
    await expect(card(page, "gone")).toContainText("見つかりません");
    await expect(card(page, "app")).not.toContainText("見つかりません");

    await card(page, "gone").click({ button: "right" });
    const menu = page.getByRole("menu", { name: "gone" });
    await expect(menu.getByRole("menuitem")).toHaveText(["再リンク…", "一覧から削除"]);
    await menu.getByRole("menuitem", { name: "再リンク…" }).click();
    await expect.poll(() => readCallArgs(page, "relink_project")).toEqual([{ projectId: GONE.id }]);
  });

  test("選択中のプロジェクトが見つからないとき_ツリーの位置に案内と再リンク_一覧から削除が出る", async ({ page }) => {
    await open(page, projectsSnapshot([APP, GONE], { selectedProjectId: GONE.id }));

    const tree = page.getByTestId("tree-area");
    await expect(page.getByTestId("tree-header")).toHaveText("gone");
    await expect(tree).toContainText("フォルダが見つかりません");
    await tree.getByRole("button", { name: "再リンク…" }).click();
    await expect.poll(() => readCallArgs(page, "relink_project")).toEqual([{ projectId: GONE.id }]);
    await tree.getByRole("button", { name: "一覧から削除" }).click();
    await expect.poll(() => readCallArgs(page, "remove_project")).toEqual([{ projectId: GONE.id, confirmed: false }]);
  });

  test("見つかるプロジェクトのツリーの位置には案内を出さない(ツリーの中身は M3)", async ({ page }) => {
    await open(page, projectsSnapshot([APP]));
    await expect(page.getByTestId("tree-area")).not.toContainText("フォルダが見つかりません");
  });
});

test.describe("削除の確認(FR-003)", () => {
  const running = [
    { terminalId: "p-app-t1", projectId: APP.id, projectName: "app", label: "ターミナル 1" },
    { terminalId: "p-app-t2", projectId: APP.id, projectName: "app", label: "ターミナル 2" },
  ];
  const needsConfirmation: Partial<TauriMockConfig> = {
    remove_project: { ok: { kind: "needsConfirmation", running, unsaved: [] } },
  };

  async function openRemoveDialog(page: Page) {
    await open(page, projectsSnapshot([{ ...APP, tabs: ["ターミナル 1", "ターミナル 2"] }]), needsConfirmation);
    await card(page, "app").click({ button: "right" });
    await page.getByRole("menuitem", { name: "一覧から削除" }).click();
    const dialog = page.getByRole("alertdialog", { name: "「app」を一覧から削除しますか?" });
    await expect(dialog).toBeVisible();
    return dialog;
  }

  test("動作中があると確認が出て動作中のターミナルが並び_中止すると削除のコマンドが呼ばれない", async ({ page }) => {
    const dialog = await openRemoveDialog(page);

    await expect(dialog.getByRole("listitem")).toHaveText(["ターミナル 1", "ターミナル 2"]);
    await expect(dialog).toContainText("フォルダとその中身は削除されません");
    // 誤って消さないよう、最初のフォーカスは「中止」。
    await expect(dialog.getByRole("button", { name: "中止" })).toBeFocused();

    await dialog.getByRole("button", { name: "中止" }).click();

    await expect(dialog).toHaveCount(0);
    expect(await readCallArgs(page, "remove_project")).toEqual([{ projectId: APP.id, confirmed: false }]);
  });

  test("削除を選ぶと確認済みで削除のコマンドが呼ばれる", async ({ page }) => {
    const dialog = await openRemoveDialog(page);
    await dialog.getByRole("button", { name: "削除" }).click();
    // 偽物は確認済みの削除にも同じ応答(確認が要る)を返すので、ダイアログが開き直る。ここでは呼んだコマンドだけを見る。
    await expect
      .poll(() => readCallArgs(page, "remove_project"))
      .toEqual([
        { projectId: APP.id, confirmed: false },
        { projectId: APP.id, confirmed: true },
      ]);
  });

  test("Esc で閉じ_Tab のフォーカスはダイアログの中を回る", async ({ page }) => {
    const dialog = await openRemoveDialog(page);
    const cancel = dialog.getByRole("button", { name: "中止" });
    const confirm = dialog.getByRole("button", { name: "削除" });

    await page.keyboard.press("Tab");
    await expect(confirm).toBeFocused();
    await page.keyboard.press("Tab");
    await expect(cancel).toBeFocused();
    await page.keyboard.press("Shift+Tab");
    await expect(confirm).toBeFocused();

    await page.keyboard.press("Escape");
    await expect(dialog).toHaveCount(0);
    expect(await readCallArgs(page, "remove_project")).toEqual([{ projectId: APP.id, confirmed: false }]);
  });

  test("動作中が無ければ確認を出さずに削除のコマンドだけが呼ばれる", async ({ page }) => {
    await open(page, projectsSnapshot([APP]));
    await card(page, "app").click({ button: "right" });
    await page.getByRole("menuitem", { name: "一覧から削除" }).click();
    await expect.poll(() => readCallArgs(page, "remove_project")).toEqual([{ projectId: APP.id, confirmed: false }]);
    await expect(page.getByRole("alertdialog")).toHaveCount(0);
  });
});

test.describe("ペインの境界(FR-009)", () => {
  test("上段と下段の境界をドラッグすると寸法のコマンドが呼ばれ最小で止まる", async ({ page }) => {
    await open(page, projectsSnapshot([APP]));
    const divider = page.getByRole("separator", { name: "カード一覧とツリーの境界" });

    const before = await drag(page, divider, 0, 100, "set_pane_sizes");

    // 動かしている間は送らず、離したときに 1 回送る。
    expect(before).toBe(0);
    await expect.poll(() => readCallArgs(page, "set_pane_sizes")).toEqual([{ sidebarWidth: 260, cardAreaHeight: 300 }]);
    // 写しが届くまでは、離した位置のまま描く。
    expect(Math.round((await box(page.getByTestId("card-area"))).height)).toBe(300);

    // 上へ大きく動かすと、カード領域の最小(72px)で止まる。
    await drag(page, divider, 0, -1000, "set_pane_sizes");
    await expect.poll(async () => (await readCallArgs(page, "set_pane_sizes")).at(-1)).toEqual({
      sidebarWidth: 260,
      cardAreaHeight: 72,
    });

    // 下へ大きく動かすと、ツリー領域の最小(120px)を残して止まる。
    await drag(page, divider, 0, 2000, "set_pane_sizes");
    const sidebar = await box(page.getByTestId("pane-sidebar"));
    await expect.poll(async () => (await readCallArgs(page, "set_pane_sizes")).at(-1)).toEqual({
      sidebarWidth: 260,
      cardAreaHeight: Math.round(sidebar.height) - 120,
    });
    expect(Math.round((await box(page.getByTestId("tree-area"))).height)).toBe(120);
  });

  test("左右の境界をドラッグすると寸法のコマンドが呼ばれ最小で止まる", async ({ page }) => {
    await open(page, projectsSnapshot([APP]));
    const divider = page.getByRole("separator", { name: "左右のペインの境界" });
    const viewport = page.viewportSize();
    if (viewport === null) throw new Error("ビューポートの大きさが取れない");

    expect(await drag(page, divider, 100, 0, "set_pane_sizes")).toBe(0);
    await expect.poll(() => readCallArgs(page, "set_pane_sizes")).toEqual([{ sidebarWidth: 360, cardAreaHeight: 200 }]);
    expect(Math.round((await box(page.getByTestId("pane-sidebar"))).width)).toBe(360);

    await drag(page, divider, -1000, 0, "set_pane_sizes");
    await expect.poll(async () => (await readCallArgs(page, "set_pane_sizes")).at(-1)).toEqual({
      sidebarWidth: 180,
      cardAreaHeight: 200,
    });

    await drag(page, divider, 3000, 0, "set_pane_sizes");
    await expect.poll(async () => (await readCallArgs(page, "set_pane_sizes")).at(-1)).toEqual({
      sidebarWidth: viewport.width - 320,
      cardAreaHeight: 200,
    });
    expect(Math.round((await box(page.getByTestId("pane-terminal"))).width)).toBe(320);
  });

  test("境界はキーボードの矢印でも動かせる", async ({ page }) => {
    await open(page, projectsSnapshot([APP]));
    const vertical = page.getByRole("separator", { name: "左右のペインの境界" });
    const horizontal = page.getByRole("separator", { name: "カード一覧とツリーの境界" });
    await expect(vertical).toHaveAttribute("aria-valuenow", "260");

    await vertical.focus();
    await page.keyboard.press("ArrowRight");
    await expect.poll(async () => (await readCallArgs(page, "set_pane_sizes")).at(-1)).toEqual({
      sidebarWidth: 276,
      cardAreaHeight: 200,
    });
    await horizontal.focus();
    await page.keyboard.press("ArrowUp");
    // 写しが届くまでは、送った幅(276)を表示値として使い続ける。
    await expect.poll(async () => (await readCallArgs(page, "set_pane_sizes")).at(-1)).toEqual({
      sidebarWidth: 276,
      cardAreaHeight: 184,
    });
  });

  test("写しの寸法で描く", async ({ page }) => {
    await open(
      page,
      projectsSnapshot([APP], { layout: { sidebarWidth: 340, sidebarCollapsed: false, cardAreaHeight: 150 } }),
    );
    expect(Math.round((await box(page.getByTestId("pane-sidebar"))).width)).toBe(340);
    expect(Math.round((await box(page.getByTestId("card-area"))).height)).toBe(150);
  });

  test("左ペインを折りたためる_右ペインが全幅になり_戻すと前の幅", async ({ page }) => {
    await open(page, projectsSnapshot([APP]));
    const viewport = page.viewportSize();
    if (viewport === null) throw new Error("ビューポートの大きさが取れない");

    await page.getByRole("button", { name: "左ペインを隠す" }).click();

    await expect.poll(() => readCallArgs(page, "set_sidebar_collapsed")).toEqual([{ collapsed: true }]);
    await expect(page.getByTestId("pane-sidebar")).toBeHidden();
    await expect(page.getByRole("separator", { name: "左右のペインの境界" })).toHaveCount(0);
    expect(Math.round((await box(page.getByTestId("pane-terminal"))).width)).toBe(viewport.width);
    expect((await box(page.getByTestId("terminal-host"))).width).toBeGreaterThan(viewport.width - 40);

    // Rust が写しを返す(保存値は幅を覚えている)。
    await emitEvent(
      page,
      "app-snapshot",
      projectsSnapshot([APP], { layout: { sidebarWidth: 260, sidebarCollapsed: true, cardAreaHeight: 200 } }),
    );
    await page.getByRole("button", { name: "左ペインを表示" }).click();
    await expect.poll(() => readCallArgs(page, "set_sidebar_collapsed")).toEqual([
      { collapsed: true },
      { collapsed: false },
    ]);
    expect(Math.round((await box(page.getByTestId("pane-sidebar"))).width)).toBe(260);
  });
});

test.describe("レイアウトの崩れ(要素の位置と大きさ)", () => {
  test("ペインがウィンドウからはみ出さず_カードが重ならず_ターミナルの置き場所が_0_でない", async ({ page }) => {
    await open(page, projectsSnapshot([APP, LOG, GONE]));
    const viewport = page.viewportSize();
    if (viewport === null) throw new Error("ビューポートの大きさが取れない");

    const sidebar = await box(page.getByTestId("pane-sidebar"));
    const terminal = await box(page.getByTestId("pane-terminal"));
    for (const pane of [sidebar, terminal]) {
      expect(pane.x).toBeGreaterThanOrEqual(0);
      expect(pane.y).toBeGreaterThanOrEqual(0);
      expect(pane.x + pane.width).toBeLessThanOrEqual(viewport.width + 0.5);
      expect(pane.y + pane.height).toBeLessThanOrEqual(viewport.height + 0.5);
    }
    // 左右のペインは隣り合い、重ならない。
    expect(terminal.x).toBeGreaterThanOrEqual(sidebar.x + sidebar.width - 0.5);
    expect(Math.round(sidebar.width + terminal.width)).toBe(viewport.width);

    // カードは上から順に並び、重ならない。カードの幅はカード領域の中。
    const area = await box(page.getByTestId("card-area"));
    const boxes = await Promise.all((await cards(page).all()).map((c) => box(c)));
    expect(boxes).toHaveLength(3);
    for (let i = 1; i < boxes.length; i++) {
      const prev = boxes[i - 1];
      const cur = boxes[i];
      if (prev === undefined || cur === undefined) throw new Error("カードが足りない");
      expect(cur.y).toBeGreaterThanOrEqual(prev.y + prev.height - 0.5);
    }
    for (const b of boxes) {
      expect(b.height).toBeGreaterThan(16);
      expect(b.x).toBeGreaterThanOrEqual(area.x);
      expect(b.x + b.width).toBeLessThanOrEqual(area.x + area.width + 0.5);
    }

    // ツリーはカード領域の下に、左ペインの下端まで。
    const tree = await box(page.getByTestId("tree-area"));
    expect(tree.y).toBeGreaterThanOrEqual(area.y + area.height - 0.5);
    expect(Math.round(tree.y + tree.height)).toBe(Math.round(sidebar.y + sidebar.height));

    const host = await box(page.getByTestId("terminal-host"));
    expect(host.width).toBeGreaterThan(0);
    expect(host.height).toBeGreaterThan(0);
    expect(host.x).toBeGreaterThanOrEqual(terminal.x - 0.5);
    expect(host.x + host.width).toBeLessThanOrEqual(terminal.x + terminal.width + 0.5);
  });

  test("狭いウィンドウでも各ペインが最小寸法を守り_ウィンドウからはみ出さない", async ({ page }) => {
    await page.setViewportSize({ width: 600, height: 420 });
    await open(
      page,
      projectsSnapshot([APP, LOG], { layout: { sidebarWidth: 400, sidebarCollapsed: false, cardAreaHeight: 300 } }),
    );

    const sidebar = await box(page.getByTestId("pane-sidebar"));
    const terminal = await box(page.getByTestId("pane-terminal"));
    const area = await box(page.getByTestId("card-area"));
    const tree = await box(page.getByTestId("tree-area"));
    // 保存値(400・300)は入らないので、表示だけを縮める。
    expect(Math.round(sidebar.width)).toBe(600 - 320);
    expect(Math.round(terminal.width)).toBe(320);
    expect(Math.round(terminal.x + terminal.width)).toBeLessThanOrEqual(600);
    expect(area.height).toBeGreaterThanOrEqual(72);
    expect(Math.round(tree.height)).toBe(120);
    expect(Math.round(tree.y + tree.height)).toBeLessThanOrEqual(420);
    const host = await box(page.getByTestId("terminal-host"));
    expect(host.width).toBeGreaterThan(0);
    expect(host.height).toBeGreaterThan(0);
    // 表示を縮めただけでは保存値を送らない。
    expect(await readCallArgs(page, "set_pane_sizes")).toEqual([]);
  });

  test("ウィンドウを狭めると表示が縮み_広げ直すと保存値に戻る", async ({ page }) => {
    await open(page, projectsSnapshot([APP], { layout: { sidebarWidth: 500, sidebarCollapsed: false, cardAreaHeight: 200 } }));
    expect(Math.round((await box(page.getByTestId("pane-sidebar"))).width)).toBe(500);
    await page.setViewportSize({ width: 700, height: 600 });
    await expect.poll(async () => Math.round((await box(page.getByTestId("pane-sidebar"))).width)).toBe(380);
    await page.setViewportSize({ width: 1280, height: 720 });
    await expect.poll(async () => Math.round((await box(page.getByTestId("pane-sidebar"))).width)).toBe(500);
  });
});

test.describe("色(FR-060)", () => {
  for (const colorScheme of ["light", "dark"] as const) {
    test(`カードとツリーのヘッダのプロジェクト色の印が同じ色相(${colorScheme})`, async ({ page }) => {
      await page.emulateMedia({ colorScheme });
      await open(page, projectsSnapshot([APP, LOG]));

      const cardColor = await dotColor(card(page, "app"));
      expect(cardColor).toBe(await dotColor(page.getByTestId("tree-header")));
      // 他のプロジェクトとは色が違う。
      expect(cardColor).not.toBe(await dotColor(card(page, "ribbitlog")));
      // 印の色は透明でない。
      expect(cardColor).not.toBe("rgba(0, 0, 0, 0)");
    });
  }

  test("ライトとダークで色が切り替わる(OS の外観に追従)", async ({ page }) => {
    await page.emulateMedia({ colorScheme: "light" });
    await open(page, projectsSnapshot([APP]));
    const light = await dotColor(card(page, "app"));
    const lightBody = await background(page.getByTestId("app-shell"));
    await page.emulateMedia({ colorScheme: "dark" });
    await expect.poll(() => dotColor(card(page, "app"))).not.toBe(light);
    await expect.poll(() => background(page.getByTestId("app-shell"))).not.toBe(lightBody);
  });
});

test.describe("知らせ", () => {
  const notice = "状態ファイルを読めなかったため state.json.broken-20261115T093000Z に退避し、空の状態で起動しました";

  test("退避の知らせが_1_行で出て閉じられる", async ({ page }) => {
    await page.setViewportSize({ width: 640, height: 480 });
    await open(page, projectsSnapshot([APP], { notice }));

    const banner = page.getByTestId("notice");
    await expect(banner).toBeVisible();
    await expect(banner).toContainText("state.json.broken-20261115T093000Z");
    // 長くても 1 行(折り返さない)で、全文はツールチップで読める。
    const text = banner.getByTestId("notice-text");
    expect(await text.evaluate((el) => getComputedStyle(el).whiteSpace)).toBe("nowrap");
    expect((await box(banner)).height).toBeLessThan(40);
    await expect(text).toHaveAttribute("title", notice);

    await banner.getByRole("button", { name: "知らせを閉じる" }).click();
    await expect(banner).toHaveCount(0);

    // 同じ知らせの写しがもう一度届いても出さない。
    await emitEvent(page, "app-snapshot", projectsSnapshot([APP], { notice }));
    await expect(page.getByTestId("notice")).toHaveCount(0);
  });

  test("知らせが無ければ何も出さない", async ({ page }) => {
    await open(page, projectsSnapshot([APP]));
    await expect(page.getByTestId("notice")).toHaveCount(0);
  });
});

test.describe("キーボード", () => {
  test("カードを上下の矢印で移り_Enter_で選択できる", async ({ page }) => {
    await open(page, projectsSnapshot([APP, LOG, GONE]));
    await card(page, "app").focus();
    // macOS の既定では Tab がボタンに止まらない(キーボードナビゲーションの設定による)ので、矢印でも移れる。
    await page.keyboard.press("ArrowDown");
    await expect(card(page, "ribbitlog")).toBeFocused();
    await page.keyboard.press("ArrowDown");
    await expect(card(page, "gone")).toBeFocused();
    // 端では止まる。
    await page.keyboard.press("ArrowDown");
    await expect(card(page, "gone")).toBeFocused();
    await page.keyboard.press("ArrowUp");
    await expect(card(page, "ribbitlog")).toBeFocused();
    await page.keyboard.press("Enter");
    await expect.poll(() => readCallArgs(page, "select_project")).toEqual([{ projectId: LOG.id }]);
  });

  test("Shift+F10_でメニューが開き_矢印で動いて_Enter_で選べ_Esc_で閉じるとカードにフォーカスが戻る", async ({
    page,
  }) => {
    await open(page, projectsSnapshot([APP, LOG]));
    await card(page, "ribbitlog").focus();
    await page.keyboard.press("Shift+F10");

    const menu = page.getByRole("menu", { name: "ribbitlog" });
    await expect(menu).toBeVisible();
    await expect(menu.getByRole("menuitem", { name: "新規ターミナル" })).toBeFocused();
    await page.keyboard.press("ArrowDown");
    await expect(menu.getByRole("menuitem", { name: "絶対パスをコピー" })).toBeFocused();
    await page.keyboard.press("ArrowDown");
    await expect(menu.getByRole("menuitem", { name: "色" })).toBeFocused();
    // 右の矢印で色の一覧を開き、いまの色(オレンジ)にフォーカスが移る。
    await page.keyboard.press("ArrowRight");
    const colors = page.getByRole("menu", { name: "色" });
    await expect(colors.getByRole("menuitemradio", { name: "オレンジ" })).toBeFocused();
    await page.keyboard.press("ArrowDown");
    await expect(colors.getByRole("menuitemradio", { name: "黄" })).toBeFocused();
    // Esc で色の一覧だけを閉じ、「色」に戻る。
    await page.keyboard.press("Escape");
    await expect(colors).toHaveCount(0);
    await expect(menu.getByRole("menuitem", { name: "色" })).toBeFocused();
    // 先頭(新規ターミナル)からさらに上で、末尾へ回る。
    await page.keyboard.press("ArrowUp");
    await page.keyboard.press("ArrowUp");
    await page.keyboard.press("ArrowUp");
    await expect(menu.getByRole("menuitem", { name: "一覧から削除" })).toBeFocused();
    await page.keyboard.press("ArrowDown");
    await page.keyboard.press("ArrowDown");
    await expect(menu.getByRole("menuitem", { name: "絶対パスをコピー" })).toBeFocused();
    await page.keyboard.press("Enter");
    await expect.poll(() => readCallArgs(page, "copy_project_path")).toEqual([{ projectId: LOG.id }]);
    await expect(menu).toHaveCount(0);
    await expect(card(page, "ribbitlog")).toBeFocused();

    // Esc で閉じても、カードにフォーカスが戻る。
    await page.keyboard.press("Shift+F10");
    await expect(page.getByRole("menu")).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(page.getByRole("menu")).toHaveCount(0);
    await expect(card(page, "ribbitlog")).toBeFocused();
  });

  test("色の一覧で_Enter_を押すと色の変更のコマンドが呼ばれる", async ({ page }) => {
    await open(page, projectsSnapshot([APP]));
    await card(page, "app").focus();
    await page.keyboard.press("Shift+F10");
    await page.keyboard.press("ArrowDown");
    await page.keyboard.press("ArrowDown");
    await page.keyboard.press("Enter");
    const colors = page.getByRole("menu", { name: "色" });
    await expect(colors.getByRole("menuitemradio", { name: "緑", exact: true })).toBeFocused();
    await page.keyboard.press("ArrowDown");
    await page.keyboard.press(" ");
    await expect.poll(() => readCallArgs(page, "set_project_color")).toEqual([{ projectId: APP.id, color: "teal" }]);
  });
});
