// Markdown のエディタの基本の動き(M4.8。T83a〜T83c。ADR-015、AC-E01・E03・E04・E05・E07・E08・E10・E15)。
//
// 偽の Rust 側は写しを送り返さないので、コマンドが届いたことを確かめてから、Rust が返す写し(withEditors)を流す。
// エディタの部品(CodeMirror)は本物を動的 import で読む。シナリオの通し(T84)は e2e/m48-scenario.spec.ts の役目。
// 画像は testreport/e2e/m48/ に撮る(ライト・ダーク)。

import { expect, type Page, test } from "@playwright/test";

import type { AppSnapshot } from "../src/ipc/generated/AppSnapshot";
import type { EditorTabSnapshot } from "../src/ipc/generated/EditorTabSnapshot";
import {
  emitEvent,
  installTauriMock,
  listed,
  listingKey,
  projectsSnapshot,
  readCallArgs,
  readRecordedCalls,
  sampleEditorTab,
  type SampleProject,
  setMockReply,
  type TauriMockConfig,
  withEditors,
  withFontSizes,
} from "./fixtures/tauriMock";

const APP: SampleProject = { id: "p-app", name: "app", color: "green", displayPath: "~/dev/app" };
const EDITOR_ID = "editor-1";
const PATH = "docs/README.md";

const DOCUMENT = [
  "# 見出し",
  "",
  "本文の段落。**太字** と *強調* と ~~取り消し~~ と `code`。",
  "",
  "| 名前 | 数 |",
  "| :-- | --: |",
  "| りんご | 3 |",
  "",
  "- [x] 済んだこと",
  "- [ ] まだのこと",
  "",
  "> 引用の文",
  "",
  "[リンク](https://example.com) と ![図](figure.png) と <b>生の HTML</b>",
  "",
].join("\n");

const tab = (overrides: Partial<EditorTabSnapshot> = {}) => sampleEditorTab(EDITOR_ID, APP.id, PATH, overrides);

function collectPageErrors(page: Page): string[] {
  const errors: string[] = [];
  page.on("console", (message) => {
    if (message.type() === "error") errors.push(message.text());
  });
  page.on("pageerror", (error) => errors.push(error.message));
  return errors;
}

function shotPath(name: string): string {
  const configFile = test.info().config.configFile;
  if (configFile === undefined) throw new Error("Playwright の設定ファイルの場所が分からない");
  return `${configFile.slice(0, configFile.lastIndexOf("/"))}/testreport/e2e/m48/${name}.png`;
}

const base = (): AppSnapshot => projectsSnapshot([APP]);

/** 画面を開き、ツリーの README.md まで出す。 */
async function open(page: Page, overrides: Partial<TauriMockConfig> = {}): Promise<void> {
  await installTauriMock(
    page,
    {
      get_app_snapshot: { ok: base() },
      open_document: { ok: { kind: "opened", editorId: EDITOR_ID, text: DOCUMENT, lineEnding: "lf", readOnly: null } },
      ...overrides,
    },
    {
      [listingKey(APP.id, "")]: listed([["docs", "directory"]]),
      [listingKey(APP.id, "docs")]: listed([
        ["README.md", "file"],
        ["note.txt", "file"],
      ]),
    },
  );
  await page.goto("/");
  await expect
    .poll(async () => (await readRecordedCalls(page)).map((call) => call.command))
    .toContain("notify_app_ready");
  await page.locator('[data-testid="tree-row"][data-path="docs"]').dblclick();
  await expect(treeRow(page, PATH)).toBeVisible();
}

const treeRow = (page: Page, path: string) =>
  page.locator(`[data-testid="tree-row"][data-path=${JSON.stringify(path)}]`);
const panel = (page: Page) => page.getByTestId("editor-panel");
const content = (page: Page) => panel(page).locator(".cm-content");
const preview = (page: Page) => page.getByTestId("editor-preview");
const editorTab = (page: Page) => page.getByTestId("editor-tab-item");

/** ツリーで README.md を開き、Rust が返す写し(エディタを表示中)を流す。 */
async function openDocument(page: Page, snapshot: AppSnapshot = base()): Promise<AppSnapshot> {
  await treeRow(page, PATH).dblclick();
  await expect.poll(() => readCallArgs(page, "open_document")).toEqual([{ projectId: APP.id, path: PATH }]);
  const opened = withEditors(snapshot, APP.id, [tab()]);
  await emitEvent(page, "app-snapshot", opened);
  await expect(content(page)).toBeVisible();
  return opened;
}

