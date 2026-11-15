// M4 のシナリオの画面の自動テスト(T49a)。
//
// PRD §8「UI」のタイル表示を、機能をまたいだ流れで通す(タブ表示 → タイル表示 → ページ送り → 別のプロジェクトのタイルの
// クリック(カード・ツリー・入力の宛先の連動)→ ツリーからタイルへのドラッグ → 拡大と復帰 → タブ表示に戻す)。
// AC-M14(10 枚で 1/2)、AC-M15(タイルのクリックでカードとツリーが替わる)、AC-C02(カードとタイルの色の印)、
// FR-055(アテンションの印)、FR-052(隠れたページの出力の継続)、FR-048(表示モードの復元の画面側)を確かめる。
// 部品ごとの細かい振る舞い(寸法・ツールチップ・端のボタンなど)は tiles.spec.ts が見るので、ここでは繰り返さない。
//
// 偽の Rust 側(fixtures/tauriMock.ts)は写しを送り返さない。各操作のあと「コマンドが正しい引数で呼ばれた」ことを
// 確かめてから、Rust が返すはずの写しを withTileDisplay(Rust の規則の写し)で作って app-snapshot で流す。
//
// 要素は役割とアクセシブルな名前(getByRole)と data-testid(と data-* の属性)で探す。見た目(クラス名・寸法・位置)は
// 見ない。例外は AC-C02 の色の印の色(受け入れ基準そのもの)だけで、印を探すのに tiles.spec.ts と同じ .project-dot を使う。
//
// 時間の記録(NFR-009 の画面側)は T49b(e2e/perf-tiles.spec.ts、npm run perf)の受け持ちなので、ここでは測らない。

import { expect, type Locator, type Page, test } from "@playwright/test";

import type { AppSnapshot } from "../src/ipc/generated/AppSnapshot";
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
  type TileDisplayOptions,
  withTileDisplay,
} from "./fixtures/tauriMock";

// --- プロジェクト(色違い。合計 10 枚: 1 ページ目に 9 枚、2 ページ目に 1 枚) ---

const APP: SampleProject = {
  id: "p-app",
  name: "app",
  color: "green",
  tabs: ["ターミナル 1", "ターミナル 2", "ターミナル 3"],
};
const LOG: SampleProject = { id: "p-log", name: "ribbitlog", color: "orange", tabs: ["ターミナル 1", "サーバー"] };
const NOTES: SampleProject = { id: "p-notes", name: "notes", color: "purple", tabs: ["ターミナル 1", "ターミナル 2"] };
const WEB: SampleProject = { id: "p-web", name: "web", color: "teal", tabs: ["ターミナル 1", "ターミナル 2", "ビルド"] };
const ALL = [APP, LOG, NOTES, WEB] as const;

/** 写しの順(プロジェクト順 → タブ順)のターミナルの ID。 */
const ALL_IDS = [
  "p-app-t1",
  "p-app-t2",
  "p-app-t3",
  "p-log-t1",
  "p-log-t2",
  "p-notes-t1",
  "p-notes-t2",
  "p-web-t1",
  "p-web-t2",
  "p-web-t3",
];
const PAGE_ONE = ALL_IDS.slice(0, 9);

/** タブ表示の写し(Rust の起動時と同じ。最初のプロジェクトが選択中で、各プロジェクトの最初のタブが表示中)。 */
const BASE: AppSnapshot = projectsSnapshot(ALL);

/** Rust が返すはずの写し(BASE から Rust の規則で表示の状態を作る)。 */
const rust = (options: TileDisplayOptions = {}): AppSnapshot => withTileDisplay(BASE, options);

/** ツリーのルートの中身(プロジェクトごと)。 */
function scenarioListings(): Record<string, MockListing> {
  return {
    [listingKey(APP.id, "")]: listed([
      ["src", "directory"],
      ["README.md", "file"],
    ]),
    [listingKey(LOG.id, "")]: listed([
      ["crates", "directory"],
      ["Cargo.toml", "file"],
    ]),
    [listingKey(NOTES.id, "")]: listed([["todo.md", "file"]]),
    [listingKey(WEB.id, "")]: listed([["index.html", "file"]]),
  };
}

// --- 開く・待つ ---

/** 画面のコンソールのエラーと、捕まえられなかった例外を集める。 */
function collectPageErrors(page: Page): string[] {
  const errors: string[] = [];
  page.on("console", (message) => {
    if (message.type() === "error") errors.push(message.text());
  });
  page.on("pageerror", (error) => errors.push(error.message));
  return errors;
}

async function open(page: Page, snapshot: AppSnapshot): Promise<void> {
  await installTauriMock(page, { get_app_snapshot: { ok: snapshot } }, scenarioListings());
  await page.goto("/");
  await waitReady(page);
}

async function waitReady(page: Page): Promise<void> {
  await expect
    .poll(async () => (await readRecordedCalls(page)).map((call) => call.command))
    .toContain("notify_app_ready");
}

/** 描き直しが済むのを待つ(2 フレーム)。「呼ばれない」ことを確かめる前に使う。 */
async function settle(page: Page): Promise<void> {
  await page.evaluate(
    () => new Promise<void>((resolve) => requestAnimationFrame(() => requestAnimationFrame(() => resolve()))),
  );
}

// --- 要素(役割と名前、data-testid) ---

