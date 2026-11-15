// M3 のシナリオの画面の自動テスト(T44b)。
//
// PRD §8「UI」のツリーの操作を、機能をまたいだ 1 本の流れで通す(展開 → 新規ファイル → 名称変更 → ドラッグでの移動 →
// 衝突(両方残す)→ コピーとペースト → ターミナルへの挿入 → ゴミ箱 → ドラッグ中の外からの変更(AC-T25)→
// 消された項目の名称変更(AC-T24)→ プロジェクトの切替と戻り)。個々の機能の細かい振る舞いは tree.spec.ts・
// tree-ops.spec.ts・tree-dnd.spec.ts が見るので、ここでは繰り返さない。
//
// 偽の Rust 側(fixtures/tauriMock.ts)は、操作の結果の写しやフォルダの中身を自分からは送り返さない。そのため各操作の前に
// 読み直されるフォルダの応答を "pending"(応答を待たせる)にしておき、操作のあと「コマンドが正しい引数で呼ばれた」ことを
// 確かめてから、本物の Rust が返すはずの中身を置いて(待っていた問い合わせに返して)画面が変わることを確かめる。
// 外からの変更は、中身を置き換えてから tree-changed を流す(本物の Rust と同じ順)。
//
// 画面のデザインを別の作業が直しているので、要素は役割とアクセシブルな名前(getByRole)と data-testid(と行の data-path)
// で探し、見た目(色・寸法・位置・クラス名)は検査しない。
//
// 画面側の展開の時間(NFR-005)は記録だけで、数値では失敗させない。testreport/perf/e2e-tree-expand-*.json に書く。

import { expect, type Locator, type Page, test } from "@playwright/test";

import type { AppSnapshot } from "../src/ipc/generated/AppSnapshot";
import type { EntryKind } from "../src/ipc/generated/EntryKind";
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
  setMockListing,
  setMockReply,
} from "./fixtures/tauriMock";

const APP: SampleProject = { id: "p-app", name: "app", color: "green", displayPath: "~/dev/app" };
const LOG: SampleProject = { id: "p-log", name: "ribbitlog", color: "orange", displayPath: "~/dev/ribbitlog" };
const APP_TERMINAL = "p-app-t1";

type Entries = ReadonlyArray<readonly [string, EntryKind]>;

const ROOT_ENTRIES: Entries = [
  ["docs", "directory"],
  ["src", "directory"],
  ["README.md", "file"],
  ["a b.ts", "file"],
  ["a.md", "file"],
];
const SRC_ENTRIES: Entries = [
  ["lib", "directory"],
  ["main.ts", "file"],
];
const LOG_ROOT_ENTRIES: Entries = [
  ["Cargo.toml", "file"],
  ["crates", "directory"],
];

const SNAPSHOT_APP = projectsSnapshot([APP, LOG]);
const SNAPSHOT_LOG: AppSnapshot = { ...SNAPSHOT_APP, selectedProjectId: LOG.id };

function scenarioListings(extra: Record<string, MockListing> = {}): Record<string, MockListing> {
  return {
    [listingKey(APP.id, "")]: listed(ROOT_ENTRIES),
    [listingKey(APP.id, "src")]: listed(SRC_ENTRIES),
    [listingKey(APP.id, "docs")]: listed([["a.md", "file"]]),
    [listingKey(LOG.id, "")]: listed(LOG_ROOT_ENTRIES),
    [listingKey(LOG.id, "crates")]: listed([["arin-core", "directory"]]),
    ...extra,
  };
}

/** 画面のコンソールのエラーと、捕まえられなかった例外を集める。 */
function collectPageErrors(page: Page): string[] {
  const errors: string[] = [];
  page.on("console", (message) => {
    if (message.type() === "error") errors.push(message.text());
  });
  page.on("pageerror", (error) => errors.push(error.message));
  return errors;
}

async function open(page: Page, snapshot: AppSnapshot, listings: Record<string, MockListing>): Promise<void> {
  await installTauriMock(page, { get_app_snapshot: { ok: snapshot } }, listings);
  await page.goto("/");
  await waitReady(page);
}

async function waitReady(page: Page): Promise<void> {
  await expect
    .poll(async () => (await readRecordedCalls(page)).map((call) => call.command))
    .toContain("notify_app_ready");
}

// --- 要素(役割と名前、data-testid) ---