test.describe("エディタのタブ(AC-E01・E03・E04)", () => {
  test("ツリーで_md_をダブルクリックするとエディタのタブが開きフォーカスがエディタにある", async ({ page }) => {
    const errors = collectPageErrors(page);
    await open(page);

    await openDocument(page);

    await expect(editorTab(page)).toHaveText("README.md");
    await expect(editorTab(page).getByRole("tab")).toHaveAttribute("aria-selected", "true");
    await expect(page.getByTestId("editor-path")).toHaveText(PATH);
    await expect.poll(() => content(page).evaluate((el) => el.contains(document.activeElement))).toBe(true);
    // ターミナルのタブは選ばれていない(置き場所も描かない)
    await expect(page.getByTestId("tab-item").getByRole("tab")).toHaveAttribute("aria-selected", "false");
    await expect(page.getByTestId("terminal-host")).toHaveCount(0);
    expect(errors).toEqual([]);
  });

  test("txt_の開くは既定のアプリで_md_の右クリックに既定のアプリで開くがある", async ({ page }) => {
    await open(page);

    await treeRow(page, "docs/note.txt").dblclick();
    await expect.poll(() => readCallArgs(page, "open_entry")).toEqual([{ projectId: APP.id, path: "docs/note.txt" }]);

    await treeRow(page, PATH).click({ button: "right" });
    await page.getByRole("menuitem", { name: "既定のアプリで開く" }).click();
    await expect
      .poll(() => readCallArgs(page, "open_entry"))
      .toEqual([
        { projectId: APP.id, path: "docs/note.txt" },
        { projectId: APP.id, path: PATH },
      ]);
    expect(await readCallArgs(page, "open_document")).toEqual([]);
  });

  test("切替で編集だけ_並べて_プレビューだけになる_プレビューに_a_href_と_img_が無い", async ({ page }) => {
    await open(page);
    await openDocument(page);
    const radio = (name: string) => page.getByTestId("editor-view-switch").getByRole("radio", { name });

    // 既定は並べて
    await expect(radio("並べて")).toHaveAttribute("aria-checked", "true");
    await expect(preview(page).getByRole("heading", { name: "見出し" })).toBeVisible();
    await expect(preview(page).locator("table")).toBeVisible();
    await expect(preview(page)).toContainText("☑");
    await expect(preview(page)).toContainText("<b>生の HTML</b>");
    expect(await preview(page).locator("a[href], img, iframe, script, input").count()).toBe(0);

    await radio("編集").click();
    await expect(preview(page)).toHaveCount(0);
    await expect(content(page)).toBeVisible();

    await radio("プレビュー").click();
    await expect(preview(page)).toBeVisible();
    await expect(content(page)).toBeHidden();

    await radio("並べて").click();
    await expect(preview(page)).toBeVisible();
    await expect(content(page)).toBeVisible();
  });

  test("打つとタブに未保存の印が付き_cmd_s_で_save_document_が呼ばれ消える", async ({ page }) => {
    await open(page);
    const opened = await openDocument(page);

    await page.keyboard.press("Meta+ArrowDown");
    await page.keyboard.type("追記");
    await expect
      .poll(() => readCallArgs(page, "set_document_dirty"))
      .toEqual([{ editorId: EDITOR_ID, dirty: true }]);
    await emitEvent(page, "app-snapshot", withEditors(opened, APP.id, [tab({ dirty: true })]));
    await expect(page.getByTestId("editor-tab-dirty")).toBeVisible();
    await expect(editorTab(page).getByRole("tab")).toHaveAccessibleName("README.md(未保存)");
    await expect(page.getByTestId("editor-save")).toBeEnabled();

    await page.keyboard.press("Meta+s");
    await expect
      .poll(() => readCallArgs(page, "save_document"))
      .toEqual([{ editorId: EDITOR_ID, text: `${DOCUMENT}追記`, mode: "normal" }]);
    // 文字にはならない
    await expect(content(page)).not.toContainText("追記s");
    await emitEvent(page, "app-snapshot", withEditors(opened, APP.id, [tab({ dirty: false })]));
    await expect(page.getByTestId("editor-tab-dirty")).toHaveCount(0);
    await expect(page.getByTestId("editor-save")).toBeDisabled();
  });

  test("ターミナルのタブを選ぶとエディタが隠れ戻すと文字が残る", async ({ page }) => {
    await open(page);
    const opened = await openDocument(page);
    await page.keyboard.press("Meta+ArrowDown");
    await page.keyboard.type("残る文字");

    await page.getByTestId("tab-item").getByRole("tab").click();
    await expect.poll(() => readCallArgs(page, "select_terminal")).toEqual([{ terminalId: `${APP.id}-t1` }]);
    await emitEvent(page, "app-snapshot", withEditors(opened, APP.id, [tab({ dirty: true })], null));
    await expect(panel(page)).toHaveCount(0);
    await expect(page.getByTestId("terminal-host")).toBeVisible();

    await editorTab(page).getByRole("tab").click();
    await expect.poll(() => readCallArgs(page, "select_editor")).toEqual([{ editorId: EDITOR_ID }]);
    await emitEvent(page, "app-snapshot", withEditors(opened, APP.id, [tab({ dirty: true })], EDITOR_ID));
    await expect(content(page)).toContainText("残る文字");
    // 表示に戻したので外で変わったかを確かめる
    await expect.poll(async () => (await readCallArgs(page, "check_document")).length).toBeGreaterThan(0);
  });

  test("ターミナルの文字の大きさの写しでエディタとプレビューの文字が変わる", async ({ page }) => {
    await open(page);
    const opened = await openDocument(page);
    const sizes = async () => ({
      editor: await panel(page).locator(".editor-surface").evaluate((el) => getComputedStyle(el).fontSize),
      preview: await preview(page).locator(".md-preview").evaluate((el) => getComputedStyle(el).fontSize),
    });
    expect(await sizes()).toEqual({ editor: "15px", preview: "15px" });

    await emitEvent(page, "app-snapshot", withFontSizes(opened, { terminal: 20, interface: 16 }));

    await expect.poll(sizes).toEqual({ editor: "20px", preview: "20px" });
  });

  test("読み取り専用の帯を出し保存を押せない", async ({ page }) => {
    await open(page, {
      open_document: { ok: { kind: "opened", editorId: EDITOR_ID, text: DOCUMENT, lineEnding: "lf", readOnly: "hard-link" } },
    });
    await treeRow(page, PATH).dblclick();
    await emitEvent(page, "app-snapshot", withEditors(base(), APP.id, [tab({ readOnly: "hard-link" })]));

    await expect(page.getByTestId("editor-read-only")).toContainText("読み取り専用");
    await expect(page.getByTestId("editor-save")).toBeDisabled();
  });
});

