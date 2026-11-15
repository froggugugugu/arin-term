// ツリーの操作(T43c): コンテキストメニュー、ショートカット、インライン編集、衝突の確認、コピー・切り取り・ペーストの振り分け。
//
// 偽の Rust 側(fixtures/tauriMock.ts)のフォルダごとの応答(listings)を置いて画面を開き、右クリック・キー・
// クリップボードのイベントで操作して、呼ばれたコマンドと引数・行の並び・知らせを見る。
// ファイル操作のあとの並びは、本物の Rust と同じく、応答を置き換えてから(操作の結果で)読み直させる。
//
// ⌘C・⌘X・⌘V は実アプリではメニューの「編集」の項目が copy・cut・paste のイベントにする。この画面のテストには
// メニューが無く、本物のクリップボードにも触れたくないので、フォーカスのある要素へ ClipboardEvent を発火する
// (e2e/terminal.spec.ts と同じ形)。メニューの項目で受けるキーは OS が受けるので、ここでは扱わない。

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
const TERMINAL_ID = "p-app-t1";

type Entries = ReadonlyArray<readonly [string, EntryKind]>;

const ROOT_ENTRIES: Entries = [
  ["docs", "directory"],
  ["src", "directory"],
  ["README.md", "file"],
  ["a b.ts", "file"],
];
const ROOT = listed(ROOT_ENTRIES);
const SRC = listed([
  ["lib", "directory"],
  ["main.ts", "file"],
]);

