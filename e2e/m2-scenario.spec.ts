// M2 のシナリオの画面の自動テスト(T35a)。
//
// PRD §8「UI: 起動 → プロジェクト追加 → 切替」を 1 本の流れで通し、FR-042(AC-M04)・FR-004(AC-P04)・
// FR-009(AC-P09)の画面側を確かめる。Rust 側は偽物(fixtures/tauriMock.ts)で、コマンドの結果の写しを送り返さない。
// そのため各操作のあと「コマンドが正しい引数で呼ばれた」ことを確かめてから、Rust が返すはずの写しを
// app-snapshot のイベントで自分で流し、画面が変わることを確かめる。

import { expect, type Locator, type Page, test } from "@playwright/test";

import type { AppSnapshot } from "../src/ipc/generated/AppSnapshot";
import type { ProjectColor } from "../src/ipc/generated/ProjectColor";
import {
  emitEvent,
  installTauriMock,
  parseChannelId,
  readCallArgs,
  readRecordedCalls,
  sampleRunningTerminal,
  sampleSnapshot,
  sendChannelBytes,
  setMockReply,
  type TauriMockConfig,
} from "./fixtures/tauriMock";

/** 写しを組み立てるためのプロジェクト 1 件。タブはターミナルの ID と表示名を自分で決める(閉じた ID を使い回さないため)。 */
interface ProjectSpec {
  id: string;
  name: string;
  color: ProjectColor;
  tabs: readonly { id: string; label: string }[];
  /** 表示中のタブ。省略すると最初のタブ。 */
  active?: string | null;
}

/** プロジェクトの並び(写しの順)と選択中のプロジェクトから写しを作る。ターミナルはすべて動作中。 */
function snapshotOf(
  projects: readonly ProjectSpec[],
  selected: string | null,
  overrides: Partial<AppSnapshot> = {},
): AppSnapshot {
  return sampleSnapshot({
    projects: projects.map((p) => ({
      id: p.id,
      name: p.name,
      color: p.color,
      displayPath: `~/dev/${p.name}`,
      missing: false,
      showHidden: false,
      terminalIds: p.tabs.map((t) => t.id),
      activeTerminalId: p.active === undefined ? (p.tabs[0]?.id ?? null) : p.active,
      editorIds: [],
      shownEditorId: null,
    })),
    selectedProjectId: selected,
    terminals: projects.flatMap((p) =>
      p.tabs.map((t) => ({
        id: t.id,
        projectId: p.id,
        label: t.label,
        status: { kind: "running" as const },
        attention: false,
      })),
    ),
    ...overrides,
  });
}

/** 写しを渡して画面を開き、準備完了まで待つ。 */
async function open(page: Page, snapshot: AppSnapshot, overrides: Partial<TauriMockConfig> = {}): Promise<void> {
  await installTauriMock(page, { get_app_snapshot: { ok: snapshot }, ...overrides });
  await page.goto("/");
  await waitReady(page);
}