/** 表示中のプロジェクトのツリー(WAI-ARIA の tree。名前は「<プロジェクト名> のファイル」)。 */
const fileTree = (page: Page, project: SampleProject) => page.getByRole("tree", { name: `${project.name} のファイル` });
const rows = (page: Page) => page.getByTestId("tree-row");
/** パスで行を指す(テストのパスは CSS の文字列として JSON の書き方で足りるものだけ)。 */
const row = (page: Page, rel: string) => page.locator(`[data-testid="tree-row"][data-path=${JSON.stringify(rel)}]`);
const toggle = (page: Page, rel: string) => row(page, rel).getByTestId("tree-row-toggle");
const menu = (page: Page) => page.getByRole("menu");
const menuItem = (page: Page, name: string) => menu(page).getByRole("menuitem", { name, exact: true });
const textbox = (page: Page, name: string) => page.getByRole("textbox", { name, exact: true });
const dialog = (page: Page, name: string) => page.getByRole("alertdialog", { name, exact: true });
const treeNotice = (page: Page) => page.getByTestId("tree-notice");
const terminalHost = (page: Page) => page.getByTestId("terminal-host");
const ghost = (page: Page) => page.getByTestId("tree-drag-ghost");
const projectCard = (page: Page, project: SampleProject) =>
  page.getByTestId("project-card").and(page.getByRole("button", { name: project.name, exact: true }));

/** 描かれている行のパス(上から順)。 */
async function rowPaths(page: Page): Promise<string[]> {
  return rows(page).evaluateAll((elements) => elements.map((el) => el.getAttribute("data-path") ?? ""));
}

/** フォルダの中身の問い合わせのパス(届いた順)。 */
async function listedPaths(page: Page, projectId: string): Promise<string[]> {
  return (await readCallArgs(page, "list_directory")).filter((a) => a.projectId === projectId).map((a) => a.path);
}

async function listedCount(page: Page, projectId: string, dir: string): Promise<number> {
  return (await listedPaths(page, projectId)).filter((p) => p === dir).length;
}

/** 届いたコマンドの名前(届いた順)。 */
async function commands(page: Page): Promise<string[]> {
  return (await readRecordedCalls(page)).map((call) => call.command);
}

/**
 * 画面が受け取ったイベントの処理と描き直しが済むのを待つ(2 フレーム)。届いた変更が保留されて「何も起きない」ことを
 * 確かめる前に使う(tree-changed の受け取りは同期で読み込みのコマンドを呼ぶので、保留されていなければこの間に記録に残る)。
 */
async function settle(page: Page): Promise<void> {
  await page.evaluate(
    () => new Promise<void>((resolve) => requestAnimationFrame(() => requestAnimationFrame(() => resolve()))),
  );
}

async function centerOf(locator: Locator): Promise<{ x: number; y: number }> {
  const box = await locator.boundingBox();
  if (box === null) throw new Error("要素の大きさが取れない(表示されていない)");
  return { x: box.x + box.width / 2, y: box.y + box.height / 2 };
}

/** 行を押し、しきい値を超えて動かしてドラッグを始める(pointer のイベント。ADR-007 の (b))。 */
async function startDrag(page: Page, source: Locator): Promise<void> {
  const from = await centerOf(source);
  await page.mouse.move(from.x, from.y);
  await page.mouse.down();
  await page.mouse.move(from.x + 6, from.y + 6, { steps: 3 });
  await expect(ghost(page)).toBeVisible();
}

/** ドラッグ中に target の上へ動かし、そこが落とし先として強調されるのを待つ。 */
async function hoverDropTarget(page: Page, target: Locator): Promise<void> {
  const to = await centerOf(target);
  await page.mouse.move(to.x, to.y, { steps: 4 });
  await expect(target).toHaveAttribute("data-drop-active", "true");
}

async function dragAndDrop(page: Page, source: Locator, target: Locator): Promise<void> {
  await startDrag(page, source);
  await hoverDropTarget(page, target);
  await page.mouse.up();
}

async function chooseFromMenu(page: Page, target: Locator, item: string): Promise<void> {
  await target.click({ button: "right" });
  await expect(menu(page)).toBeVisible();
  await menuItem(page, item).click();
  await expect(menu(page)).toHaveCount(0);
}

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

// 各テストの終わりに、画面の失敗(例外、通信の失敗の報告)が無かったことと、
// 画面から Rust へ送った引数に絶対パスとドットドットが 1 つも無いこと(設計書 §12。絶対パスは Rust だけが持つ)を見る。
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