function appListings(extra: Record<string, MockListing> = {}): Record<string, MockListing> {
  return {
    [listingKey(APP.id, "")]: ROOT,
    [listingKey(APP.id, "src")]: SRC,
    [listingKey(APP.id, "src/lib")]: listed([]),
    [listingKey(APP.id, "docs")]: listed([["a.md", "file"]]),
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

async function open(
  page: Page,
  snapshot: AppSnapshot = projectsSnapshot([APP]),
  listings: Record<string, MockListing> = appListings(),
): Promise<void> {
  await installTauriMock(page, { get_app_snapshot: { ok: snapshot } }, listings);
  await page.goto("/");
  await expect
    .poll(async () => (await readRecordedCalls(page)).map((call) => call.command))
    .toContain("notify_app_ready");
  await expect(row(page, "README.md")).toBeVisible();
}

const fileTree = (page: Page) => page.getByTestId("file-tree");
const scroller = (page: Page) => page.getByTestId("tree-scroller");
const header = (page: Page) => page.getByTestId("tree-header");
const rows = (page: Page) => page.getByTestId("tree-row");
const row = (page: Page, path: string) => page.locator(`[data-testid="tree-row"][data-path=${JSON.stringify(path)}]`);
const toggle = (page: Page, path: string) => row(page, path).getByTestId("tree-row-toggle");
const menu = (page: Page) => page.getByTestId("context-menu");
/** メニューの項目(menuitem・menuitemcheckbox)。 */
const menuItems = (page: Page) => menu(page).locator('[role^="menuitem"]');
const menuItem = (page: Page, label: string) =>
  menuItems(page).filter({ has: page.locator(".context-menu__label", { hasText: new RegExp(`^${escapeRegExp(label)}$`) }) });
const nameInput = (page: Page) => page.getByTestId("tree-name-input");
const notice = (page: Page) => page.getByTestId("tree-notice");
const conflictDialog = (page: Page) => page.getByTestId("tree-conflict-dialog");

function escapeRegExp(text: string): string {
  return text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

async function rowPaths(page: Page): Promise<string[]> {
  return rows(page).evaluateAll((elements) => elements.map((el) => el.getAttribute("data-path") ?? ""));
}

/** メニューの項目の名前とキーの表示(上から順)。 */
async function menuLabels(page: Page): Promise<string[]> {
  return menu(page)
    .locator(".context-menu__label")
    .evaluateAll((elements) => elements.map((el) => el.textContent ?? ""));
}

async function menuShortcuts(page: Page): Promise<Record<string, string>> {
  return menuItems(page).evaluateAll((elements) =>
    Object.fromEntries(
      elements.map((el) => [
        el.querySelector(".context-menu__label")?.textContent ?? "",
        el.querySelector(".context-menu__shortcut")?.textContent ?? "",
      ]),
    ),
  );
}

/** 無効の項目の名前。 */
async function disabledLabels(page: Page): Promise<string[]> {
  return menuItems(page).evaluateAll((elements) =>
    elements
      .filter((el) => el.getAttribute("aria-disabled") === "true")
      .map((el) => el.querySelector(".context-menu__label")?.textContent ?? ""),
  );
}

/** 行の空いたところ(名前の右)を右クリックする。 */
async function rightClick(locator: Locator): Promise<void> {
  await locator.click({ button: "right" });
}

/** 行の無い余白(ツリーの下の方)を右クリックする。 */
async function rightClickBlank(page: Page): Promise<void> {
  const box = await scroller(page).boundingBox();
  if (box === null) throw new Error("ツリーの大きさが取れない");
  await page.mouse.click(box.x + box.width / 2, box.y + box.height - 10, { button: "right" });
}

async function chooseFromMenu(page: Page, target: Locator, label: string): Promise<void> {
  await rightClick(target);
  await menuItem(page, label).click();
}

interface ClipboardResult {
  prevented: boolean;
  target: string;
}

/** フォーカスのある要素へ copy・cut・paste のイベントを発火する(メニューの「編集」が発火するのと同じ形)。 */
async function fireClipboard(page: Page, type: "copy" | "cut" | "paste", text = ""): Promise<ClipboardResult> {
  return page.evaluate(
    ([type, text]) => {
      const data = new DataTransfer();
      if (text !== "") data.setData("text/plain", text);
      const event = new ClipboardEvent(type, { clipboardData: data, bubbles: true, cancelable: true });
      const target = document.activeElement ?? document.body;
      target.dispatchEvent(event);
      return { prevented: event.defaultPrevented, target: target.getAttribute("data-testid") ?? target.className };
    },
    [type, text] as const,
  );
}

/** ファイル操作のコマンド(フォルダの中身の問い合わせなどを除く)の名前を、届いた順に返す。 */
async function fileCommands(page: Page): Promise<string[]> {
  const ops = new Set([
    "create_entry",
    "rename_entry",
    "move_entry",
    "copy_entry",
    "trash_entry",
    "copy_entries",
    "cut_entries",
    "paste_entries",
    "reveal_entry",
    "open_entry",
    "copy_entry_path",
    "insert_entry_path",
    "set_show_hidden",
    "write_terminal",
  ]);
  return (await readRecordedCalls(page)).map((call) => call.command).filter((command) => ops.has(command));
}

/** 入力欄の選択範囲。 */
async function inputSelection(page: Page): Promise<{ value: string; start: number | null; end: number | null }> {
  return nameInput(page).evaluate((el) => {
    const input = el as HTMLInputElement;
    return { value: input.value, start: input.selectionStart, end: input.selectionEnd };
  });
}

test.describe("コンテキストメニュー(FR-024、FR-035)", () => {
  test("ファイルの上で右クリックすると表の項目が出る", async ({ page }) => {
    const errors = collectPageErrors(page);
    await open(page);

    await rightClick(row(page, "README.md"));

    await expect(menu(page)).toBeVisible();
    await expect(menu(page)).toHaveAttribute("role", "menu");
    expect(await menuLabels(page)).toEqual([
      "開く",
      "Finder で表示",
      "パスをターミナルに挿入",
      "相対パスをコピー",
      "絶対パスをコピー",
      "切り取り",
      "コピー",
      "ペースト",
      "名称変更…",
      "ゴミ箱に入れる",
      "新規ファイル…",
      "新規フォルダ…",
    ]);
    // キーの表示はショートカットの表から(macOS の記号の順 ⌃⌥⇧⌘)。
    expect(await menuShortcuts(page)).toEqual({
      開く: "⌘O",
      "Finder で表示": "⌥⌘R",
      パスをターミナルに挿入: "⌘↩",
      相対パスをコピー: "⌥⇧⌘C",
      絶対パスをコピー: "⌥⌘C",
      切り取り: "⌘X",
      コピー: "⌘C",
      ペースト: "⌘V",
      "名称変更…": "↩",
      ゴミ箱に入れる: "⌘⌫",
      "新規ファイル…": "⌘N",
      "新規フォルダ…": "⇧⌘N",
    });
    expect(await disabledLabels(page)).toEqual([]);
    await expect(menu(page).getByRole("separator")).toHaveCount(5);
    // 右クリックした行が選ばれ、メニューの表示中は保留される。
    await expect(row(page, "README.md")).toHaveAttribute("aria-selected", "true");
    // 最初の項目にフォーカスがある(キーボードで操作できる)。
    await expect(menuItem(page, "開く")).toBeFocused();
    expect(errors).toEqual([]);
  });

  test("フォルダの上でも同じ項目が出て_開くでフォルダが開く", async ({ page }) => {
    await open(page);

    await chooseFromMenu(page, row(page, "src"), "開く");

    await expect(row(page, "src/main.ts")).toBeVisible();
    await expect(menu(page)).toHaveCount(0);
    expect(await readCallArgs(page, "open_entry")).toEqual([]);
  });

  test("余白の右クリックで_7_項目が出る", async ({ page }) => {
    await open(page);
    await row(page, "README.md").click();

    await rightClickBlank(page);

    expect(await menuLabels(page)).toEqual([
      "新規ファイル…",
      "新規フォルダ…",
      "ペースト",
      "Finder で表示",
      "ルートの絶対パスをコピー",
      "隠しファイルを表示",
      "すべて折りたたむ",
    ]);
    // 隠しファイルの表示は印の付く項目。
    const hidden = menuItem(page, "隠しファイルを表示");
    await expect(hidden).toHaveAttribute("role", "menuitemcheckbox");
    await expect(hidden).toHaveAttribute("aria-checked", "false");
    // 余白の右クリックは選択を外す。
    await expect(row(page, "README.md")).toHaveAttribute("aria-selected", "false");

    await hidden.click();
    await expect.poll(() => readCallArgs(page, "set_show_hidden")).toEqual([{ projectId: APP.id, showHidden: true }]);
  });

  test("ルートでは名称変更_切り取り_ゴミ箱が無効", async ({ page }) => {
    await open(page);

    await rightClick(header(page));

    await expect(header(page)).toHaveAttribute("aria-selected", "true");
    expect(await disabledLabels(page)).toEqual(["切り取り", "名称変更…", "ゴミ箱に入れる"]);
    // 無効の項目は押しても何もせず、メニューも閉じない。
    await menuItem(page, "名称変更…").click({ force: true });
    await expect(menu(page)).toBeVisible();
    await expect(nameInput(page)).toHaveCount(0);
    // キーボードの移動は無効の項目を飛ばす(コピー → ペースト → 新規ファイル…)。
    await menuItem(page, "コピー").focus();
    await page.keyboard.press("ArrowDown");
    await expect(menuItem(page, "ペースト")).toBeFocused();
    await page.keyboard.press("ArrowDown");
    await expect(menuItem(page, "新規ファイル…")).toBeFocused();

    // ショートカットと切り取りのイベントも何もしない。
    await page.keyboard.press("Escape");
    await expect(menu(page)).toHaveCount(0);
    await expect(fileTree(page)).toBeFocused();
    await page.keyboard.press("Enter");
    await page.keyboard.press("Meta+Backspace");
    // 切り取れないときは既定の動作に任せる(空の中身でクリップボードを書き換えない)。
    const cut = await fireClipboard(page, "cut");
    expect(cut.prevented).toBe(false);
    await expect(nameInput(page)).toHaveCount(0);
    expect(await fileCommands(page)).toEqual([]);
  });

  test("esc_と外側のクリックで閉じ_閉じるとツリーにフォーカスが戻る", async ({ page }) => {
    await open(page);

    await rightClick(row(page, "a b.ts"));
    await expect(menu(page)).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(menu(page)).toHaveCount(0);
    await expect(fileTree(page)).toBeFocused();

    await rightClick(row(page, "a b.ts"));
    await page.mouse.click(5, 5);
    await expect(menu(page)).toHaveCount(0);

    // キーボードで選んで実行する(↓ で「Finder で表示」、Enter)。
    await rightClick(row(page, "a b.ts"));
    await page.keyboard.press("ArrowDown");
    await page.keyboard.press("Enter");
    await expect(menu(page)).toHaveCount(0);
    await expect(fileTree(page)).toBeFocused();
    await expect.poll(() => readCallArgs(page, "reveal_entry")).toEqual([{ projectId: APP.id, path: "a b.ts" }]);
  });

  test("finder_で表示のコマンドが呼ばれる", async ({ page }) => {
    await open(page);

    await chooseFromMenu(page, row(page, "README.md"), "Finder で表示");
    await rightClickBlank(page);
    await menuItem(page, "Finder で表示").click();

    await expect
      .poll(() => readCallArgs(page, "reveal_entry"))
      .toEqual([
        { projectId: APP.id, path: "README.md" },
        { projectId: APP.id, path: "" },
      ]);
  });

  test("相対パスをコピーと絶対パスをコピーで正しいコマンドが呼ばれる", async ({ page }) => {
    await open(page);

    await chooseFromMenu(page, row(page, "a b.ts"), "相対パスをコピー");
    await chooseFromMenu(page, row(page, "a b.ts"), "絶対パスをコピー");
    await rightClickBlank(page);
    await menuItem(page, "ルートの絶対パスをコピー").click();

    await expect
      .poll(() => readCallArgs(page, "copy_entry_path"))
      .toEqual([
        { projectId: APP.id, path: "a b.ts", kind: "relative" },
        { projectId: APP.id, path: "a b.ts", kind: "absolute" },
        { projectId: APP.id, path: "", kind: "absolute" },
      ]);
  });

  test("パスをターミナルに挿入で表示中のターミナルへの挿入が呼ばれる", async ({ page }) => {
    await open(page);

    await chooseFromMenu(page, row(page, "a b.ts"), "パスをターミナルに挿入");

    await expect
      .poll(() => readCallArgs(page, "insert_entry_path"))
      .toEqual([{ projectId: APP.id, paths: ["a b.ts"], terminalId: TERMINAL_ID }]);
  });

  test("タブが無ければパスをターミナルに挿入は無効", async ({ page }) => {
    await open(page, projectsSnapshot([{ ...APP, tabs: [] }]));

    await rightClick(row(page, "a b.ts"));

    expect(await disabledLabels(page)).toEqual(["パスをターミナルに挿入"]);
  });

  test("切り取り_コピー_ペーストの項目でそれぞれのコマンドが呼ばれる", async ({ page }) => {
    await open(page);

    await chooseFromMenu(page, row(page, "README.md"), "切り取り");
    await chooseFromMenu(page, row(page, "a b.ts"), "コピー");
    await chooseFromMenu(page, row(page, "src"), "ペースト");
    await chooseFromMenu(page, row(page, "README.md"), "ペースト");

    await expect
      .poll(() => fileCommands(page))
      .toEqual(["cut_entries", "copy_entries", "paste_entries", "paste_entries"]);
    expect(await readCallArgs(page, "cut_entries")).toEqual([{ projectId: APP.id, paths: ["README.md"] }]);
    expect(await readCallArgs(page, "copy_entries")).toEqual([{ projectId: APP.id, paths: ["a b.ts"] }]);
    // フォルダの上ならその中、ファイルの上なら同じ階層(FR-027)。
    expect(await readCallArgs(page, "paste_entries")).toEqual([
      { projectId: APP.id, destDir: "src" },
      { projectId: APP.id, destDir: "" },
    ]);
  });
});

test.describe("インライン編集(FR-025、FR-026、【仮定】9・10)", () => {
  test("新規ファイルで_src_に入力欄が出て_a.ts_と_enter_で作成のコマンドが呼ばれ作った行が選ばれる", async ({ page }) => {
    const errors = collectPageErrors(page);
    await open(page);

    await chooseFromMenu(page, row(page, "src"), "新規ファイル…");

    // フォルダが開き、その中(1 段深い位置)に入力欄が出る。
    await expect(nameInput(page)).toBeFocused();
    await expect(row(page, "src/main.ts")).toBeVisible();
    await expect(page.getByTestId("tree-new-entry")).toHaveAttribute("aria-level", "3");
    await expect(nameInput(page)).toHaveValue("");

    await setMockReply(page, "create_entry", { ok: { kind: "done", select: "src/a.ts" } });
    await setMockListing(
      page,
      listingKey(APP.id, "src"),
      listed([
        ["lib", "directory"],
        ["a.ts", "file"],
        ["main.ts", "file"],
      ]),
    );
    await nameInput(page).fill("a.ts");
    await nameInput(page).press("Enter");

    await expect(nameInput(page)).toHaveCount(0);
    await expect
      .poll(() => readCallArgs(page, "create_entry"))
      .toEqual([{ projectId: APP.id, parent: "src", name: "a.ts", kind: "file" }]);
    await expect(row(page, "src/a.ts")).toHaveAttribute("aria-selected", "true");
    await expect(fileTree(page)).toBeFocused();
    expect(errors).toEqual([]);
  });

  test("ファイルの上の新規ファイルは同じ階層_余白ではルート", async ({ page }) => {
    await open(page);
    await toggle(page, "src").click();
    await expect(row(page, "src/main.ts")).toBeVisible();

    await chooseFromMenu(page, row(page, "src/main.ts"), "新規ファイル…");
    await expect(page.getByTestId("tree-new-entry")).toHaveAttribute("aria-level", "3");
    await nameInput(page).fill("x.ts");
    await nameInput(page).press("Enter");

    await rightClickBlank(page);
    await menuItem(page, "新規フォルダ…").click();
    await expect(page.getByTestId("tree-new-entry")).toHaveAttribute("aria-level", "2");
    await nameInput(page).fill("tmp");
    await nameInput(page).press("Enter");

    await expect
      .poll(() => readCallArgs(page, "create_entry"))
      .toEqual([
        { projectId: APP.id, parent: "src", name: "x.ts", kind: "file" },
        { projectId: APP.id, parent: "", name: "tmp", kind: "folder" },
      ]);
  });

  test("esc_で取り消すと何も呼ばない", async ({ page }) => {
    await open(page);

    await chooseFromMenu(page, row(page, "README.md"), "新規ファイル…");
    await nameInput(page).fill("draft.ts");
    await nameInput(page).press("Escape");

    await expect(nameInput(page)).toHaveCount(0);
    await expect(fileTree(page)).toBeFocused();

    // フォーカスが外れても取り消す(【仮定】9)。
    await chooseFromMenu(page, row(page, "README.md"), "名称変更…");
    await nameInput(page).fill("other.md");
    await page.getByTestId("project-card").first().focus();
    await expect(nameInput(page)).toHaveCount(0);
    await expect(row(page, "README.md")).toHaveText("README.md");

    expect(await fileCommands(page)).toEqual([]);
  });

  test("変換中の_enter_では確定しない", async ({ page }) => {
    await open(page);
    await chooseFromMenu(page, row(page, "README.md"), "新規ファイル…");
    await nameInput(page).fill("にほんご");

    // IME の変換を確定する Enter(isComposing)。
    await nameInput(page).evaluate((el) =>
      el.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", isComposing: true, bubbles: true, cancelable: true })),
    );

    await expect(nameInput(page)).toBeVisible();
    expect(await readCallArgs(page, "create_entry")).toEqual([]);
  });

  test("名称変更は拡張子を除いた部分が選ばれた入力欄になる", async ({ page }) => {
    await open(page);

    await chooseFromMenu(page, row(page, "README.md"), "名称変更…");

    await expect(nameInput(page)).toBeFocused();
    // 行の中の名前の位置に出し、行の高さは変えない。
    const box = await row(page, "README.md").boundingBox();
    expect(box?.height).toBe(22);
    await expect(row(page, "README.md").getByTestId("tree-name-input")).toHaveCount(1);
    expect(await inputSelection(page)).toEqual({ value: "README.md", start: 0, end: 6 });
    await nameInput(page).press("Escape");

    // フォルダは全体。Enter のキー(表の tree.rename)でも始まる。
    await row(page, "docs").click();
    await page.keyboard.press("Enter");
    expect(await inputSelection(page)).toEqual({ value: "docs", start: 0, end: 4 });
  });

  test("確定すると名称変更のコマンドが呼ばれ新しい名前が出る", async ({ page }) => {
    await open(page);
    await chooseFromMenu(page, row(page, "README.md"), "名称変更…");

    await setMockReply(page, "rename_entry", { ok: { kind: "done", select: "GUIDE.md" } });
    await setMockListing(
      page,
      listingKey(APP.id, ""),
      listed([
        ["docs", "directory"],
        ["src", "directory"],
        ["a b.ts", "file"],
        ["GUIDE.md", "file"],
      ]),
    );
    await nameInput(page).fill("GUIDE.md");
    await nameInput(page).press("Enter");

    await expect
      .poll(() => readCallArgs(page, "rename_entry"))
      .toEqual([{ projectId: APP.id, path: "README.md", newName: "GUIDE.md" }]);
    await expect(row(page, "GUIDE.md")).toHaveAttribute("aria-selected", "true");
    await expect(row(page, "README.md")).toHaveCount(0);
  });

  test("名前を変えずに確定すると何も呼ばない", async ({ page }) => {
    await open(page);
    await chooseFromMenu(page, row(page, "README.md"), "名称変更…");

    await nameInput(page).press("Enter");

    await expect(nameInput(page)).toHaveCount(0);
    expect(await readCallArgs(page, "rename_entry")).toEqual([]);
  });

  test("入力中のコピーとペーストは入力欄の文字の操作になりツリーの操作にしない", async ({ page }) => {
    await open(page);
    await chooseFromMenu(page, row(page, "README.md"), "名称変更…");

    const copy = await fireClipboard(page, "copy");
    const paste = await fireClipboard(page, "paste", "x");
    // 入力欄の中のキーはツリーのショートカットにしない(⌘⌫ はゴミ箱にしない)。
    await nameInput(page).press("Meta+Backspace");

    expect(copy).toEqual({ prevented: false, target: "tree-name-input" });
    expect(paste.prevented).toBe(false);
    await expect(nameInput(page)).toBeVisible();
    expect(await fileCommands(page)).toEqual([]);
  });

  test("名前が正しくないと返ると_1_行の知らせが出る", async ({ page }) => {
    await open(page);
    await setMockReply(page, "create_entry", { ok: { kind: "invalidName", reason: "containsSlash" } });

    await chooseFromMenu(page, header(page), "新規ファイル…");
    await nameInput(page).fill("a/b");
    await nameInput(page).press("Enter");

    await expect(notice(page)).toHaveText("名前に「/」は使えません");
  });
});

test.describe("ショートカット(FR-024、【要件の追加】)", () => {
  test("表の画面で受けるキーでメニューと同じ操作が呼ばれる", async ({ page }) => {
    await open(page);
    await row(page, "a b.ts").click();
    await expect(fileTree(page)).toBeFocused();

    await page.keyboard.press("Meta+O");
    await page.keyboard.press("Meta+Alt+R");
    await page.keyboard.press("Meta+Enter");
    await page.keyboard.press("Meta+Shift+Alt+C");
    await page.keyboard.press("Meta+Alt+C");

    await expect
      .poll(() => fileCommands(page))
      .toEqual(["open_entry", "reveal_entry", "insert_entry_path", "copy_entry_path", "copy_entry_path"]);
    expect(await readCallArgs(page, "open_entry")).toEqual([{ projectId: APP.id, path: "a b.ts" }]);
    expect(await readCallArgs(page, "insert_entry_path")).toEqual([
      { projectId: APP.id, paths: ["a b.ts"], terminalId: TERMINAL_ID },
    ]);
    expect((await readCallArgs(page, "copy_entry_path")).map((a) => a.kind)).toEqual(["relative", "absolute"]);

    // ⌘N・⇧⌘N は入力欄を出す(ファイルの上なら同じ階層)。
    await page.keyboard.press("Meta+N");
    await expect(nameInput(page)).toBeFocused();
    await nameInput(page).press("Escape");
    await page.keyboard.press("Meta+Shift+N");
    await expect(nameInput(page)).toBeFocused();
    await nameInput(page).fill("new-dir");
    await nameInput(page).press("Enter");
    await expect
      .poll(() => readCallArgs(page, "create_entry"))
      .toEqual([{ projectId: APP.id, parent: "", name: "new-dir", kind: "folder" }]);
  });

  test("cmd_backspace_でゴミ箱のコマンドが呼ばれ行が消える", async ({ page }) => {
    await open(page);
    await row(page, "README.md").click();

    await setMockListing(
      page,
      listingKey(APP.id, ""),
      listed([
        ["docs", "directory"],
        ["src", "directory"],
        ["a b.ts", "file"],
      ]),
    );
    await page.keyboard.press("Meta+Backspace");

    await expect.poll(() => readCallArgs(page, "trash_entry")).toEqual([{ projectId: APP.id, path: "README.md" }]);
    await expect(row(page, "README.md")).toHaveCount(0);
    await expect.poll(() => rowPaths(page)).toEqual(["docs", "src", "a b.ts"]);
  });

  test("ツリーにフォーカスが無いときはキーを受けない", async ({ page }) => {
    await open(page);
    await row(page, "README.md").click();
    await page.getByTestId("project-card").first().focus();

    await page.keyboard.press("Meta+Backspace");
    await page.keyboard.press("Meta+O");

    expect(await fileCommands(page)).toEqual([]);
  });
});

test.describe("見つからない・衝突(FR-030、FR-036)", () => {
  test("見つからないが返ると見つかりませんと出て行が消える", async ({ page }) => {
    await open(page);
    await rightClick(row(page, "README.md"));
    // メニューを開いたあとに外から消された(AC-T24)。
    await setMockListing(
      page,
      listingKey(APP.id, ""),
      listed([
        ["docs", "directory"],
        ["src", "directory"],
        ["a b.ts", "file"],
      ]),
    );
    await setMockReply(page, "rename_entry", { ok: { kind: "notFound" } });
    await menuItem(page, "名称変更…").click();
    await nameInput(page).fill("b.md");
    await nameInput(page).press("Enter");

    await expect(notice(page)).toHaveText("「README.md」が見つかりません");
    await expect(row(page, "README.md")).toHaveCount(0);
    expect(await readCallArgs(page, "rename_entry")).toEqual([{ projectId: APP.id, path: "README.md", newName: "b.md" }]);
  });

  test("衝突が返ると置き換える_両方残す_中止が出て中止では何も呼ばない", async ({ page }) => {
    const errors = collectPageErrors(page);
    await open(page);
    await setMockReply(page, "rename_entry", { ok: { kind: "conflict", name: "a b.ts", total: 1 } });

    await chooseFromMenu(page, row(page, "README.md"), "名称変更…");
    await nameInput(page).fill("a b.ts");
    await nameInput(page).press("Enter");

    const dialog = conflictDialog(page);
    await expect(dialog).toBeVisible();
    await expect(dialog).toHaveAttribute("role", "alertdialog");
    await expect(dialog.getByRole("heading")).toHaveText("「a b.ts」という名前の項目がすでにあります");
    await expect(dialog).toContainText("ゴミ箱");
    // macOS のアラートの並び: 左に置き換える(取り返しのつく破壊的な操作)、右に中止と既定の両方残す。
    await expect(dialog.getByRole("button")).toHaveText(["置き換える", "中止", "両方残す"]);
    await expect(dialog.getByRole("button", { name: "両方残す" })).toBeFocused();

    await dialog.getByRole("button", { name: "中止" }).click();
    await expect(dialog).toHaveCount(0);
    expect(await readCallArgs(page, "rename_entry")).toHaveLength(1);

    // Esc でも中止になる。
    await chooseFromMenu(page, row(page, "README.md"), "名称変更…");
    await nameInput(page).fill("a b.ts");
    await nameInput(page).press("Enter");
    await expect(dialog).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(dialog).toHaveCount(0);
    expect(await readCallArgs(page, "rename_entry")).toHaveLength(2);
    expect(errors).toEqual([]);
  });

  test("両方残すと置き換えるは対処を付けて呼び直す", async ({ page }) => {
    await open(page);
    const conflict = { ok: { kind: "conflict", name: "a.md", total: 1 } } as const;
    const done = { ok: { kind: "done", select: "docs/a 2.md" } } as const;

    // Return は既定のボタン(両方残す)。
    await setMockReply(page, "paste_entries", conflict);
    await chooseFromMenu(page, row(page, "docs"), "ペースト");
    await expect(conflictDialog(page)).toBeVisible();
    await setMockReply(page, "paste_entries", done);
    await page.keyboard.press("Enter");
    await expect(conflictDialog(page)).toHaveCount(0);

    // 置き換えるは押したときだけ。
    await setMockReply(page, "paste_entries", conflict);
    await chooseFromMenu(page, row(page, "docs"), "ペースト");
    await expect(conflictDialog(page)).toBeVisible();
    await setMockReply(page, "paste_entries", done);
    await conflictDialog(page).getByRole("button", { name: "置き換える" }).click();
    await expect(conflictDialog(page)).toHaveCount(0);

    await expect
      .poll(() => readCallArgs(page, "paste_entries"))
      .toEqual([
        { projectId: APP.id, destDir: "docs" },
        { projectId: APP.id, destDir: "docs", resolution: "keepBoth" },
        { projectId: APP.id, destDir: "docs" },
        { projectId: APP.id, destDir: "docs", resolution: "replace" },
      ]);
  });
});

test.describe("複数項目の衝突(FR-030)", () => {
  test("衝突した項目が_2_つ以上なら数が分かる文面になり_1_つなら今までどおり", async ({ page }) => {
    await open(page);
    await setMockReply(page, "paste_entries", { ok: { kind: "conflict", name: "a.md", total: 3 } });
    await chooseFromMenu(page, row(page, "docs"), "ペースト");

    const dialog = conflictDialog(page);
    await expect(dialog).toBeVisible();
    await expect(dialog.getByRole("heading")).toHaveText("「a.md」ほか 2 項目がすでにあります");
    await expect(dialog.locator("p")).toHaveText([
      "置き換えると、いまある 3 項目はゴミ箱に入ります。",
      "両方残すと、新しい方の名前に番号を付けます。",
    ]);
    // ボタンと既定のボタンは変わらない。
    await expect(dialog.getByRole("button")).toHaveText(["置き換える", "中止", "両方残す"]);
    await expect(dialog.getByRole("button", { name: "両方残す" })).toBeFocused();
    await dialog.getByRole("button", { name: "中止" }).click();
    await expect(dialog).toHaveCount(0);

    await setMockReply(page, "paste_entries", { ok: { kind: "conflict", name: "a.md", total: 1 } });
    await chooseFromMenu(page, row(page, "docs"), "ペースト");
    await expect(dialog.getByRole("heading")).toHaveText("「a.md」という名前の項目がすでにあります");
    await expect(dialog.locator("p")).toHaveText([
      "置き換えると、いまある「a.md」はゴミ箱に入ります。",
      "両方残すと、新しい方の名前に番号を付けます。",
    ]);
  });
});

test.describe("更新の保留(FR-037)", () => {
  const ADDED = listed([...ROOT_ENTRIES, ["zz-new.ts", "file"]]);

  test("メニューの表示中と入力中は届いた変更で行が動かない", async ({ page }) => {
    await open(page);
    const before = await rowPaths(page);

    // メニューの表示中。
    await rightClick(row(page, "a b.ts"));
    await setMockListing(page, listingKey(APP.id, ""), ADDED);
    await emitEvent(page, "tree-changed", { projectId: APP.id, dirs: [""], fullReload: false });
    await page.waitForTimeout(300);
    expect(await rowPaths(page)).toEqual(before);
    await page.keyboard.press("Escape");
    await expect(row(page, "zz-new.ts")).toBeVisible();

    // 入力中。
    await chooseFromMenu(page, row(page, "a b.ts"), "名称変更…");
    const editing = await rowPaths(page);
    await setMockListing(page, listingKey(APP.id, ""), ROOT);
    await emitEvent(page, "tree-changed", { projectId: APP.id, dirs: [""], fullReload: false });
    await page.waitForTimeout(300);
    expect(await rowPaths(page)).toEqual(editing);
    await expect(nameInput(page)).toBeFocused();
    await nameInput(page).press("Escape");
    await expect(row(page, "zz-new.ts")).toHaveCount(0);
  });
});

test.describe("コピー・切り取り・ペーストの振り分け(C-04、FR-027)", () => {
  test("ツリーにフォーカスがあるとコピー_切り取り_ペーストのイベントがツリーの操作になりターミナルに貼り付かない", async ({
    page,
  }) => {
    await open(page);
    await row(page, "README.md").click();
    await expect(fileTree(page)).toBeFocused();

    const copy = await fireClipboard(page, "copy");
    const cut = await fireClipboard(page, "cut");
    const paste = await fireClipboard(page, "paste", "echo pasted");
    await row(page, "src").click();
    await fireClipboard(page, "paste", "echo pasted");

    expect([copy.prevented, cut.prevented, paste.prevented]).toEqual([true, true, true]);
    await expect
      .poll(() => fileCommands(page))
      .toEqual(["copy_entries", "cut_entries", "paste_entries", "paste_entries"]);
    expect(await readCallArgs(page, "copy_entries")).toEqual([{ projectId: APP.id, paths: ["README.md"] }]);
    expect(await readCallArgs(page, "cut_entries")).toEqual([{ projectId: APP.id, paths: ["README.md"] }]);
    expect(await readCallArgs(page, "paste_entries")).toEqual([
      { projectId: APP.id, destDir: "" },
      { projectId: APP.id, destDir: "src" },
    ]);
    expect(await readCallArgs(page, "write_terminal")).toEqual([]);
  });

  test("ターミナルにフォーカスがあるときのコピーとペーストは今までどおり", async ({ page }) => {
    await open(page);
    await row(page, "README.md").click();
    await page.getByTestId("terminal-host").locator(".xterm-screen").click();
    await expect(page.locator(".xterm-helper-textarea")).toBeFocused();

    const paste = await fireClipboard(page, "paste", "ls");

    expect(paste.prevented).toBe(true);
    await expect.poll(async () => (await readCallArgs(page, "write_terminal")).map((w) => w.data).join("")).toBe("ls");
    expect(await readCallArgs(page, "paste_entries")).toEqual([]);
    expect(await readCallArgs(page, "copy_entries")).toEqual([]);
  });
});

test.describe("表示だけの状態の片付け(部品が消えたとき・プロジェクトが変わったとき)", () => {
  const LOG: SampleProject = { id: "p-log", name: "ribbitlog", color: "orange", displayPath: "~/dev/ribbitlog" };
  const many = (count: number): Entries =>
    Array.from({ length: count }, (_, i) => [`file-${String(i + 1).padStart(4, "0")}.txt`, "file"] as const);

  test("名称変更中にスクロールして行が窓から外れると入力を取り消し_ツリーの更新が止まらない", async ({ page }) => {
    const errors = collectPageErrors(page);
    await open(page, projectsSnapshot([APP]), appListings({ [listingKey(APP.id, "big")]: listed(many(500)) }));
    await setMockListing(page, listingKey(APP.id, ""), listed([["big", "directory"]]));
    await emitEvent(page, "tree-changed", { projectId: APP.id, dirs: [""], fullReload: false });
    await toggle(page, "big").click();
    await expect(row(page, "big/file-0001.txt")).toBeVisible();

    await chooseFromMenu(page, row(page, "big/file-0001.txt"), "名称変更…");
    await expect(nameInput(page)).toBeFocused();
    await scroller(page).evaluate((el) => {
      el.scrollTop = el.scrollHeight;
    });

    await expect(row(page, "big/file-0500.txt")).toBeVisible();
    await expect(nameInput(page)).toHaveCount(0);
    // 入力を取り消したので、保留が解けて外からの変更が出る。
    await setMockListing(page, listingKey(APP.id, ""), listed([["big", "directory"], ["zz-new.ts", "file"]]));
    await emitEvent(page, "tree-changed", { projectId: APP.id, dirs: [""], fullReload: false });
    await scroller(page).evaluate((el) => {
      el.scrollTop = el.scrollHeight;
    });
    await expect(row(page, "zz-new.ts")).toBeVisible();
    expect(await readCallArgs(page, "rename_entry")).toEqual([]);
    expect(errors).toEqual([]);
  });

  test("衝突の確認を出したまま見つからなくなり一覧から削除しても_別のプロジェクトに確認が出ない", async ({ page }) => {
    const snapshot = projectsSnapshot([APP, LOG]);
    await open(page, snapshot, { ...appListings(), [listingKey(LOG.id, "")]: listed([["Cargo.toml", "file"]]) });
    await setMockReply(page, "paste_entries", { ok: { kind: "conflict", name: "a.md", total: 1 } });
    await chooseFromMenu(page, row(page, "docs"), "ペースト");
    await expect(conflictDialog(page)).toBeVisible();

    // 確認の表示中にフォルダが見つからなくなる。
    await emitEvent(page, "app-snapshot", {
      ...snapshot,
      projects: snapshot.projects.map((p) => (p.id === APP.id ? { ...p, missing: true } : p)),
    });
    await expect(page.getByTestId("tree-missing")).toBeVisible();
    await expect(conflictDialog(page)).toHaveCount(0);

    // 一覧から削除すると、残ったプロジェクトが選ばれる。
    await emitEvent(page, "app-snapshot", {
      ...snapshot,
      projects: snapshot.projects.filter((p) => p.id !== APP.id),
      terminals: snapshot.terminals.filter((t) => t.projectId !== APP.id),
      selectedProjectId: LOG.id,
    });
    await expect(page.getByTestId("tree-header")).toHaveText("ribbitlog");
    await expect(row(page, "Cargo.toml")).toBeVisible();
    await page.waitForTimeout(200);
    await expect(conflictDialog(page)).toHaveCount(0);
    expect(await readCallArgs(page, "paste_entries")).toEqual([{ projectId: APP.id, destDir: "docs" }]);
  });

  test("ツリーにフォーカスがあるままプロジェクトが変わっても_ターミナルでのペーストはターミナルへ", async ({ page }) => {
    const snapshot = projectsSnapshot([APP, LOG]);
    await open(page, snapshot, { ...appListings(), [listingKey(LOG.id, "")]: listed([["Cargo.toml", "file"]]) });
    await row(page, "README.md").click();
    await expect(fileTree(page)).toBeFocused();

    // Finder からのドロップでの登録などで、画面の操作なしに選択中のプロジェクトが変わる。
    await emitEvent(page, "app-snapshot", { ...snapshot, selectedProjectId: LOG.id });
    await expect(row(page, "Cargo.toml")).toBeVisible();
    await page.getByTestId("terminal-host").locator(".xterm-screen").click();
    await expect(page.locator(".xterm-helper-textarea:focus")).toHaveCount(1);

    const paste = await fireClipboard(page, "paste", "ls");

    expect(paste.prevented).toBe(true);
    await expect.poll(async () => (await readCallArgs(page, "write_terminal")).map((w) => w.data).join("")).toBe("ls");
    expect(await readCallArgs(page, "paste_entries")).toEqual([]);
  });

  test("読めないフォルダを開くと知らせが出て_フォルダは閉じたまま", async ({ page }) => {
    await open(page, projectsSnapshot([APP]), appListings());
    await setMockListing(page, listingKey(APP.id, ""), listed([...ROOT_ENTRIES, ["secret", "directory"]]));
    await emitEvent(page, "tree-changed", { projectId: APP.id, dirs: [""], fullReload: false });
    await expect(row(page, "secret")).toBeVisible();
    // 表に無いフォルダの問い合わせは失敗する(権限の無いフォルダなど)。
    await setMockReply(page, "list_directory", { error: "Permission denied" });

    await toggle(page, "secret").click();

    await expect(notice(page)).toHaveText("フォルダを読めませんでした");
    await expect(row(page, "secret")).toHaveAttribute("aria-expanded", "false");
    await expect(row(page, "secret")).not.toHaveAttribute("aria-busy", "true");
    expect(await readCallArgs(page, "list_directory")).toContainEqual({ projectId: APP.id, path: "secret" });
  });
});

test.describe("「編集」メニューの有効化(beforecopy・beforecut・beforepaste)", () => {
  /** フォーカスのある要素へ beforecopy などを発火し、取り消されたかを返す。 */
  const fireBefore = (page: Page, type: "beforecopy" | "beforecut" | "beforepaste") =>
    page.evaluate((type) => {
      const event = new Event(type, { bubbles: true, cancelable: true });
      (document.activeElement ?? document.body).dispatchEvent(event);
      return event.defaultPrevented;
    }, type);

  test("ツリーにフォーカスがあり操作になるときだけ取り消す", async ({ page }) => {
    await open(page);
    await row(page, "README.md").click();
    await expect(fileTree(page)).toBeFocused();
    expect([
      await fireBefore(page, "beforecopy"),
      await fireBefore(page, "beforecut"),
      await fireBefore(page, "beforepaste"),
    ]).toEqual([true, true, true]);

    // ルートの切り取りは操作にならないので取り消さない。
    await header(page).click();
    expect(await fireBefore(page, "beforecut")).toBe(false);

    // ターミナルでは何もしない。
    await page.getByTestId("terminal-host").locator(".xterm-screen").click();
    await expect(page.locator(".xterm-helper-textarea")).toBeFocused();
    expect(await fireBefore(page, "beforecopy")).toBe(false);
    expect(await fileCommands(page)).toEqual([]);
  });
});