async function waitReady(page: Page): Promise<void> {
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

const cards = (page: Page) => page.getByTestId("project-card");
const card = (page: Page, name: string) => cards(page).filter({ hasText: name });
const tabList = (page: Page) => page.getByRole("tablist", { name: "ターミナルのタブ" });
const tabs = (page: Page) => tabList(page).getByRole("tab");
const tab = (page: Page, name: string) => tabList(page).getByRole("tab", { name, exact: true });
const treeHeader = (page: Page) => page.getByTestId("tree-header");
const terminalRows = (page: Page) => page.getByTestId("terminal-host").locator(".xterm-rows > div");

/** 届いたコマンドの名前(届いた順)。 */
async function commands(page: Page): Promise<string[]> {
  return (await readRecordedCalls(page)).map((call) => call.command);
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

async function connectCount(page: Page, terminalId: string): Promise<number> {
  return (await readCallArgs(page, "connect_terminal")).filter((c) => c.terminalId === terminalId).length;
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

test.describe("M2 のシナリオ(PRD §8 UI)", () => {
  test("起動_追加_切替_タブ操作_削除の確認_終了の確認を通す", async ({ page }) => {
    // Rust の登録は、登録したプロジェクトを選択中にし、タブを 1 枚開く(ProjectStore::register)。
    const A: ProjectSpec = { id: "p-a", name: "alpha", color: "green", tabs: [{ id: "a-1", label: "ターミナル 1" }] };
    const B: ProjectSpec = { id: "p-b", name: "bravo", color: "orange", tabs: [{ id: "b-1", label: "ターミナル 1" }] };

    // --- 起動: プロジェクト 0 件 ---
    await open(page, snapshotOf([], null), { add_project_by_dialog: { ok: { kind: "added", projectId: A.id } } });
    await expect(cards(page)).toHaveCount(0);
    await expect(page.getByTestId("projects-empty")).toContainText("プロジェクトがありません");
    await expect(page.getByTestId("terminals-empty")).toContainText("プロジェクトがありません");

    // --- 1 件目の追加 ---
    await page.getByRole("button", { name: "フォルダを追加" }).click();
    await expect.poll(() => readCallArgs(page, "add_project_by_dialog")).toHaveLength(1);
    await emitEvent(page, "app-snapshot", snapshotOf([A], A.id));
    await expect(cards(page)).toHaveCount(1);
    await expect(card(page, "alpha")).toHaveAttribute("aria-current", "true");
    await expect(treeHeader(page)).toHaveText("alpha");
    await expect(tabs(page)).toHaveText(["ターミナル 1"]);
    await connectOf(page, "a-1");

    // --- 2 件目の追加(登録した B が選択中になる) ---
    await setMockReply(page, "add_project_by_dialog", { ok: { kind: "added", projectId: B.id } });
    await page.getByRole("button", { name: "フォルダを追加" }).click();
    await expect.poll(() => readCallArgs(page, "add_project_by_dialog")).toHaveLength(2);
    await emitEvent(page, "app-snapshot", snapshotOf([A, B], B.id));
    await expect(cards(page)).toHaveText([/alpha/, /bravo/]);
    await expect(card(page, "bravo")).toHaveAttribute("aria-current", "true");
    await expect(treeHeader(page)).toHaveText("bravo");
    await expect(tab(page, "ターミナル 1")).toHaveAttribute("title", "bravo: ターミナル 1");
    await connectOf(page, "b-1");

    // --- カードのクリックで A へ切替(タブ群とツリーのヘッダが替わる) ---
    await card(page, "alpha").click();
    await expect.poll(() => readCallArgs(page, "select_project")).toEqual([{ projectId: A.id }]);
    await emitEvent(page, "app-snapshot", snapshotOf([A, B], A.id));
    await expect(card(page, "alpha")).toHaveAttribute("aria-current", "true");
    await expect(card(page, "bravo")).not.toHaveAttribute("aria-current", "true");
    await expect(treeHeader(page)).toHaveText("alpha");
    await expect(tab(page, "ターミナル 1")).toHaveAttribute("title", "alpha: ターミナル 1");

    // --- タブの追加 ---
    await page.getByRole("button", { name: "新規ターミナル", exact: true }).click();
    await expect.poll(() => readCallArgs(page, "new_terminal")).toHaveLength(1);
    const a2 = { id: "a-2", label: "ターミナル 2" };
    let a: ProjectSpec = { ...A, tabs: [...A.tabs, a2], active: a2.id };
    await emitEvent(page, "app-snapshot", snapshotOf([a, B], A.id));
    await expect(tabs(page)).toHaveText(["ターミナル 1", "ターミナル 2"]);
    await expect(tab(page, "ターミナル 2")).toHaveAttribute("aria-selected", "true");
    await connectOf(page, "a-2");

    // --- タブの切替 ---
    await tab(page, "ターミナル 1").click();
    await expect.poll(() => readCallArgs(page, "select_terminal")).toEqual([{ terminalId: "a-1" }]);
    a = { ...a, active: "a-1" };
    await emitEvent(page, "app-snapshot", snapshotOf([a, B], A.id));
    await expect(tab(page, "ターミナル 1")).toHaveAttribute("aria-selected", "true");
    await expect(tab(page, "ターミナル 2")).toHaveAttribute("aria-selected", "false");
    await expect(page.getByRole("tabpanel", { name: "ターミナル 1" })).toBeVisible();

    // --- タブを閉じる(0 枚まで) ---
    await page.getByRole("button", { name: "「ターミナル 2」を閉じる" }).click();
    await expect.poll(() => readCallArgs(page, "close_terminal")).toEqual([{ terminalId: "a-2" }]);
    a = { ...a, tabs: A.tabs, active: "a-1" };
    await emitEvent(page, "app-snapshot", snapshotOf([a, B], A.id));
    await expect(tabs(page)).toHaveText(["ターミナル 1"]);

    await page.getByRole("button", { name: "「ターミナル 1」を閉じる" }).click();
    await expect.poll(() => readCallArgs(page, "close_terminal")).toEqual([{ terminalId: "a-2" }, { terminalId: "a-1" }]);
    a = { ...a, tabs: [], active: null };
    await emitEvent(page, "app-snapshot", snapshotOf([a, B], A.id));
    const empty = page.getByTestId("terminals-empty");
    await expect(empty).toContainText("ターミナルがありません");
    await expect(tabs(page)).toHaveCount(0);
    await expect(page.getByTestId("terminal-host")).toHaveCount(0);
    // 閉じただけで勝手に開かない。
    expect(await readCallArgs(page, "new_terminal")).toHaveLength(1);

    // --- 0 枚の表示から新規ターミナル ---
    await empty.getByRole("button", { name: /新規ターミナル/ }).click();
    await expect.poll(() => readCallArgs(page, "new_terminal")).toHaveLength(2);
    a = { ...a, tabs: [{ id: "a-3", label: "ターミナル 1" }], active: "a-3" };
    await emitEvent(page, "app-snapshot", snapshotOf([a, B], A.id));
    await expect(tabs(page)).toHaveText(["ターミナル 1"]);
    await expect(tab(page, "ターミナル 1")).toHaveAttribute("aria-selected", "true");
    await expect(page.getByTestId("terminal-host")).toHaveCount(1);
    await connectOf(page, "a-3");

    // --- 一覧から削除: 動作中があるので確認が出る → 中止 ---
    const runningB = [sampleRunningTerminal(B.id, B.name, "b-1", "ターミナル 1")];
    await setMockReply(page, "remove_project", { ok: { kind: "needsConfirmation", running: runningB, unsaved: [] } });
    await card(page, "bravo").click({ button: "right" });
    await page.getByRole("menu", { name: "bravo" }).getByRole("menuitem", { name: "一覧から削除" }).click();
    const removeDialog = page.getByRole("alertdialog", { name: "「bravo」を一覧から削除しますか?" });
    await expect(removeDialog).toBeVisible();
    await expect(removeDialog.getByRole("listitem")).toHaveText(["ターミナル 1"]);
    await expect.poll(() => readCallArgs(page, "remove_project")).toEqual([{ projectId: B.id, confirmed: false }]);

    const beforeCancel = await commands(page);
    await removeDialog.getByRole("button", { name: "中止" }).click();
    await expect(removeDialog).toHaveCount(0);
    // 中止では何も呼ばれない(削除のコマンドも、ほかのコマンドも)。
    expect(await commands(page)).toEqual(beforeCancel);
    await expect(cards(page)).toHaveCount(2);

    // --- もう一度削除を選び、確定する ---
    await card(page, "bravo").click({ button: "right" });
    await page.getByRole("menu", { name: "bravo" }).getByRole("menuitem", { name: "一覧から削除" }).click();
    await expect(removeDialog).toBeVisible();
    // 確認済みの削除には Rust は「削除した」と返す。
    await setMockReply(page, "remove_project", { ok: { kind: "removed" } });
    await removeDialog.getByRole("button", { name: "削除" }).click();
    await expect
      .poll(() => readCallArgs(page, "remove_project"))
      .toEqual([
        { projectId: B.id, confirmed: false },
        { projectId: B.id, confirmed: false },
        { projectId: B.id, confirmed: true },
      ]);
    await expect(removeDialog).toHaveCount(0);
    await emitEvent(page, "app-snapshot", snapshotOf([a], A.id));
    await expect(cards(page)).toHaveText([/alpha/]);
    await expect(treeHeader(page)).toHaveText("alpha");

    // --- 終了の確認: 取り消し → 何も呼ばれない ---
    const runningA = [sampleRunningTerminal(A.id, A.name, "a-3", "ターミナル 1")];
    await emitEvent(page, "quit-requested", { running: runningA, unsaved: [] });
    const quitDialog = page.getByRole("alertdialog", { name: "Arin を終了しますか?" });
    await expect(quitDialog).toBeVisible();
    await expect(quitDialog.getByRole("listitem")).toHaveText(["alpha: ターミナル 1"]);
    const beforeQuitCancel = await commands(page);
    await quitDialog.getByRole("button", { name: "取り消し" }).click();
    await expect(quitDialog).toHaveCount(0);
    expect(await commands(page)).toEqual(beforeQuitCancel);
    expect(await readCallArgs(page, "confirm_quit")).toEqual([]);

    // --- 終了の確認: 終了 → confirm_quit ---
    await emitEvent(page, "quit-requested", { running: runningA, unsaved: [] });
    await expect(quitDialog).toBeVisible();
    await quitDialog.getByRole("button", { name: "終了" }).click();
    await expect.poll(() => readCallArgs(page, "confirm_quit")).toHaveLength(1);
    await expect(quitDialog).toHaveCount(0);
  });
});

test.describe("切り替えても出力が途切れない(FR-042・AC-M04 の画面側)", () => {
  test("A_で_1_秒ごとに出力を流し_B_に切り替えて_10_秒後に_A_へ戻ると途切れず進んでいる", async ({ page }) => {
    // 【仮定】実時間を縮める: 1 秒ごと → 50ms ごと、隠す 10 秒 → 20 行(およそ 1 秒)。
    const INTERVAL_MS = 50;
    const BEFORE = 5;
    const HIDDEN = 20;
    const AFTER = 5;
    const TOTAL = BEFORE + HIDDEN + AFTER;

    const A: ProjectSpec = { id: "p-a", name: "alpha", color: "green", tabs: [{ id: "a-1", label: "ターミナル 1" }] };
    const B: ProjectSpec = { id: "p-b", name: "bravo", color: "orange", tabs: [{ id: "b-1", label: "ターミナル 1" }] };
    await open(page, snapshotOf([A, B], A.id));
    const connectA = await connectOf(page, "a-1");
    const channelA = parseChannelId(connectA.onOutput);
    const channelB = parseChannelId((await connectOf(page, "b-1")).onOutput);
    // 番号の行がすべて画面に収まる(スクロールで押し出されない)ことを前提にする。
    expect(connectA.rows, "ターミナルの行数が流す行数より多い").toBeGreaterThan(TOTAL);

    const line = (n: number) => `A ${String(n).padStart(3, "0")}`;
    let sent = 0;
    let sentBytes = 0;
    // A のチャネルに、番号つきの行を一定の間隔で流し続ける(画面の切り替えとは独立に進む)。
    const streaming = (async () => {
      for (let n = 1; n <= TOTAL; n++) {
        sentBytes += await sendChannelBytes(page, channelA, [`${line(n)}\r\n`]);
        sent = n;
        if (n < TOTAL) await new Promise((resolve) => setTimeout(resolve, INTERVAL_MS));
      }
    })();

    // 切替前の行が A の画面に出ている。
    await expect.poll(() => sent).toBeGreaterThanOrEqual(BEFORE);
    await expect(terminalRows(page).nth(0)).toHaveText(line(1));

    // B に切り替える(A は隠れる)。B には B の出力が出る。
    await emitEvent(page, "app-snapshot", snapshotOf([A, B], B.id));
    await expect(treeHeader(page)).toHaveText("bravo");
    await sendChannelBytes(page, channelB, ["B shown\r\n"]);
    await expect(terminalRows(page).nth(0)).toHaveText("B shown");
    const hiddenFrom = sent;

    // 隠れている間も A の出力は取り込まれ続ける(処理済みの量が増える)。
    const ackedAtHide = await ackedBytes(page, "a-1");
    await expect.poll(() => sent, { timeout: 15_000 }).toBeGreaterThanOrEqual(BEFORE + HIDDEN);
    await expect.poll(() => ackedBytes(page, "a-1")).toBeGreaterThan(ackedAtHide);
    const hiddenTo = sent;
    expect(hiddenTo - hiddenFrom, "隠れている間に流した行がある").toBeGreaterThan(0);

    // A に戻る。
    await emitEvent(page, "app-snapshot", snapshotOf([A, B], A.id));
    await expect(treeHeader(page)).toHaveText("alpha");
    await streaming;

    // 切替前・隠れていた間・戻ったあとの番号が、欠けず順に並ぶ。
    const expected = Array.from({ length: TOTAL }, (_, i) => line(i + 1));
    await expect
      .poll(async () => (await terminalRows(page).allTextContents()).slice(0, TOTAL).map((t) => t.replace(/\u00a0/g, " ").trimEnd()))
      .toEqual(expected);
    await expect(terminalRows(page).nth(TOTAL)).toHaveText("");

    // A の接続は作り直されていない。取り込み済みの量は流したバイト数と同じ。
    expect(await connectCount(page, "a-1")).toBe(1);
    await expect.poll(() => ackedBytes(page, "a-1")).toBe(sentBytes);
  });
});

test.describe("写しのとおりに描く(FR-004・AC-P04、FR-009・AC-P09 の画面側)", () => {
  test("届いた写しのとおり同じ順_同じ選択_同じペイン配分で描かれる", async ({ page }) => {
    const A: ProjectSpec = { id: "p-a", name: "alpha", color: "green", tabs: [{ id: "a-1", label: "ターミナル 1" }] };
    const B: ProjectSpec = {
      id: "p-b",
      name: "bravo",
      color: "orange",
      tabs: [
        { id: "b-1", label: "ターミナル 1" },
        { id: "b-2", label: "ターミナル 2" },
      ],
      active: "b-2",
    };
    const C: ProjectSpec = { id: "p-c", name: "charlie", color: "purple", tabs: [{ id: "c-1", label: "ターミナル 1" }] };
    // 既定(幅 260・高さ 200)と違うペイン配分。
    const layout = { sidebarWidth: 340, sidebarCollapsed: false, cardAreaHeight: 150 };
    await open(page, snapshotOf([A, B, C], B.id, { layout }));
    const viewport = page.viewportSize();
    if (viewport === null) throw new Error("ビューポートの大きさが取れない");

    const verify = async (when: string) => {
      // 同じ順。
      await expect(cards(page), when).toHaveText([/alpha/, /bravo/, /charlie/]);
      // 同じ選択(B だけが強調され、ツリーのヘッダとタブ群が B のもの)。
      await expect(card(page, "bravo"), when).toHaveAttribute("aria-current", "true");
      for (const name of ["alpha", "charlie"]) {
        await expect(card(page, name), when).not.toHaveAttribute("aria-current", "true");
      }
      await expect(treeHeader(page), when).toHaveText("bravo");
      await expect(tabs(page), when).toHaveText(["ターミナル 1", "ターミナル 2"]);
      await expect(tab(page, "ターミナル 2"), when).toHaveAttribute("aria-selected", "true");
      await expect(tab(page, "ターミナル 2"), when).toHaveAttribute("title", "bravo: ターミナル 2");

      // 同じペイン配分(左右の幅、上段・下段の配分)。
      const sidebar = await box(page.getByTestId("pane-sidebar"));
      const terminal = await box(page.getByTestId("pane-terminal"));
      const cardArea = await box(page.getByTestId("card-area"));
      const tree = await box(page.getByTestId("tree-area"));
      expect(Math.round(sidebar.width), `${when}: 左ペインの幅`).toBe(layout.sidebarWidth);
      expect(Math.round(terminal.x), `${when}: 右ペインの左端`).toBe(layout.sidebarWidth);
      expect(Math.round(terminal.width), `${when}: 右ペインの幅`).toBe(viewport.width - layout.sidebarWidth);
      expect(Math.round(cardArea.height), `${when}: カード領域の高さ`).toBe(layout.cardAreaHeight);
      expect(Math.round(tree.y), `${when}: ツリー領域の上端`).toBe(Math.round(cardArea.y + layout.cardAreaHeight));
      expect(Math.round(tree.height), `${when}: ツリー領域の高さ`).toBe(
        Math.round(sidebar.height) - layout.cardAreaHeight,
      );
      // 境界の値も写しのとおり。
      await expect(page.getByRole("separator", { name: "左右のペインの境界" }), when).toHaveAttribute(
        "aria-valuenow",
        String(layout.sidebarWidth),
      );
      await expect(page.getByRole("separator", { name: "カード一覧とツリーの境界" }), when).toHaveAttribute(
        "aria-valuenow",
        String(layout.cardAreaHeight),
      );
      // 描いただけでは選択も寸法も送らない。
      expect(await readCallArgs(page, "set_pane_sizes"), when).toEqual([]);
      expect(await readCallArgs(page, "select_project"), when).toEqual([]);
      expect(await readCallArgs(page, "select_terminal"), when).toEqual([]);
    };

    await verify("起動");

    // 再起動の代わりに読み込み直す(偽の Rust 側は同じ写しを返す)。もう一度同じに描かれる。
    await page.reload();
    await waitReady(page);
    await verify("読み込み直し");
  });
});