test.describe("確認と設定画面(AC-E05・E07・E08)", () => {
  test("未保存のタブの閉じるで確認が出て保存しないで閉じる", async ({ page }) => {
    await open(page, { close_editor: { ok: { kind: "needs-confirmation" } } });
    const opened = await openDocument(page);
    await emitEvent(page, "app-snapshot", withEditors(opened, APP.id, [tab({ dirty: true })]));

    await editorTab(page).getByRole("button", { name: "「README.md」を閉じる" }).click();
    const dialog = page.getByTestId("unsaved-close-dialog");
    await expect(dialog).toBeVisible();
    await expect(dialog).toContainText("「README.md」の変更を保存しますか?");
    await setMockReply(page, "close_editor", { ok: { kind: "closed" } });
    await dialog.getByRole("button", { name: "保存しない" }).click();

    await expect(dialog).toHaveCount(0);
    await expect
      .poll(() => readCallArgs(page, "close_editor"))
      .toEqual([
        { editorId: EDITOR_ID, discard: false },
        { editorId: EDITOR_ID, discard: true },
      ]);
  });

  test("editor_close_requested_で同じ確認が出て_esc_で閉じるとエディタにフォーカスが戻る", async ({ page }) => {
    await open(page);
    await openDocument(page);

    await emitEvent(page, "editor-close-requested", { editorId: EDITOR_ID });
    const dialog = page.getByTestId("unsaved-close-dialog");
    await expect(dialog).toBeVisible();
    await page.keyboard.press("Escape");

    await expect(dialog).toHaveCount(0);
    await expect.poll(() => content(page).evaluate((el) => el.contains(document.activeElement))).toBe(true);
    expect(await readCallArgs(page, "close_editor")).toEqual([]);
  });

  test("changed_outside_で衝突の確認が出て_キャンセルでは何も呼ばず_上書き保存は_mode_overwrite", async ({
    page,
  }) => {
    await open(page, { save_document: { ok: { kind: "changed-outside" } } });
    await openDocument(page);
    await page.keyboard.type("x");

    await page.keyboard.press("Meta+s");
    const dialog = page.getByTestId("save-conflict-dialog");
    await expect(dialog).toBeVisible();
    await expect(dialog.getByRole("button", { name: "キャンセル" })).toBeFocused();
    await dialog.getByRole("button", { name: "キャンセル" }).click();
    await expect(dialog).toHaveCount(0);
    expect(await readCallArgs(page, "save_document")).toHaveLength(1);

    await page.keyboard.press("Meta+s");
    await expect(dialog).toBeVisible();
    await setMockReply(page, "save_document", { ok: { kind: "saved" } });
    await dialog.getByRole("button", { name: "上書き保存" }).click();
    await expect
      .poll(async () => (await readCallArgs(page, "save_document")).map((a) => a.mode))
      .toEqual(["normal", "normal", "overwrite"]);
  });

  test("未保存があると終了の確認に文書の名前が並ぶ", async ({ page }) => {
    await open(page);

    await emitEvent(page, "quit-requested", {
      running: [],
      unsaved: [{ editorId: EDITOR_ID, projectName: APP.name, path: PATH }],
    });

    const dialog = page.getByTestId("quit-dialog");
    await expect(dialog).toBeVisible();
    await expect(page.getByTestId("quit-unsaved-list")).toHaveText(`${APP.name}: ${PATH}`);
    await expect(dialog).not.toContainText("動作中のターミナルがあります");
  });

  test("vim_の切替で_set_editor_vim_が呼ばれ届いた写しで_vim_になる_ターミナルとエディタの行の名前", async ({ page }) => {
    await open(page);
    await emitEvent(page, "menu-command", "open-settings");
    const sheet = page.getByTestId("settings-dialog");
    await expect(sheet).toBeVisible();
    await expect(page.getByTestId("font-size-row-terminal")).toContainText("ターミナルとエディタ");
    const toggle = page.getByTestId("editor-vim-switch");
    await expect(toggle).toHaveAttribute("aria-checked", "false");
    await expect(toggle).toHaveAccessibleName("Vim のキー操作");

    await toggle.click();
    await expect.poll(() => readCallArgs(page, "set_editor_vim")).toEqual([{ enabled: true }]);
    await emitEvent(page, "app-snapshot", { ...base(), editor: { vim: true } });
    await expect(toggle).toHaveAttribute("aria-checked", "true");
  });
});