const modeSwitch = (page: Page) => page.getByTestId("display-mode-switch");
const tabsButton = (page: Page) => modeSwitch(page).getByRole("button", { name: "タブ表示" });
const tilesButton = (page: Page) => modeSwitch(page).getByRole("button", { name: "タイル表示" });
const grid = (page: Page) => page.getByTestId("tile-grid");
const tiles = (page: Page) => page.getByTestId("tile");
/** タイル(役割は group、名前は「プロジェクト名: タブ名」)。 */
const tile = (page: Page, title: string) => page.getByRole("group", { name: title, exact: true });
const titleBar = (page: Page, title: string) => tile(page, title).getByTestId("tile-title");
const pageLabel = (page: Page) => page.getByTestId("tile-page-label");
const paging = (page: Page) => page.getByTestId("tile-paging");
const previousButton = (page: Page) => paging(page).getByRole("button", { name: /^前のページ/ });
const nextButton = (page: Page) => paging(page).getByRole("button", { name: /^次のページ/ });
const pageMark = (button: Locator) => button.getByTestId("page-attention");
const attentionMark = (locator: Locator) => locator.getByRole("img", { name: "新しい通知があります" });
const maximizedStrip = (page: Page) => page.getByTestId("maximized-strip");
const restoreButton = (page: Page) => maximizedStrip(page).getByRole("button", { name: "タイル表示に戻す" });
const tabBar = (page: Page) => page.getByTestId("tab-bar");
const tabList = (page: Page) => page.getByRole("tablist", { name: "ターミナルのタブ" });
const terminalHosts = (page: Page) => page.getByTestId("terminal-host");
const hostOf = (page: Page, terminalId: string) =>
  page.locator(`[data-testid="terminal-host"][data-terminal-id=${JSON.stringify(terminalId)}]`);
const card = (page: Page, project: SampleProject) =>
  page.locator(`[data-testid="project-card"][data-project-id=${JSON.stringify(project.id)}]`);
const treeHeader = (page: Page) => page.getByTestId("tree-header");
const fileTree = (page: Page, project: SampleProject) => page.getByRole("tree", { name: `${project.name} のファイル` });
const treeRow = (page: Page, path: string) =>
  page.locator(`[data-testid="tree-row"][data-path=${JSON.stringify(path)}]`);
const ghost = (page: Page) => page.getByTestId("tree-drag-ghost");

/** プロジェクト色の印の色(AC-C02 の確かめだけに使う)。 */
const dotColor = (locator: Locator) =>
  locator.locator(".project-dot").first().evaluate((el) => getComputedStyle(el).backgroundColor);

// --- 記録の読み取り ---

async function commands(page: Page): Promise<string[]> {
  return (await readRecordedCalls(page)).map((call) => call.command);
}

async function countOf(page: Page, command: string): Promise<number> {
  return (await commands(page)).filter((c) => c === command).length;
}

/** 表示中のページのタイルのターミナルの ID(描いた順)。 */
async function tileIds(page: Page): Promise<string[]> {
  return tiles(page).evaluateAll((els) => els.map((el) => el.getAttribute("data-tile-id") ?? ""));
}

/** 入力のフォーカスがあるターミナル(フォーカスのある要素を囲む置き場所の ID)。無ければ null。 */
async function inputFocusedTerminal(page: Page): Promise<string | null> {
  return page.evaluate(
    () =>
      document.activeElement?.closest('[data-testid="terminal-host"]')?.getAttribute("data-terminal-id") ?? null,
  );
}

/** 指定のターミナルへ書いた文字列(write_terminal の data を届いた順につないだもの)。 */
async function writtenTo(page: Page, terminalId: string): Promise<string> {
  return (await readCallArgs(page, "write_terminal"))
    .filter((w) => w.terminalId === terminalId)
    .map((w) => w.data)
    .join("");
}

/** 指定のターミナルの接続。届くまで待つ。 */
async function connectOf(page: Page, terminalId: string) {
  await expect
    .poll(async () => (await readCallArgs(page, "connect_terminal")).some((c) => c.terminalId === terminalId))
    .toBe(true);
  const found = (await readCallArgs(page, "connect_terminal")).find((c) => c.terminalId === terminalId);
  if (found === undefined) throw new Error(`${terminalId} の接続が無い`);
  return found;
}

async function channelOf(page: Page, terminalId: string): Promise<number> {
  return parseChannelId((await connectOf(page, terminalId)).onOutput);
}

/** ターミナルごとの接続の回数(全ターミナル)。 */
async function connectCounts(page: Page): Promise<Record<string, number>> {
  const counts: Record<string, number> = {};
  for (const c of await readCallArgs(page, "connect_terminal")) counts[c.terminalId] = (counts[c.terminalId] ?? 0) + 1;
  return counts;
}

const ONCE_EACH = Object.fromEntries(ALL_IDS.map((id) => [id, 1]));

async function ackedBytes(page: Page, terminalId: string): Promise<number> {
  return (await readCallArgs(page, "ack_terminal_output"))
    .filter((a) => a.terminalId === terminalId)
    .reduce((sum, a) => sum + a.bytes, 0);
}

// --- 操作 ---