test.describe("M3 のシナリオ(PRD §8 UI のツリーの操作)", () => {
  test("ツリーの展開_新規作成_名称変更_コピーとペースト_ゴミ箱_ドラッグでの移動_ターミナルへの挿入を通す", async ({
    page,
  }) => {
    test.setTimeout(90_000);
    const errors = collectPageErrors(page);
    // src は展開したときに読む。応答を待たせ、問い合わせを確かめてから返す。
    await open(page, SNAPSHOT_APP, scenarioListings({ [listingKey(APP.id, "src")]: "pending" }));
    const tree = fileTree(page, APP);
    await expect(tree).toBeVisible();
    await expect.poll(() => rowPaths(page)).toEqual(["docs", "src", "README.md", "a b.ts", "a.md"]);
    await expect(terminalHost(page)).toBeVisible();
    expect(await listedPaths(page, APP.id)).toEqual([""]);

    // --- 展開(FR-013): 中身は展開したときに求める ---
    await toggle(page, "src").click();
    await expect.poll(() => listedPaths(page, APP.id)).toEqual(["", "src"]);
    await expect(row(page, "src")).toHaveAttribute("aria-expanded", "true");
    await expect(row(page, "src/main.ts")).toHaveCount(0);
    await setMockListing(page, listingKey(APP.id, "src"), listed(SRC_ENTRIES));
    await expect.poll(() => rowPaths(page)).toEqual(["docs", "src", "src/lib", "src/main.ts", "README.md", "a b.ts", "a.md"]);

    // --- 新規ファイル(FR-025、AC-T13): src に util.ts を作り、作った行が選ばれる ---
    await chooseFromMenu(page, row(page, "src"), "新規ファイル…");
    const newName = textbox(page, "新しいファイルの名前");
    await expect(newName).toBeFocused();
    await setMockReply(page, "create_entry", { ok: { kind: "done", select: "src/util.ts" } });
    await setMockListing(page, listingKey(APP.id, "src"), "pending");
    await newName.fill("util.ts");
    await newName.press("Enter");
    await expect
      .poll(() => readCallArgs(page, "create_entry"))
      .toEqual([{ projectId: APP.id, parent: "src", name: "util.ts", kind: "file" }]);
    await expect(newName).toHaveCount(0);
    await setMockListing(
      page,
      listingKey(APP.id, "src"),
      listed([
        ["lib", "directory"],
        ["main.ts", "file"],
        ["util.ts", "file"],
      ]),
    );
    await expect(row(page, "src/util.ts")).toHaveAttribute("aria-selected", "true");
    await expect(tree).toBeFocused();

    // --- 名称変更(FR-026): util.ts → helper.ts ---
    await chooseFromMenu(page, row(page, "src/util.ts"), "名称変更…");
    const renameInput = textbox(page, "「util.ts」の新しい名前");
    await expect(renameInput).toBeFocused();
    await setMockReply(page, "rename_entry", { ok: { kind: "done", select: "src/helper.ts" } });
    await setMockListing(page, listingKey(APP.id, "src"), "pending");
    await renameInput.fill("helper.ts");
    await renameInput.press("Enter");
    await expect
      .poll(() => readCallArgs(page, "rename_entry"))
      .toEqual([{ projectId: APP.id, path: "src/util.ts", newName: "helper.ts" }]);
    await setMockListing(
      page,
      listingKey(APP.id, "src"),
      listed([
        ["lib", "directory"],
        ["helper.ts", "file"],
        ["main.ts", "file"],
      ]),
    );
    await expect(row(page, "src/helper.ts")).toHaveAttribute("aria-selected", "true");
    await expect(row(page, "src/util.ts")).toHaveCount(0);

    // --- ほかのフォルダへドラッグで移動(FR-029): 確認を経て移す。移した先(閉じていた docs)が開き、移した項目が選ばれる ---
    await dragAndDrop(page, row(page, "src/helper.ts"), row(page, "docs"));
    const moveHelper = dialog(page, "「helper.ts」を「docs」に移動しますか?");
    await expect(moveHelper).toBeVisible();
    expect(await readCallArgs(page, "move_entry"), "確認の前には移さない").toEqual([]);
    await setMockReply(page, "move_entry", { ok: { kind: "done", select: "docs/helper.ts" } });
    await setMockListing(page, listingKey(APP.id, "src"), "pending");
    await setMockListing(page, listingKey(APP.id, "docs"), "pending");
    await moveHelper.getByRole("button", { name: "移動", exact: true }).click();
    await expect(moveHelper).toHaveCount(0);
    await expect
      .poll(() => readCallArgs(page, "move_entry"))
      .toEqual([{ projectId: APP.id, source: "src/helper.ts", destDir: "docs" }]);
    await setMockListing(page, listingKey(APP.id, "src"), listed(SRC_ENTRIES));
    await setMockListing(
      page,
      listingKey(APP.id, "docs"),
      listed([
        ["a.md", "file"],
        ["helper.ts", "file"],
      ]),
    );
    await expect(row(page, "docs/helper.ts")).toHaveAttribute("aria-selected", "true");
    await expect
      .poll(() => rowPaths(page))
      .toEqual(["docs", "docs/a.md", "docs/helper.ts", "src", "src/lib", "src/main.ts", "README.md", "a b.ts", "a.md"]);

    // --- 衝突(FR-030、AC-T18): 同名のある docs へ a.md を移すと 3 択が出て、両方残すで対処を付けて呼び直す ---
    await setMockReply(page, "move_entry", { ok: { kind: "conflict", name: "a.md", total: 1 } });
    await dragAndDrop(page, row(page, "a.md"), row(page, "docs"));
    await dialog(page, "「a.md」を「docs」に移動しますか?").getByRole("button", { name: "移動", exact: true }).click();
    const conflict = dialog(page, "「a.md」という名前の項目がすでにあります");
    await expect(conflict).toBeVisible();
    await expect.poll(async () => (await readCallArgs(page, "move_entry")).length).toBe(2);
    await setMockReply(page, "move_entry", { ok: { kind: "done", select: "docs/a 2.md" } });
    await setMockListing(page, listingKey(APP.id, ""), "pending");
    await setMockListing(page, listingKey(APP.id, "docs"), "pending");
    await conflict.getByRole("button", { name: "両方残す", exact: true }).click();
    await expect(conflict).toHaveCount(0);
    await expect
      .poll(() => readCallArgs(page, "move_entry"))
      .toEqual([
        { projectId: APP.id, source: "src/helper.ts", destDir: "docs" },
        { projectId: APP.id, source: "a.md", destDir: "docs" },
        { projectId: APP.id, source: "a.md", destDir: "docs", resolution: "keepBoth" },
      ]);
    await setMockListing(
      page,
      listingKey(APP.id, ""),
      listed([
        ["docs", "directory"],
        ["src", "directory"],
        ["README.md", "file"],
        ["a b.ts", "file"],
      ]),
    );
    await setMockListing(
      page,
      listingKey(APP.id, "docs"),
      listed([
        ["a 2.md", "file"],
        ["a.md", "file"],
        ["helper.ts", "file"],
      ]),
    );
    await expect(row(page, "docs/a 2.md")).toHaveAttribute("aria-selected", "true");
    await expect
      .poll(() => rowPaths(page))
      .toEqual([
        "docs",
        "docs/a 2.md",
        "docs/a.md",
        "docs/helper.ts",
        "src",
        "src/lib",
        "src/main.ts",
        "README.md",
        "a b.ts",
      ]);

    // --- コピーとペースト(FR-027): README.md をコピーし、src にペーストする ---
    await chooseFromMenu(page, row(page, "README.md"), "コピー");
    await expect.poll(() => readCallArgs(page, "copy_entries")).toEqual([{ projectId: APP.id, paths: ["README.md"] }]);
    await setMockReply(page, "paste_entries", { ok: { kind: "done", select: "src/README.md" } });
    await setMockListing(page, listingKey(APP.id, "src"), "pending");
    await chooseFromMenu(page, row(page, "src"), "ペースト");
    await expect.poll(() => readCallArgs(page, "paste_entries")).toEqual([{ projectId: APP.id, destDir: "src" }]);
    await setMockListing(
      page,
      listingKey(APP.id, "src"),
      listed([
        ["lib", "directory"],
        ["main.ts", "file"],
        ["README.md", "file"],
      ]),
    );
    await expect(row(page, "src/README.md")).toHaveAttribute("aria-selected", "true");
    // コピーなので元も残る。
    await expect(row(page, "README.md")).toBeVisible();

    // --- ターミナルへドラッグしてパスを挿入(FR-016、AC-T04 の画面側) ---
    // 挿入の文字列(エスケープと末尾の空白)は Rust が作って PTY に書く。画面はターミナルへ何も書かず、挿入のコマンドだけを呼ぶ。
    const beforeInsert = (await commands(page)).length;
    await dragAndDrop(page, row(page, "a b.ts"), terminalHost(page));
    await expect
      .poll(() => readCallArgs(page, "insert_entry_path"))
      .toEqual([{ projectId: APP.id, paths: ["a b.ts"], terminalId: APP_TERMINAL }]);
    await settle(page);
    expect((await commands(page)).slice(beforeInsert)).toEqual(["insert_entry_path"]);
    await expect(terminalHost(page)).not.toHaveAttribute("data-drop-active", "true");

    // --- ゴミ箱(FR-028、AC-T16 の画面側): ペーストした src/README.md をゴミ箱に入れると行が消える ---
    await setMockListing(page, listingKey(APP.id, "src"), "pending");
    await chooseFromMenu(page, row(page, "src/README.md"), "ゴミ箱に入れる");
    await expect.poll(() => readCallArgs(page, "trash_entry")).toEqual([{ projectId: APP.id, path: "src/README.md" }]);
    await setMockListing(page, listingKey(APP.id, "src"), listed(SRC_ENTRIES));
    await expect(row(page, "src/README.md")).toHaveCount(0);
    await expect(row(page, "src/main.ts")).toBeVisible();

    // --- ドラッグ中の外からの変更(FR-037、AC-T25): 100 ファイルが足されても行が動かず、落としたあとに反映される ---
    const beforeDrag = await rowPaths(page);
    const srcLoads = await listedCount(page, APP.id, "src");
    await startDrag(page, row(page, "src/main.ts"));
    await hoverDropTarget(page, row(page, "docs"));
    const added: Entries = Array.from({ length: 100 }, (_, i) => [`new-${String(i).padStart(3, "0")}.ts`, "file"] as const);
    await setMockListing(page, listingKey(APP.id, "src"), listed([["lib", "directory"], ...added, ["main.ts", "file"]]));
    await emitEvent(page, "tree-changed", { projectId: APP.id, dirs: ["src"], fullReload: false });
    await settle(page);
    expect(await listedCount(page, APP.id, "src"), "ドラッグ中は読み直さない").toBe(srcLoads);
    expect(await rowPaths(page)).toEqual(beforeDrag);
    await expect(row(page, "docs")).toHaveAttribute("data-drop-active", "true");
    // docs に落とすと移動の確認が出る(確認の表示中も保留が続く)。中止して何も移さない。
    await page.mouse.up();
    const moveMain = dialog(page, "「main.ts」を「docs」に移動しますか?");
    await expect(moveMain).toBeVisible();
    await settle(page);
    expect(await listedCount(page, APP.id, "src"), "確認の表示中は読み直さない").toBe(srcLoads);
    expect(await rowPaths(page)).toEqual(beforeDrag);
    await moveMain.getByRole("button", { name: "中止", exact: true }).click();
    await expect(moveMain).toHaveCount(0);
    // 保留が解けると、積んだ変更をまとめて 1 回で読み直して反映する。
    await expect(row(page, "src/new-000.ts")).toBeVisible();
    await expect(row(page, "src/new-005.ts")).toBeVisible();
    expect(await listedCount(page, APP.id, "src")).toBe(srcLoads + 1);
    expect(await readCallArgs(page, "move_entry"), "中止では移さない").toHaveLength(3);
    // 以降の行を見やすくするため src を閉じる(開閉はコマンドを呼ばない)。
    await toggle(page, "src").click();
    await expect(row(page, "src/main.ts")).toHaveCount(0);

    // --- メニューを開いたあとに外から消された項目の名称変更(FR-036、AC-T24) ---
    await row(page, "docs/helper.ts").click({ button: "right" });
    await expect(menu(page)).toBeVisible();
    const docsLoads = await listedCount(page, APP.id, "docs");
    // 外から docs/helper.ts が消された。メニューの表示中なので読み直さない。
    await setMockListing(page, listingKey(APP.id, "docs"), "pending");
    await emitEvent(page, "tree-changed", { projectId: APP.id, dirs: ["docs"], fullReload: false });
    await settle(page);
    expect(await listedCount(page, APP.id, "docs"), "メニューの表示中は読み直さない").toBe(docsLoads);
    await setMockReply(page, "rename_entry", { ok: { kind: "notFound" } });
    await menuItem(page, "名称変更…").click();
    const goneInput = textbox(page, "「helper.ts」の新しい名前");
    await expect(goneInput).toBeFocused();
    await goneInput.fill("renamed.ts");
    await goneInput.press("Enter");
    await expect
      .poll(async () => (await readCallArgs(page, "rename_entry")).at(-1))
      .toEqual({ projectId: APP.id, path: "docs/helper.ts", newName: "renamed.ts" });
    await expect(treeNotice(page)).toContainText("「helper.ts」が見つかりません");
    await setMockListing(
      page,
      listingKey(APP.id, "docs"),
      listed([
        ["a 2.md", "file"],
        ["a.md", "file"],
      ]),
    );
    await expect(row(page, "docs/helper.ts")).toHaveCount(0);
    await expect.poll(() => rowPaths(page)).toEqual(["docs", "docs/a 2.md", "docs/a.md", "src", "README.md", "a b.ts"]);

    // --- プロジェクトを切り替えて戻ると、ツリーの状態(開いたフォルダ)が保たれる(FR-002) ---
    await row(page, "docs/a.md").click();
    const beforeSwitch = await rowPaths(page);
    await setMockListing(page, listingKey(LOG.id, ""), "pending");
    await projectCard(page, LOG).click();
    await expect.poll(() => readCallArgs(page, "select_project")).toEqual([{ projectId: LOG.id }]);
    // 写しが届くまでは切り替えない(選択中のプロジェクトは Rust が決める)。
    await expect(tree).toBeVisible();
    await emitEvent(page, "app-snapshot", SNAPSHOT_LOG);
    await expect(fileTree(page, LOG)).toBeVisible();
    await expect.poll(() => listedPaths(page, LOG.id)).toEqual([""]);
    await setMockListing(page, listingKey(LOG.id, ""), listed(LOG_ROOT_ENTRIES));
    await expect.poll(() => rowPaths(page)).toEqual(["Cargo.toml", "crates"]);

    await projectCard(page, APP).click();
    await expect
      .poll(() => readCallArgs(page, "select_project"))
      .toEqual([{ projectId: LOG.id }, { projectId: APP.id }]);
    await emitEvent(page, "app-snapshot", SNAPSHOT_APP);
    await expect(fileTree(page, APP)).toBeVisible();
    await expect.poll(() => rowPaths(page)).toEqual(beforeSwitch);
    await expect(row(page, "docs/a.md")).toHaveAttribute("aria-selected", "true");

    // ターミナルへは画面から何も書いていない(挿入は Rust の役目)。
    expect(await readCallArgs(page, "write_terminal")).toEqual([]);
    expect(await readCallArgs(page, "write_terminal_bytes")).toEqual([]);
    expect(errors).toEqual([]);
  });

  test("プロジェクトを切り替えるとツリーと読み込みの対象が切り替わる", async ({ page }) => {
    const errors = collectPageErrors(page);
    await open(page, SNAPSHOT_APP, scenarioListings());
    await expect.poll(() => rowPaths(page)).toEqual(["docs", "src", "README.md", "a b.ts", "a.md"]);
    await toggle(page, "src").click();
    await row(page, "src/main.ts").click();
    await expect(row(page, "src/main.ts")).toHaveAttribute("aria-selected", "true");

    // --- B(ribbitlog)へ ---
    await projectCard(page, LOG).click();
    await expect.poll(() => readCallArgs(page, "select_project")).toEqual([{ projectId: LOG.id }]);
    await emitEvent(page, "app-snapshot", SNAPSHOT_LOG);
    await expect(fileTree(page, LOG)).toBeVisible();
    await expect(fileTree(page, APP)).toHaveCount(0);
    await expect.poll(() => rowPaths(page)).toEqual(["Cargo.toml", "crates"]);
    const appLoads = (await listedPaths(page, APP.id)).length;

    // 隠れている A の外からの変更は受けない(読み直さない)。
    await setMockListing(
      page,
      listingKey(APP.id, "src"),
      listed([...SRC_ENTRIES, ["new.txt", "file"]]),
    );
    await emitEvent(page, "tree-changed", { projectId: APP.id, dirs: ["src"], fullReload: false });
    await settle(page);
    expect((await listedPaths(page, APP.id)).length, "隠れたプロジェクトのフォルダは読まない").toBe(appLoads);

    // B の展開・操作は B の ID で呼ぶ。挿入は B のターミナルへ。
    await toggle(page, "crates").click();
    await expect.poll(() => listedPaths(page, LOG.id)).toEqual(["", "crates"]);
    await expect(row(page, "crates/arin-core")).toBeVisible();
    await chooseFromMenu(page, row(page, "Cargo.toml"), "相対パスをコピー");
    await chooseFromMenu(page, row(page, "Cargo.toml"), "パスをターミナルに挿入");
    await expect
      .poll(() => readCallArgs(page, "copy_entry_path"))
      .toEqual([{ projectId: LOG.id, path: "Cargo.toml", kind: "relative" }]);
    await expect
      .poll(() => readCallArgs(page, "insert_entry_path"))
      .toEqual([{ projectId: LOG.id, paths: ["Cargo.toml"], terminalId: "p-log-t1" }]);

    // --- A へ戻る: 開いたフォルダと選択を保ち、読み込み済みのフォルダを読み直す(隠れている間の変更も拾う) ---
    await projectCard(page, APP).click();
    await expect
      .poll(() => readCallArgs(page, "select_project"))
      .toEqual([{ projectId: LOG.id }, { projectId: APP.id }]);
    await emitEvent(page, "app-snapshot", SNAPSHOT_APP);
    await expect(fileTree(page, APP)).toBeVisible();
    await expect
      .poll(async () => [...new Set((await listedPaths(page, APP.id)).slice(appLoads))].sort())
      .toEqual(["", "src"]);
    await expect
      .poll(() => rowPaths(page))
      .toEqual(["docs", "src", "src/lib", "src/main.ts", "src/new.txt", "README.md", "a b.ts", "a.md"]);
    await expect(row(page, "src/main.ts")).toHaveAttribute("aria-selected", "true");
    // B のフォルダは、戻ったあとは読まない。
    const logLoads = (await listedPaths(page, LOG.id)).length;
    await emitEvent(page, "tree-changed", { projectId: LOG.id, dirs: [""], fullReload: false });
    await settle(page);
    expect((await listedPaths(page, LOG.id)).length).toBe(logLoads);
    expect(errors).toEqual([]);
  });
});