test.describe("見た目(AC-E15)", () => {
  for (const colorScheme of ["light", "dark"] as const) {
    test(`エディタ_並べて表示_確認_設定画面の画像を撮る_${colorScheme}`, async ({ page }) => {
      await page.setViewportSize({ width: 1200, height: 720 });
      await page.emulateMedia({ colorScheme });
      await open(page, { close_editor: { ok: { kind: "needs-confirmation" } } });
      const opened = await openDocument(page);
      const dirty = withEditors(opened, APP.id, [tab({ dirty: true })]);
      await emitEvent(page, "app-snapshot", dirty);
      await expect(preview(page).getByRole("heading", { name: "見出し" })).toBeVisible();
      await page.screenshot({ path: shotPath(`split-${colorScheme}`) });

      await page.getByTestId("editor-view-switch").getByRole("radio", { name: "編集" }).click();
      await page.screenshot({ path: shotPath(`edit-${colorScheme}`) });
      await page.getByTestId("editor-view-switch").getByRole("radio", { name: "プレビュー" }).click();
      await page.screenshot({ path: shotPath(`preview-${colorScheme}`) });

      await editorTab(page).getByRole("button", { name: "「README.md」を閉じる" }).click();
      await expect(page.getByTestId("unsaved-close-dialog")).toBeVisible();
      await page.screenshot({ path: shotPath(`unsaved-close-${colorScheme}`) });
      await page.keyboard.press("Escape");

      await emitEvent(page, "menu-command", "open-settings");
      await expect(page.getByTestId("settings-dialog")).toBeVisible();
      await page.screenshot({ path: shotPath(`settings-${colorScheme}`) });
    });
  }
});
