// ツリーのドラッグ(T43d): ツリー内の移動・コピー(FR-029)、ターミナルへのパスの挿入(FR-016)、ドラッグ中の更新の保留(FR-037)。
//
// ドラッグは HTML 標準のドラッグ&ドロップではなく pointer のイベントで自前に作っている(ADR-007 の (b))ので、
// Playwright の page.mouse の down・move・up でそのまま再現できる。⌥ は page.keyboard.down("Alt") で押したままにする
// (Playwright はキーボードで押している修飾キーをマウスのイベントにも付ける)。
// 偽の Rust 側(fixtures/tauriMock.ts)のフォルダごとの応答を置いて画面を開き、呼ばれたコマンドと引数・行の並びを見る。
//
// Playwright の WebKit は macOS 標準の WKWebView とは別のビルドなので、実際のマウスとトラックパッドでの手触りは
// 本人が T45 で確かめる(ADR-007 の (b))。

import { expect, type Locator, type Page, test } from "@playwright/test";

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
  ["a.md", "file"],
];
const SRC_ENTRIES: Entries = [
  ["lib", "directory"],
  ["main.ts", "file"],
];

function appListings(): Record<string, MockListing> {
  return {
    [listingKey(APP.id, "")]: listed(ROOT_ENTRIES),
    [listingKey(APP.id, "src")]: listed(SRC_ENTRIES),
    [listingKey(APP.id, "src/lib")]: listed([["util.ts", "file"]]),
    [listingKey(APP.id, "docs")]: listed([["a.md", "file"]]),
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

async function open(page: Page): Promise<void> {
  await installTauriMock(page, { get_app_snapshot: { ok: projectsSnapshot([APP]) } }, appListings());
  await page.goto("/");
  await expect
    .poll(async () => (await readRecordedCalls(page)).map((call) => call.command))
    .toContain("notify_app_ready");
  await expect(row(page, "README.md")).toBeVisible();
  await expect(terminalHost(page).locator(".xterm-screen")).toBeVisible();
}

const rows = (page: Page) => page.getByTestId("tree-row");
const row = (page: Page, path: string) => page.locator(`[data-testid="tree-row"][data-path=${JSON.stringify(path)}]`);
const toggle = (page: Page, path: string) => row(page, path).getByTestId("tree-row-toggle");
const header = (page: Page) => page.getByTestId("tree-header");
const terminalHost = (page: Page) => page.getByTestId("terminal-host");
const ghost = (page: Page) => page.getByTestId("tree-drag-ghost");
const copyBadge = (page: Page) => page.getByTestId("tree-drag-copy-badge");
const moveDialog = (page: Page) => page.getByTestId("move-confirm-dialog");
const conflictDialog = (page: Page) => page.getByTestId("tree-conflict-dialog");
/** 落とせる先として強調している要素(フォルダの行・ヘッダ・ターミナルの置き場所)。 */
const dropActive = (page: Page) => page.locator('[data-drop-active="true"]');

async function rowPaths(page: Page): Promise<string[]> {
  return rows(page).evaluateAll((elements) => elements.map((el) => el.getAttribute("data-path") ?? ""));
}

/** ファイル操作のコマンドの名前を、届いた順に返す。 */
async function fileCommands(page: Page): Promise<string[]> {
  const ops = new Set(["move_entry", "copy_entry", "insert_entry_path", "write_terminal", "write_terminal_bytes"]);
  return (await readRecordedCalls(page)).map((call) => call.command).filter((command) => ops.has(command));
}

async function centerOf(locator: Locator): Promise<{ x: number; y: number }> {
  const box = await locator.boundingBox();
  if (box === null) throw new Error("要素の大きさが取れない");
  return { x: box.x + box.width / 2, y: box.y + box.height / 2 };
}

/** 項目の名前の上で押し、しきい値(4px)を超えて動かしてドラッグを始める。 */
async function startDrag(page: Page, source: Locator): Promise<void> {
  const from = await centerOf(source.locator(".tree-row__name"));
  await page.mouse.move(from.x, from.y);
  await page.mouse.down();
  await page.mouse.move(from.x + 6, from.y + 6, { steps: 3 });
  await expect(ghost(page)).toBeVisible();
}

/** ドラッグ中に target の上へ動かす。 */
async function hover(page: Page, target: Locator): Promise<void> {
  const to = await centerOf(target);
  await page.mouse.move(to.x, to.y, { steps: 4 });
}

/** source を target へドラッグして離す。 */
async function dragAndDrop(page: Page, source: Locator, target: Locator): Promise<void> {
  await startDrag(page, source);
  await hover(page, target);
  await page.mouse.up();
}

test.describe("ターミナルへの挿入(FR-016)", () => {
  test("ツリーの項目をターミナルへ落とすと挿入のコマンドが呼ばれる", async ({ page }) => {
    const errors = collectPageErrors(page);
    await open(page);

    await startDrag(page, row(page, "a b.ts"));
    // xterm.js の要素(React の管理の外)の上でも、置き場所を落とし先と判定する。
    await hover(page, terminalHost(page).locator(".xterm-screen"));
    await expect(terminalHost(page)).toHaveAttribute("data-drop-active", "true");
    await page.mouse.up();

    await expect
      .poll(() => readCallArgs(page, "insert_entry_path"))
      .toEqual([{ projectId: APP.id, paths: ["a b.ts"], terminalId: TERMINAL_ID }]);
    // Enter を含め、画面からターミナルへは何も書かない(挿入の文字列は Rust が作って書く。AC-T04)。
    expect(await fileCommands(page)).toEqual(["insert_entry_path"]);
    // 続けて入力できるように、ターミナルにフォーカスが移る。
    await expect(page.locator(".xterm-helper-textarea")).toBeFocused();
    await expect(ghost(page)).toHaveCount(0);
    await expect(dropActive(page)).toHaveCount(0);
    expect(errors).toEqual([]);
  });
});

test.describe("ツリー内の移動・コピー(FR-029)", () => {
  test("ほかのフォルダへ落とすと移動の確認が出て中止なら何も呼ばない", async ({ page }) => {
    const errors = collectPageErrors(page);
    await open(page);

    await startDrag(page, row(page, "README.md"));
    await hover(page, row(page, "docs"));
    await expect(row(page, "docs")).toHaveAttribute("data-drop-active", "true");
    await expect(dropActive(page)).toHaveCount(1);
    await page.mouse.up();

    const dialog = moveDialog(page);
    await expect(dialog).toBeVisible();
    await expect(dialog).toHaveAttribute("role", "alertdialog");
    await expect(dialog.getByRole("heading")).toHaveText("「README.md」を「docs」に移動しますか?");
    await expect(dialog.getByRole("button")).toHaveText(["中止", "移動"]);
    // 誤って移さないよう、開いたときのフォーカスは「中止」。
    await expect(dialog.getByRole("button", { name: "中止" })).toBeFocused();
    await expect(ghost(page)).toHaveCount(0);
    await expect(dropActive(page)).toHaveCount(0);

    await dialog.getByRole("button", { name: "中止" }).click();
    await expect(dialog).toHaveCount(0);

    // Esc でも中止になる。
    await dragAndDrop(page, row(page, "README.md"), row(page, "docs"));
    await expect(dialog).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(dialog).toHaveCount(0);

    await page.waitForTimeout(200);
    expect(await fileCommands(page)).toEqual([]);
    expect(await rowPaths(page)).toContain("README.md");
    expect(errors).toEqual([]);
  });

  test("確認すると移動のコマンドが呼ばれる", async ({ page }) => {
    await open(page);

    await dragAndDrop(page, row(page, "README.md"), row(page, "docs"));
    await moveDialog(page).getByRole("button", { name: "移動" }).click();
    await expect(moveDialog(page)).toHaveCount(0);

    // ファイルの行に落とすとその親(ここではルートの直下のファイルなので、ルート)。ヘッダはルート。
    await toggle(page, "src").click();
    await expect(row(page, "src/main.ts")).toBeVisible();
    await startDrag(page, row(page, "src/main.ts"));
    await hover(page, header(page));
    await expect(header(page)).toHaveAttribute("data-drop-active", "true");
    await page.mouse.up();
    await expect(moveDialog(page).getByRole("heading")).toHaveText("「main.ts」を「app」に移動しますか?");
    await moveDialog(page).getByRole("button", { name: "移動" }).click();

    await expect
      .poll(() => readCallArgs(page, "move_entry"))
      .toEqual([
        { projectId: APP.id, source: "README.md", destDir: "docs" },
        { projectId: APP.id, source: "src/main.ts", destDir: "" },
      ]);
    expect(await readCallArgs(page, "copy_entry")).toEqual([]);
  });

  test("ファイルの行の上ではその親のフォルダを強調する", async ({ page }) => {
    await open(page);
    await toggle(page, "src").click();
    await expect(row(page, "src/main.ts")).toBeVisible();

    await startDrag(page, row(page, "README.md"));
    await hover(page, row(page, "src/main.ts"));
    await expect(row(page, "src")).toHaveAttribute("data-drop-active", "true");
    await expect(dropActive(page)).toHaveCount(1);
    await page.mouse.up();

    await expect(moveDialog(page).getByRole("heading")).toHaveText("「README.md」を「src」に移動しますか?");
  });

  test("option_を押して落とすとコピーのコマンドが呼ばれる", async ({ page }) => {
    await open(page);

    await startDrag(page, row(page, "README.md"));
    await hover(page, row(page, "docs"));
    await expect(copyBadge(page)).toHaveCount(0);
    await page.keyboard.down("Alt");
    // ⌥ を押している間は、写しに「+」の印が付く(動かさなくても)。
    await expect(copyBadge(page)).toBeVisible();
    await page.mouse.up();
    await page.keyboard.up("Alt");

    await expect
      .poll(() => readCallArgs(page, "copy_entry"))
      .toEqual([{ projectId: APP.id, source: "README.md", destDir: "docs" }]);
    // コピーは確認を出さない(計画【仮定】12)。
    await expect(moveDialog(page)).toHaveCount(0);
    expect(await readCallArgs(page, "move_entry")).toEqual([]);
  });

  test("落とした先に同名があると衝突の確認が出る", async ({ page }) => {
    await open(page);
    const conflict = { ok: { kind: "conflict", name: "a.md" } } as const;

    // 移動: 確認 → 衝突の確認(AC-T18)。中止ではどちらも変わらない(呼び直さない)。
    await setMockReply(page, "move_entry", conflict);
    await dragAndDrop(page, row(page, "a.md"), row(page, "docs"));
    await moveDialog(page).getByRole("button", { name: "移動" }).click();
    await expect(conflictDialog(page)).toBeVisible();
    await expect(conflictDialog(page).getByRole("heading")).toHaveText("「a.md」という名前の項目がすでにあります");
    await conflictDialog(page).getByRole("button", { name: "中止" }).click();
    await expect(conflictDialog(page)).toHaveCount(0);

    // ⌥ のコピーも同じ経路。
    await setMockReply(page, "copy_entry", conflict);
    await page.keyboard.down("Alt");
    await dragAndDrop(page, row(page, "a.md"), row(page, "docs"));
    await page.keyboard.up("Alt");
    await expect(conflictDialog(page)).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(conflictDialog(page)).toHaveCount(0);

    await page.waitForTimeout(200);
    expect(await readCallArgs(page, "move_entry")).toEqual([{ projectId: APP.id, source: "a.md", destDir: "docs" }]);
    expect(await readCallArgs(page, "copy_entry")).toEqual([{ projectId: APP.id, source: "a.md", destDir: "docs" }]);
  });

  test("自分の子孫へは落とせない", async ({ page }) => {
    await open(page);
    await toggle(page, "src").click();
    await toggle(page, "src/lib").click();
    await expect(row(page, "src/lib/util.ts")).toBeVisible();

    await startDrag(page, row(page, "src"));
    // 自分の子孫のフォルダ・子孫の中のファイル・自分自身の上では強調しない。
    for (const path of ["src/lib", "src/lib/util.ts", "src"]) {
      await hover(page, row(page, path));
      await expect(dropActive(page)).toHaveCount(0);
    }
    await page.mouse.up();

    await expect(ghost(page)).toHaveCount(0);
    await page.waitForTimeout(200);
    await expect(moveDialog(page)).toHaveCount(0);
    expect(await fileCommands(page)).toEqual([]);
  });

  test("同じフォルダや落とせない所では強調せず何も呼ばない", async ({ page }) => {
    await open(page);

    await startDrag(page, row(page, "README.md"));
    // 同じ親(ルート)のファイルの上・ヘッダ(いまの親)の上。
    await hover(page, row(page, "a b.ts"));
    await expect(dropActive(page)).toHaveCount(0);
    await hover(page, header(page));
    await expect(dropActive(page)).toHaveCount(0);
    // ツリーの外(カードの一覧)。
    await hover(page, page.getByTestId("project-card").first());
    await expect(dropActive(page)).toHaveCount(0);
    await page.mouse.up();

    await page.waitForTimeout(200);
    await expect(moveDialog(page)).toHaveCount(0);
    expect(await fileCommands(page)).toEqual([]);
  });
});

test.describe("ドラッグ中の更新の保留(FR-037)", () => {
  test("ドラッグ中に_100_件の変更が届いても行が動かず落としたあとに反映される", async ({ page }) => {
    await open(page);
    await toggle(page, "src").click();
    await expect(row(page, "src/main.ts")).toBeVisible();
    const before = await rowPaths(page);

    await startDrag(page, row(page, "src/main.ts"));
    await hover(page, row(page, "docs"));
    // 外から src に 100 ファイルが足された(AC-T25)。
    const added: Entries = Array.from({ length: 100 }, (_, i) => [`new-${String(i).padStart(3, "0")}.ts`, "file"] as const);
    await setMockListing(page, listingKey(APP.id, "src"), listed([["lib", "directory"], ...added, ["main.ts", "file"]]));
    await emitEvent(page, "tree-changed", { projectId: APP.id, dirs: ["src"], fullReload: false });
    await page.waitForTimeout(300);
    expect(await rowPaths(page)).toEqual(before);
    await expect(row(page, "docs")).toHaveAttribute("data-drop-active", "true");

    // ターミナルへ落とす(移動の確認も保留の理由なので、確認の出ない落とし先にする)。
    await hover(page, terminalHost(page));
    await page.mouse.up();

    await expect(row(page, "src/new-000.ts")).toBeVisible();
    await expect(row(page, "src/new-005.ts")).toBeVisible();
  });
});

test.describe("取り消しとクリック", () => {
  test("esc_で取り消す", async ({ page }) => {
    await open(page);

    await startDrag(page, row(page, "README.md"));
    await hover(page, row(page, "docs"));
    await expect(dropActive(page)).toHaveCount(1);
    await page.keyboard.press("Escape");

    // 写し・強調が消え、離しても何も起きない。
    await expect(ghost(page)).toHaveCount(0);
    await expect(dropActive(page)).toHaveCount(0);
    await page.mouse.up();
    await page.waitForTimeout(200);
    await expect(moveDialog(page)).toHaveCount(0);
    expect(await fileCommands(page)).toEqual([]);
    // 離したときの click で選択を外さない(ドラッグを始めたときに選んだ項目のまま)。
    await expect(row(page, "README.md")).toHaveAttribute("aria-selected", "true");

    // 保留も解けている(届いた変更がすぐ反映される)。
    await setMockListing(page, listingKey(APP.id, ""), listed([...ROOT_ENTRIES, ["zz-new.ts", "file"]]));
    await emitEvent(page, "tree-changed", { projectId: APP.id, dirs: [""], fullReload: false });
    await expect(row(page, "zz-new.ts")).toBeVisible();
  });

  test("フォーカスを失うか_pointercancel_で取り消して状態を戻す", async ({ page }) => {
    await open(page);

    await startDrag(page, row(page, "README.md"));
    await hover(page, row(page, "docs"));
    await page.evaluate(() => window.dispatchEvent(new Event("blur")));
    await expect(ghost(page)).toHaveCount(0);
    await expect(dropActive(page)).toHaveCount(0);
    await page.mouse.up();

    await startDrag(page, row(page, "README.md"));
    await hover(page, terminalHost(page));
    await expect(terminalHost(page)).toHaveAttribute("data-drop-active", "true");
    await page.evaluate(() =>
      document
        .querySelector('[data-testid="file-tree"]')
        ?.dispatchEvent(new PointerEvent("pointercancel", { pointerId: 1, bubbles: true })),
    );
    await expect(ghost(page)).toHaveCount(0);
    await expect(dropActive(page)).toHaveCount(0);
    await page.mouse.up();

    await page.waitForTimeout(200);
    await expect(moveDialog(page)).toHaveCount(0);
    expect(await fileCommands(page)).toEqual([]);
    // 保留も解けている。
    await setMockListing(page, listingKey(APP.id, ""), listed([...ROOT_ENTRIES, ["zz-new.ts", "file"]]));
    await emitEvent(page, "tree-changed", { projectId: APP.id, dirs: [""], fullReload: false });
    await expect(row(page, "zz-new.ts")).toBeVisible();
  });

  test("小さな動きはクリックのまま", async ({ page }) => {
    await open(page);
    const from = await centerOf(row(page, "README.md").locator(".tree-row__name"));

    await page.mouse.move(from.x, from.y);
    await page.mouse.down();
    await page.mouse.move(from.x + 3, from.y + 2, { steps: 2 });
    await expect(ghost(page)).toHaveCount(0);
    await page.mouse.up();

    await expect(row(page, "README.md")).toHaveAttribute("aria-selected", "true");
    // ダブルクリックでフォルダの開閉も今までどおり。
    await row(page, "src").dblclick();
    await expect(row(page, "src/main.ts")).toBeVisible();
    expect(await fileCommands(page)).toEqual([]);
  });

  test("ドラッグのあとのクリックで選択を外さず_ドラッグした項目が選ばれる", async ({ page }) => {
    await open(page);
    await row(page, "a b.ts").click();

    await startDrag(page, row(page, "README.md"));
    await hover(page, row(page, "a b.ts"));
    await page.mouse.up();

    await expect(row(page, "README.md")).toHaveAttribute("aria-selected", "true");
    await expect(row(page, "a b.ts")).toHaveAttribute("aria-selected", "false");
  });

  test("名前の入力中の行はドラッグを始めない", async ({ page }) => {
    await open(page);
    await row(page, "README.md").click({ button: "right" });
    await page.getByTestId("context-menu").getByText("名称変更…").click();
    const input = page.getByTestId("tree-name-input");
    await expect(input).toBeFocused();

    const from = await centerOf(input);
    await page.mouse.move(from.x, from.y);
    await page.mouse.down();
    await page.mouse.move(from.x + 40, from.y + 30, { steps: 4 });
    await expect(ghost(page)).toHaveCount(0);
    await page.mouse.up();
    await expect(input).toBeVisible();
  });
});