/** 書き出しに使う node:fs の部分(e2e/ の tsconfig は Node の型を入れていないので、使う関数の形だけを書く)。 */
interface NodeFsSubset {
  mkdirSync(path: string, options: { recursive: true }): unknown;
  writeFileSync(path: string, data: string): void;
}

/** node:fs を読み込む(指定を string の変数にして、型を NodeFsSubset に絞る)。 */
async function nodeFs(): Promise<NodeFsSubset> {
  const specifier: string = "node:fs";
  return (await import(specifier)) as NodeFsSubset;
}

/** リポジトリのルート(playwright.config.ts のあるフォルダ)。 */
function repoRoot(): string {
  const configFile = test.info().config.configFile;
  if (configFile === undefined) throw new Error("Playwright の設定ファイルの場所が分からない");
  return configFile.slice(0, configFile.lastIndexOf("/"));
}

test.describe("画面側の展開の時間(NFR-005。記録だけ)", () => {
  const COUNT = 100_000;
  const RUNS = 3;

  test("10_万件のフォルダを展開して行が出るまでの時間を記録する", async ({ page, browserName, browser }) => {
    test.setTimeout(180_000);
    const files: Entries = Array.from({ length: COUNT }, (_, i) => [`file-${String(i + 1).padStart(6, "0")}.txt`, "file"] as const);
    await installTauriMock(page, { get_app_snapshot: { ok: projectsSnapshot([APP]) } }, {
      [listingKey(APP.id, "")]: listed([
        ["big", "directory"],
        ["README.md", "file"],
      ]),
      [listingKey(APP.id, "big")]: listed(files),
    });

    const runs: Array<{ rowsShownMs: number; paintedMs: number; drawnRows: number }> = [];
    for (let run = 0; run < RUNS; run++) {
      // 毎回読み込み直し、読み込み済みでない状態から展開する(偽の Rust 側も差し込み直される)。
      if (run === 0) await page.goto("/");
      else await page.reload();
      await waitReady(page);
      await expect(row(page, "big")).toBeVisible();

      // 開閉の印のクリックが届いた時刻から、最初の子の行が DOM に出た時刻と、その次の 2 フレーム(描かれた)までを測る。
      // 偽の Rust 側が 10 万件の中身を写して返す時間も含む(本物では IPC の受け渡しにあたる)。
      await page.evaluate((firstChild) => {
        const w = window as unknown as { __ARIN_EXPAND__?: { start?: number; shown?: number; painted?: number } };
        const record: { start?: number; shown?: number; painted?: number } = {};
        w.__ARIN_EXPAND__ = record;
        const selector = `[data-testid="tree-row"][data-path=${JSON.stringify(firstChild)}]`;
        document.addEventListener(
          "click",
          () => {
            record.start = performance.now();
          },
          { capture: true, once: true },
        );
        const observer = new MutationObserver(() => {
          if (record.start === undefined || document.querySelector(selector) === null) return;
          observer.disconnect();
          record.shown = performance.now();
          requestAnimationFrame(() =>
            requestAnimationFrame(() => {
              record.painted = performance.now();
            }),
          );
        });
        observer.observe(document.body, { childList: true, subtree: true });
      }, "big/file-000001.txt");

      await toggle(page, "big").click();
      await expect(row(page, "big/file-000001.txt")).toBeVisible({ timeout: 60_000 });
      const readTiming = () =>
        page.evaluate(() => {
          const r = (window as unknown as { __ARIN_EXPAND__?: { start?: number; shown?: number; painted?: number } })
            .__ARIN_EXPAND__;
          return r?.start !== undefined && r.shown !== undefined && r.painted !== undefined
            ? { rowsShownMs: r.shown - r.start, paintedMs: r.painted - r.start }
            : null;
        });
      await expect.poll(readTiming).not.toBeNull();
      const timing = await readTiming();
      if (timing === null) throw new Error("展開の時間が測れなかった");
      // 描いた行は見える数だけ(全部を DOM に出していない)ことを、記録に添える。
      const drawnRows = await rows(page).count();
      runs.push({ ...timing, drawnRows });
    }

    const sorted = (values: number[]) => [...values].sort((a, b) => a - b);
    const median = (values: number[]) => sorted(values)[Math.floor(values.length / 2)] ?? Number.NaN;
    const round = (value: number) => Math.round(value * 10) / 10;
    const result = {
      kind: "e2e-tree-expand",
      requirement: "NFR-005(画面側。記録だけで合否にしない)",
      recordedAt: new Date().toISOString(),
      browser: `${browserName} ${browser.version()}`,
      entries: COUNT,
      runs: runs.map((r) => ({ rowsShownMs: round(r.rowsShownMs), paintedMs: round(r.paintedMs), drawnRows: r.drawnRows })),
      medianRowsShownMs: round(median(runs.map((r) => r.rowsShownMs))),
      medianPaintedMs: round(median(runs.map((r) => r.paintedMs))),
      note:
        "Vite の画面を Playwright の WebKit で開き、偽の Rust 側(e2e/fixtures/tauriMock.ts)が 10 万件を返す。" +
        "開閉の印の click から、最初の子の行が DOM に出るまで(rowsShownMs)と、その後の 2 フレームまで(paintedMs)。" +
        "偽の Rust 側の JSON の写しの時間を含み、Rust の読み込み・並べ替えの時間は含まない(それは T44a の perf_tree)。",
    };
    const fs = await nodeFs();
    const outDir = `${repoRoot()}/testreport/perf`;
    fs.mkdirSync(outDir, { recursive: true });
    const stamp = result.recordedAt.replace(/[-:]/g, "").replace(/\..*$/, "").replace("T", "-");
    const body = `${JSON.stringify(result, null, 2)}\n`;
    fs.writeFileSync(`${outDir}/e2e-tree-expand-${stamp}.json`, body);
    fs.writeFileSync(`${outDir}/e2e-tree-expand-latest.json`, body);
    test.info().annotations.push({
      type: "NFR-005",
      description: `10 万件の展開: 行が出るまで 中央値 ${result.medianRowsShownMs}ms、描画まで 中央値 ${result.medianPaintedMs}ms(${RUNS} 回)`,
    });

    // 数値では失敗させない。測れたことだけを確かめる。
    expect(runs).toHaveLength(RUNS);
    for (const r of runs) expect(Number.isFinite(r.paintedMs)).toBe(true);
  });
});