async function centerOf(locator: Locator): Promise<{ x: number; y: number }> {
  const b = await locator.boundingBox();
  if (b === null) throw new Error("要素の大きさが取れない(表示されていない)");
  return { x: b.x + b.width / 2, y: b.y + b.height / 2 };
}

/** ツリーの行を押してしきい値を超えて動かし、target の上へ運んで落とす(落とし先の強調を待ってから離す)。 */
async function dragTreeRowTo(page: Page, source: Locator, target: Locator): Promise<void> {
  const from = await centerOf(source);
  await page.mouse.move(from.x, from.y);
  await page.mouse.down();
  await page.mouse.move(from.x + 6, from.y + 6, { steps: 3 });
  await expect(ghost(page)).toBeVisible();
  const to = await centerOf(target);
  await page.mouse.move(to.x, to.y, { steps: 4 });
  await expect(target).toHaveAttribute("data-drop-active", "true");
  await page.mouse.up();
}

// --- 各テストの終わりの確かめ ---

/** 値の中の文字列を、場所の名前つきで集める。 */
function collectStrings(value: unknown, where: string, out: Array<{ where: string; value: string }>): void {
  if (typeof value === "string") {
    out.push({ where, value });
  } else if (Array.isArray(value)) {
    value.forEach((item, index) => collectStrings(item, `${where}[${index}]`, out));
  } else if (value !== null && typeof value === "object") {
    for (const [key, item] of Object.entries(value)) collectStrings(item, `${where}.${key}`, out);
  }
}

// 画面の失敗(通信の失敗の報告)が無かったことと、画面から Rust へ送った引数に絶対パスとドットドットが
// 1 つも無いこと(設計書 §12。絶対パスは Rust だけが持つ)を見る。
test.afterEach(async ({ page }) => {
  const reported = await readCallArgs(page, "report_frontend_error");
  expect(reported.map((r) => r.message), "画面の失敗が Rust へ報告された").toEqual([]);

  const strings: Array<{ where: string; value: string }> = [];
  for (const [index, call] of (await readRecordedCalls(page)).entries()) {
    collectStrings(call.args, `#${index} ${call.command}`, strings);
  }
  const unsafe = strings.filter(({ value }) => value.startsWith("/") || value.split("/").includes(".."));
  expect(unsafe, "画面が絶対パスかドットドットを含む引数を送った").toEqual([]);
});

test.describe("M4 のシナリオ(PRD §8 UI のタイル表示)", () => {
  test("タブ表示からタイル表示に切り替え_ページを送り_別のプロジェクトのタイルをクリックし_拡大して戻し_タブ表示に戻すを通す", async ({
    page,
  }) => {
    test.setTimeout(90_000);
    const errors = collectPageErrors(page);

    // --- 起動(タブ表示): 選択中の app のタブバーと、表示中のタブ 1 枚。ほかのターミナルも隠したまま接続する ---
    await open(page, BASE);
    await expect(tabBar(page)).toBeVisible();
    await expect(tabList(page).getByRole("tab")).toHaveText(["ターミナル 1", "ターミナル 2", "ターミナル 3"]);
    await expect(terminalHosts(page)).toHaveCount(1);
    await expect(terminalHosts(page)).toHaveAttribute("data-terminal-id", "p-app-t1");
    await expect(tabsButton(page)).toHaveAttribute("aria-pressed", "true");
    await expect(fileTree(page, APP)).toBeVisible();
    await expect.poll(() => connectCounts(page)).toEqual(ONCE_EACH);
    await expect.poll(() => inputFocusedTerminal(page)).toBe("p-app-t1");

    // --- 表示モードの切替(FR-048): ボタン → set_display_mode。写しが届くまでは切り替えない ---
    await tilesButton(page).click();
    await expect.poll(() => readCallArgs(page, "set_display_mode")).toEqual([{ mode: "tiles" }]);
    await settle(page);
    await expect(grid(page)).toHaveCount(0);

    // Rust の写し: タイル表示、10 枚で 1 ページ目に 9 枚(3×3)、表示は 1/2(AC-M14)。
    await emitEvent(page, "app-snapshot", rust());
    await expect(grid(page)).toBeVisible();
    await expect(tabBar(page)).toHaveCount(0);
    await expect(tilesButton(page)).toHaveAttribute("aria-pressed", "true");
    await expect.poll(() => tileIds(page)).toEqual(PAGE_ONE);
    await expect(grid(page)).toHaveAttribute("data-cols", "3");
    await expect(grid(page)).toHaveAttribute("data-rows", "3");
    await expect(pageLabel(page)).toHaveText("1/2");
    // タイトルは「プロジェクト名: タブ名」(FR-061)。フォーカス中は選択中のプロジェクトの表示中のタブ。
    await expect(page.getByTestId("tile-title")).toHaveText([
      "app: ターミナル 1",
      "app: ターミナル 2",
      "app: ターミナル 3",
      "ribbitlog: ターミナル 1",
      "ribbitlog: サーバー",
      "notes: ターミナル 1",
      "notes: ターミナル 2",
      "web: ターミナル 1",
      "web: ターミナル 2",
    ]);
    await expect(tile(page, "app: ターミナル 1")).toHaveAttribute("aria-current", "true");
    await expect.poll(() => inputFocusedTerminal(page)).toBe("p-app-t1");

    // --- 次のページ(FR-052): 2 ページ目は 1 枚(1×1)。フォーカスは先頭のタイルに移り、カードとツリーも付いていく ---
    await nextButton(page).click();
    await expect.poll(() => countOf(page, "next_tile_page")).toBe(1);
    await emitEvent(page, "app-snapshot", rust({ page: 1 }));
    await expect(pageLabel(page)).toHaveText("2/2");
    await expect.poll(() => tileIds(page)).toEqual(["p-web-t3"]);
    await expect(grid(page)).toHaveAttribute("data-cols", "1");
    await expect(grid(page)).toHaveAttribute("data-rows", "1");
    await expect(tile(page, "web: ビルド")).toHaveAttribute("aria-current", "true");
    await expect(card(page, WEB)).toHaveAttribute("aria-current", "true");
    await expect(fileTree(page, WEB)).toBeVisible();
    await expect.poll(() => inputFocusedTerminal(page)).toBe("p-web-t3");

    // --- 前のページ: 1 ページ目に戻り、先頭の app のタイルにフォーカス ---
    await previousButton(page).click();
    await expect.poll(() => countOf(page, "previous_tile_page")).toBe(1);
    await emitEvent(page, "app-snapshot", rust({ page: 0 }));
    await expect(pageLabel(page)).toHaveText("1/2");
    await expect.poll(() => tileIds(page)).toEqual(PAGE_ONE);
    await expect(card(page, APP)).toHaveAttribute("aria-current", "true");
    await expect.poll(() => inputFocusedTerminal(page)).toBe("p-app-t1");

    // --- 別のプロジェクトのタイルのクリック(FR-053、AC-M15): focus_terminal → 写しでカード・ツリー・タイルがそろう ---
    await titleBar(page, "ribbitlog: サーバー").click();
    await expect.poll(() => readCallArgs(page, "focus_terminal")).toEqual([{ terminalId: "p-log-t2" }]);
    await emitEvent(page, "app-snapshot", rust({ focusedTerminalId: "p-log-t2" }));
    await expect(card(page, LOG)).toHaveAttribute("aria-current", "true");
    await expect(card(page, APP)).not.toHaveAttribute("aria-current", "true");
    await expect(treeHeader(page)).toHaveText("ribbitlog");
    await expect(fileTree(page, LOG)).toBeVisible();
    await expect(treeRow(page, "Cargo.toml")).toBeVisible();
    await expect(tile(page, "ribbitlog: サーバー")).toHaveAttribute("aria-current", "true");
    await expect(tile(page, "app: ターミナル 1")).not.toHaveAttribute("aria-current", "true");
    // 同じページの中なのでページは変わらない。入力のフォーカスが移り、キー入力はそのターミナルへ届く。
    await expect(pageLabel(page)).toHaveText("1/2");
    await expect.poll(() => inputFocusedTerminal(page)).toBe("p-log-t2");
    await page.keyboard.type("ls");
    await expect.poll(() => writtenTo(page, "p-log-t2")).toBe("ls");

    // --- ツリーからタイルへドラッグ(M3 の申し送り): 落とした先へ挿入し、そのタイルへフォーカス。書き込みは Rust の役目 ---
    const writesBeforeDrop = await countOf(page, "write_terminal");
    await dragTreeRowTo(page, treeRow(page, "Cargo.toml"), hostOf(page, "p-notes-t1"));
    await expect
      .poll(() => readCallArgs(page, "insert_entry_path"))
      .toEqual([{ projectId: LOG.id, paths: ["Cargo.toml"], terminalId: "p-notes-t1" }]);
    await expect
      .poll(() => readCallArgs(page, "focus_terminal"))
      .toEqual([{ terminalId: "p-log-t2" }, { terminalId: "p-notes-t1" }]);
    await settle(page);
    expect(await countOf(page, "write_terminal"), "挿入で画面が write_terminal を呼んだ").toBe(writesBeforeDrop);
    expect(await countOf(page, "write_terminal_bytes"), "挿入で画面が write_terminal_bytes を呼んだ").toBe(0);
    await emitEvent(page, "app-snapshot", rust({ focusedTerminalId: "p-notes-t1" }));
    await expect(card(page, NOTES)).toHaveAttribute("aria-current", "true");
    await expect(fileTree(page, NOTES)).toBeVisible();
    await expect(tile(page, "notes: ターミナル 1")).toHaveAttribute("aria-current", "true");
    await expect.poll(() => inputFocusedTerminal(page)).toBe("p-notes-t1");

    // --- 拡大(FR-054): タイトルバーのダブルクリック → そのタイルへフォーカスし、拡大を 1 回求める ---
    const focusBefore = (await readCallArgs(page, "focus_terminal")).length;
    await titleBar(page, "app: ターミナル 2").dblclick();
    await expect
      .poll(async () => [...new Set((await readCallArgs(page, "focus_terminal")).slice(focusBefore).map((c) => c.terminalId))])
      .toEqual(["p-app-t2"]);
    await expect.poll(() => countOf(page, "toggle_tile_maximize")).toBe(1);
    await emitEvent(page, "app-snapshot", rust({ focusedTerminalId: "p-app-t2", maximized: true }));
    await expect(grid(page)).toHaveCount(0);
    await expect(paging(page)).toHaveCount(0);
    await expect(terminalHosts(page)).toHaveCount(1);
    await expect(terminalHosts(page)).toHaveAttribute("data-terminal-id", "p-app-t2");
    await expect(maximizedStrip(page).getByTestId("maximized-title")).toHaveText("app: ターミナル 2");
    await expect(tilesButton(page)).toHaveAttribute("aria-pressed", "true");
    await expect(card(page, APP)).toHaveAttribute("aria-current", "true");
    await expect.poll(() => inputFocusedTerminal(page)).toBe("p-app-t2");
    await page.keyboard.type("pwd");
    await expect.poll(() => writtenTo(page, "p-app-t2")).toBe("pwd");

    // --- 元に戻す: 拡大の前のページ(1 ページ目)のグリッドに戻り、拡大したタイルがフォーカス中のまま ---
    await restoreButton(page).click();
    await expect.poll(() => countOf(page, "toggle_tile_maximize")).toBe(2);
    await emitEvent(page, "app-snapshot", rust({ focusedTerminalId: "p-app-t2" }));
    await expect(maximizedStrip(page)).toHaveCount(0);
    await expect(pageLabel(page)).toHaveText("1/2");
    await expect.poll(() => tileIds(page)).toEqual(PAGE_ONE);
    await expect(tile(page, "app: ターミナル 2")).toHaveAttribute("aria-current", "true");
    await expect.poll(() => inputFocusedTerminal(page)).toBe("p-app-t2");

    // --- タブ表示に戻す(FR-048): フォーカス中だったタブ 1 枚とタブバー ---
    await tabsButton(page).click();
    await expect.poll(() => readCallArgs(page, "set_display_mode")).toEqual([{ mode: "tiles" }, { mode: "tabs" }]);
    await emitEvent(page, "app-snapshot", rust({ mode: "tabs", focusedTerminalId: "p-app-t2" }));
    await expect(grid(page)).toHaveCount(0);
    await expect(tabBar(page)).toBeVisible();
    await expect(tabList(page).getByRole("tab")).toHaveText(["ターミナル 1", "ターミナル 2", "ターミナル 3"]);
    await expect(tabList(page).getByRole("tab", { name: "ターミナル 2", exact: true })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    await expect(terminalHosts(page)).toHaveCount(1);
    await expect(terminalHosts(page)).toHaveAttribute("data-terminal-id", "p-app-t2");
    await expect(tabsButton(page)).toHaveAttribute("aria-pressed", "true");
    await expect.poll(() => inputFocusedTerminal(page)).toBe("p-app-t2");

    // 流れ全体で接続は作り直していない(FR-042)。キー入力はフォーカスしたターミナルにだけ届いた。
    expect(await connectCounts(page)).toEqual(ONCE_EACH);
    expect([...new Set((await readCallArgs(page, "write_terminal")).map((w) => w.terminalId))].sort()).toEqual([
      "p-app-t2",
      "p-log-t2",
    ]);
    expect(errors).toEqual([]);
  });

  test("10_枚で_1/2_を出し_2_ページ目の_1_枚に出力が続いている", async ({ page }) => {
    const errors = collectPageErrors(page);
    await open(page, rust());
    await expect(tiles(page)).toHaveCount(9);
    await expect(pageLabel(page)).toHaveText("1/2");
    // 2 ページ目のターミナルは置き場所が無い(描かない)が、接続はある。
    await expect(hostOf(page, "p-web-t3")).toHaveCount(0);
    const hidden = await channelOf(page, "p-web-t3");
    const shown = await channelOf(page, "p-app-t1");

    // --- 隠れているあいだも出力を取り込み、処理済みの量を返す(FR-052、NFR-009 の画面側)。見えているタイルも同時に流れる ---
    const sentHidden = await sendChannelBytes(page, hidden, ["hidden-line-1\r\n", "hidden-line-2\r\n"]);
    const sentShown = await sendChannelBytes(page, shown, ["shown-line-1\r\n"]);
    await expect.poll(() => ackedBytes(page, "p-web-t3")).toBe(sentHidden);
    await expect.poll(() => ackedBytes(page, "p-app-t1")).toBe(sentShown);
    await expect(hostOf(page, "p-app-t1")).toContainText("shown-line-1");

    // --- 2 ページ目を出すと、隠れているあいだの出力が描かれている ---
    await nextButton(page).click();
    await expect.poll(() => countOf(page, "next_tile_page")).toBe(1);
    await emitEvent(page, "app-snapshot", rust({ page: 1 }));
    await expect(pageLabel(page)).toHaveText("2/2");
    await expect.poll(() => tileIds(page)).toEqual(["p-web-t3"]);
    await expect(hostOf(page, "p-web-t3")).toContainText("hidden-line-1");
    await expect(hostOf(page, "p-web-t3")).toContainText("hidden-line-2");
    // 1 ページ目のタイルは隠れても取り込みを続ける。
    const sentShown2 = await sendChannelBytes(page, shown, ["shown-line-2\r\n"]);
    await expect.poll(() => ackedBytes(page, "p-app-t1")).toBe(sentShown + sentShown2);

    // --- 拡大して戻しても、描かれた中身は残り、入力が届く(FR-054) ---
    await titleBar(page, "web: ビルド").dblclick();
    await expect.poll(() => countOf(page, "toggle_tile_maximize")).toBe(1);
    // フォーカス中のタイルなので、フォーカスのコマンドは呼ばない。
    expect(await readCallArgs(page, "focus_terminal")).toEqual([]);
    await emitEvent(page, "app-snapshot", rust({ page: 1, maximized: true }));
    await expect(maximizedStrip(page).getByTestId("maximized-title")).toHaveText("web: ビルド");
    await expect(hostOf(page, "p-web-t3")).toContainText("hidden-line-2");
    await expect.poll(() => inputFocusedTerminal(page)).toBe("p-web-t3");
    await page.keyboard.type("make");
    await expect.poll(() => writtenTo(page, "p-web-t3")).toBe("make");
    await restoreButton(page).click();
    await expect.poll(() => countOf(page, "toggle_tile_maximize")).toBe(2);
    await emitEvent(page, "app-snapshot", rust({ page: 1 }));
    await expect(pageLabel(page)).toHaveText("2/2");
    await expect(hostOf(page, "p-web-t3")).toContainText("hidden-line-2");

    // --- 1 ページ目へ戻ると 2 ページ目はまた隠れ、出力は続き、もう一度出すと描かれている ---
    await previousButton(page).click();
    await expect.poll(() => countOf(page, "previous_tile_page")).toBe(1);
    await emitEvent(page, "app-snapshot", rust({ page: 0 }));
    await expect(pageLabel(page)).toHaveText("1/2");
    await expect(hostOf(page, "p-web-t3")).toHaveCount(0);
    await expect(hostOf(page, "p-app-t1")).toContainText("shown-line-2");
    const sentHidden2 = await sendChannelBytes(page, hidden, ["hidden-again\r\n"]);
    await expect.poll(() => ackedBytes(page, "p-web-t3")).toBe(sentHidden + sentHidden2);
    await nextButton(page).click();
    await expect.poll(() => countOf(page, "next_tile_page")).toBe(2);
    await emitEvent(page, "app-snapshot", rust({ page: 1 }));
    await expect(hostOf(page, "p-web-t3")).toContainText("hidden-again");
    await expect(hostOf(page, "p-web-t3")).toContainText("hidden-line-1");

    // 接続は作り直さない(FR-042)。
    expect(await connectCounts(page)).toEqual(ONCE_EACH);
    expect(errors).toEqual([]);
  });

  test("a_のタイルにフォーカス中に_b_のタイルをクリックするとカードが_b_になりツリーのルートが_b_になる", async ({
    page,
  }) => {
    const errors = collectPageErrors(page);
    await open(page, rust());
    await expect(card(page, APP)).toHaveAttribute("aria-current", "true");
    await expect(fileTree(page, APP)).toBeVisible();
    await expect(treeRow(page, "README.md")).toBeVisible();
    await expect(tile(page, "app: ターミナル 1")).toHaveAttribute("aria-current", "true");
    await expect.poll(() => inputFocusedTerminal(page)).toBe("p-app-t1");

    // --- B のタイルをクリック。写しが届くまでは、カードもツリーも A のまま(選択は Rust が決める) ---
    await titleBar(page, "ribbitlog: ターミナル 1").click();
    await expect.poll(() => readCallArgs(page, "focus_terminal")).toEqual([{ terminalId: "p-log-t1" }]);
    await settle(page);
    await expect(card(page, APP)).toHaveAttribute("aria-current", "true");
    await expect(fileTree(page, APP)).toBeVisible();

    // --- Rust の写し: カードは B が選択状態、ツリーのルートは B(AC-M15)。B のルートを読みに行く ---
    await emitEvent(page, "app-snapshot", rust({ focusedTerminalId: "p-log-t1" }));
    await expect(card(page, LOG)).toHaveAttribute("aria-current", "true");
    await expect(card(page, APP)).not.toHaveAttribute("aria-current", "true");
    await expect(treeHeader(page)).toHaveText("ribbitlog");
    await expect(fileTree(page, LOG)).toBeVisible();
    await expect(fileTree(page, APP)).toHaveCount(0);
    await expect(treeRow(page, "Cargo.toml")).toBeVisible();
    await expect(treeRow(page, "README.md")).toHaveCount(0);
    expect((await readCallArgs(page, "list_directory")).filter((a) => a.projectId === LOG.id).map((a) => a.path)).toEqual([
      "",
    ]);
    await expect(tile(page, "ribbitlog: ターミナル 1")).toHaveAttribute("aria-current", "true");
    await expect(tile(page, "app: ターミナル 1")).not.toHaveAttribute("aria-current", "true");

    // 入力のフォーカスとキー入力の宛先は B のターミナル。
    await expect.poll(() => inputFocusedTerminal(page)).toBe("p-log-t1");
    await page.keyboard.type("cargo");
    await expect.poll(() => writtenTo(page, "p-log-t1")).toBe("cargo");
    expect(await writtenTo(page, "p-app-t1")).toBe("");

    // ツリーのメニューの「パスをターミナルに挿入」の宛先も、フォーカス中の B のタイル。
    await treeRow(page, "Cargo.toml").click({ button: "right" });
    await expect(page.getByRole("menu")).toBeVisible();
    await page.getByRole("menu").getByRole("menuitem", { name: "パスをターミナルに挿入", exact: true }).click();
    await expect
      .poll(() => readCallArgs(page, "insert_entry_path"))
      .toEqual([{ projectId: LOG.id, paths: ["Cargo.toml"], terminalId: "p-log-t1" }]);

    // --- A のタイルへ戻すと、カードとツリーも A に戻る(A のツリーの中身もそのまま) ---
    await titleBar(page, "app: ターミナル 3").click();
    await expect
      .poll(() => readCallArgs(page, "focus_terminal"))
      .toEqual([{ terminalId: "p-log-t1" }, { terminalId: "p-app-t3" }]);
    await emitEvent(page, "app-snapshot", rust({ focusedTerminalId: "p-app-t3" }));
    await expect(card(page, APP)).toHaveAttribute("aria-current", "true");
    await expect(card(page, LOG)).not.toHaveAttribute("aria-current", "true");
    await expect(fileTree(page, APP)).toBeVisible();
    await expect(treeRow(page, "README.md")).toBeVisible();
    await expect(tile(page, "app: ターミナル 3")).toHaveAttribute("aria-current", "true");
    await expect.poll(() => inputFocusedTerminal(page)).toBe("p-app-t3");
    expect(errors).toEqual([]);
  });

  test("緑の_a_のカードとタイルの色の印が同じ色相", async ({ page }) => {
    const errors = collectPageErrors(page);
    // 緑の app を最後に置き、app のタイルが 2 つのページにまたがるようにする(1 ページ目の 8・9 枚目、2 ページ目の 2 枚)。
    const green: SampleProject = {
      id: "p-green",
      name: "app",
      color: "green",
      tabs: ["ターミナル 1", "ターミナル 2", "ターミナル 3", "テスト"],
    };
    const others: SampleProject[] = [
      { id: "p-o", name: "ribbitlog", color: "orange", tabs: ["ターミナル 1", "サーバー"] },
      { id: "p-p", name: "notes", color: "purple", tabs: ["ターミナル 1", "ターミナル 2"] },
      { id: "p-t", name: "web", color: "teal", tabs: ["ターミナル 1", "ターミナル 2", "ビルド"] },
    ];
    const base = projectsSnapshot([...others, green]);
    await open(page, withTileDisplay(base));
    await expect(pageLabel(page)).toHaveText("1/2");

    const greenCard = await dotColor(card(page, green));
    const greenOnPageOne = ["app: ターミナル 1", "app: ターミナル 2"];
    const othersOnPageOne = [
      "ribbitlog: ターミナル 1",
      "ribbitlog: サーバー",
      "notes: ターミナル 1",
      "notes: ターミナル 2",
      "web: ターミナル 1",
      "web: ターミナル 2",
      "web: ビルド",
    ];
    // タイトルは「プロジェクト名: タブ名」(FR-061)。
    await expect(page.getByTestId("tile-title")).toHaveText([...othersOnPageOne, ...greenOnPageOne]);
    // 緑の app の全タイルの印は、app のカードの印と同じ色(AC-C02)。
    for (const title of greenOnPageOne) expect(await dotColor(tile(page, title)), title).toBe(greenCard);
    // ほかのプロジェクトのタイルの印は、それぞれのカードと同じで、緑とは違う。
    for (const project of others) {
      const projectCard = await dotColor(card(page, project));
      expect(projectCard, `${project.name} のカード`).not.toBe(greenCard);
      for (const title of othersOnPageOne.filter((t) => t.startsWith(`${project.name}: `))) {
        expect(await dotColor(tile(page, title)), title).toBe(projectCard);
      }
    }

    // --- 2 ページ目の app のタイルも同じ色 ---
    await nextButton(page).click();
    await expect.poll(() => countOf(page, "next_tile_page")).toBe(1);
    await emitEvent(page, "app-snapshot", withTileDisplay(base, { page: 1 }));
    await expect(pageLabel(page)).toHaveText("2/2");
    await expect(page.getByTestId("tile-title")).toHaveText(["app: ターミナル 3", "app: テスト"]);
    for (const title of ["app: ターミナル 3", "app: テスト"]) {
      expect(await dotColor(tile(page, title)), title).toBe(greenCard);
    }
    // カードの色はページを送っても変わらない(選択中になっても印の色は同じ)。
    await expect(card(page, green)).toHaveAttribute("aria-current", "true");
    expect(await dotColor(card(page, green))).toBe(greenCard);

    // --- 拡大中の帯の印も同じ色 ---
    await titleBar(page, "app: テスト").dblclick();
    await expect.poll(() => countOf(page, "toggle_tile_maximize")).toBe(1);
    await emitEvent(page, "app-snapshot", withTileDisplay(base, { focusedTerminalId: "p-green-t4", maximized: true }));
    await expect(maximizedStrip(page).getByTestId("maximized-title")).toHaveText("app: テスト");
    expect(await dotColor(maximizedStrip(page))).toBe(greenCard);
    expect(errors).toEqual([]);
  });

  test("別のページで鳴ったベルの印がページ表示に出て_送ると消える", async ({ page }) => {
    const errors = collectPageErrors(page);
    await open(page, rust());
    await expect(pageLabel(page)).toHaveText("1/2");
    await expect(pageMark(nextButton(page))).toHaveCount(0);

    // --- ベル: 2 ページ目の web: ビルド、同じページのフォーカスの無い ribbitlog: ターミナル 1、フォーカス中の app: ターミナル 1 ---
    // ベルの検出は Rust の読み取りスレッドの役目(T48c)。画面は出力として取り込むだけで、印は写しで受け取る。
    const bell = "\u0007";
    const targets = ["p-web-t3", "p-log-t1", "p-app-t1"];
    for (const id of targets) {
      const sent = await sendChannelBytes(page, await channelOf(page, id), [`done${bell}\r\n`]);
      await expect.poll(() => ackedBytes(page, id)).toBe(sent);
    }
    // Rust の写し: いま見ている(フォーカス中の)app: ターミナル 1 には印を立てない(計画【仮定】14、ADR-011 の 7)。
    await emitEvent(page, "app-snapshot", rust({ attention: targets }));
    await expect(attentionMark(titleBar(page, "ribbitlog: ターミナル 1"))).toBeVisible();
    await expect(attentionMark(titleBar(page, "app: ターミナル 1"))).toHaveCount(0);
    // 別のページの印は、そのページの方向のボタン(次のページ)に付き、読み上げの名前にも出る。
    await expect(pageMark(nextButton(page))).toBeVisible();
    await expect(nextButton(page)).toHaveAttribute("aria-label", /新しい通知があります/);
    await expect(pageMark(previousButton(page))).toHaveCount(0);

    // --- 次のページへ送る: 先頭の web: ビルドにフォーカスが移るので、その印が消える ---
    await nextButton(page).click();
    await expect.poll(() => countOf(page, "next_tile_page")).toBe(1);
    await emitEvent(page, "app-snapshot", rust({ page: 1, attention: ["p-web-t3", "p-log-t1"] }));
    await expect(pageLabel(page)).toHaveText("2/2");
    await expect(tile(page, "web: ビルド")).toHaveAttribute("aria-current", "true");
    await expect(attentionMark(titleBar(page, "web: ビルド"))).toHaveCount(0);
    // 1 ページ目の ribbitlog の印は残っているので、こんどは前のページのボタンに付く。
    await expect(pageMark(previousButton(page))).toBeVisible();
    await expect(pageMark(nextButton(page))).toHaveCount(0);

    // --- 前のページへ戻る: 見えただけでは消えない(フォーカスは先頭の app へ) ---
    await previousButton(page).click();
    await expect.poll(() => countOf(page, "previous_tile_page")).toBe(1);
    await emitEvent(page, "app-snapshot", rust({ page: 0, attention: ["p-log-t1"] }));
    await expect(pageLabel(page)).toHaveText("1/2");
    await expect(attentionMark(titleBar(page, "ribbitlog: ターミナル 1"))).toBeVisible();
    await expect(pageMark(previousButton(page))).toHaveCount(0);
    await expect(pageMark(nextButton(page))).toHaveCount(0);

    // --- そのタイルにフォーカスすると消える ---
    await titleBar(page, "ribbitlog: ターミナル 1").click();
    await expect.poll(() => readCallArgs(page, "focus_terminal")).toEqual([{ terminalId: "p-log-t1" }]);
    await emitEvent(page, "app-snapshot", rust({ focusedTerminalId: "p-log-t1", attention: ["p-log-t1"] }));
    await expect(tile(page, "ribbitlog: ターミナル 1")).toHaveAttribute("aria-current", "true");
    await expect(attentionMark(titleBar(page, "ribbitlog: ターミナル 1"))).toHaveCount(0);
    await expect(attentionMark(page.getByTestId("tile-grid"))).toHaveCount(0);
    expect(errors).toEqual([]);
  });

  test("再読み込みしても写しの表示モードで開く", async ({ page }) => {
    const errors = collectPageErrors(page);
    // 終了時にタイル表示で、フォーカス中のタイルが 2 ページ目だった(Rust は保存した表示モードとフォーカスで写しを作る)。
    const saved = rust({ focusedTerminalId: "p-web-t3" });
    await open(page, saved);

    const verify = async (when: string) => {
      await expect(grid(page), when).toBeVisible();
      await expect(tabBar(page), when).toHaveCount(0);
      await expect(tilesButton(page), when).toHaveAttribute("aria-pressed", "true");
      await expect(tabsButton(page), when).toHaveAttribute("aria-pressed", "false");
      await expect(pageLabel(page), when).toHaveText("2/2");
      await expect.poll(() => tileIds(page), { message: when }).toEqual(["p-web-t3"]);
      await expect(tile(page, "web: ビルド"), when).toHaveAttribute("aria-current", "true");
      await expect(card(page, WEB), when).toHaveAttribute("aria-current", "true");
      await expect(fileTree(page, WEB), when).toBeVisible();
      await expect.poll(() => inputFocusedTerminal(page), { message: when }).toBe("p-web-t3");
      // 描いただけでは表示のコマンドを送らない(表示モードは Rust が持つ)。
      await settle(page);
      const sent = (await commands(page)).filter((c) =>
        ["set_display_mode", "focus_terminal", "next_tile_page", "previous_tile_page", "toggle_tile_maximize"].includes(c),
      );
      expect(sent, when).toEqual([]);
    };

    await verify("起動");
    // 再起動の代わりに読み込み直す(偽の Rust 側は同じ写しを返す)。もう一度タイル表示で開く。
    await page.reload();
    await waitReady(page);
    await verify("読み込み直し");
    expect(await connectCounts(page)).toEqual(ONCE_EACH);
    expect(errors).toEqual([]);
  });
});
