// M4.8 のシナリオの画面の自動テスト(T84。ADR-015 の AC-E の通し)。
//
// 1 人の利用者が Markdown の文書を開いて書き、保存し、外のアプリとぶつかり、閉じる・終了するまでを、機能をまたいだ流れで通す。
// 基本の動き(開く・切替・⌘S・衝突の確認のキャンセルと上書き・保存しない・Esc・読み取り専用の帯・文字の大きさ・画像)は
// e2e/editor.spec.ts が見るので、ここでは繰り返さない。
//
// 偽の Rust 側(fixtures/tauriMock.ts)は写しを送り返さないので、コマンドが届いたことを確かめてから、Rust が返す写し
// (withEditors・withTileDisplay)を流す。外での変更は check_document の応答を setMockReply で替えてから tree-changed を流す
// (選択中のプロジェクト)か、ウィンドウの focus を起こす(ウィンドウが前面に戻った)。
// エディタの部品(CodeMirror)は本物を動的 import で読む。読み込みの要求はページの request で数える(Vite の開発サーバーは
// モジュールを 1 つずつ配るので、エディタのモジュールの要求が出たかで「読んだか」が分かる)。

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
  sampleRunningTerminal,
  type SampleProject,
  setMockReply,
  type TauriMockConfig,
  withEditors,
  withTileDisplay,
} from "./fixtures/tauriMock";

const APP: SampleProject = { id: "p-app", name: "app", color: "green", displayPath: "~/dev/app" };
const LOG: SampleProject = { id: "p-log", name: "ribbitlog", color: "orange", displayPath: "~/dev/ribbitlog" };
const EDITOR_ID = "editor-1";
const PATH = "docs/README.md";
const TERMINAL_ID = `${APP.id}-t1`;

const DOCUMENT = "# 題\n\n本文\n";

const tab = (overrides: Partial<EditorTabSnapshot> = {}) => sampleEditorTab(EDITOR_ID, APP.id, PATH, overrides);
const base = (): AppSnapshot => projectsSnapshot([APP, LOG]);

/** エディタのモジュール(CodeMirror・lezer・Vim と、それを組み立てる src/editor/codemirror/)の要求か。 */
const isEditorModule = (url: string) => /codemirror|@lezer|lezer_|@replit|replit_/i.test(url);

/** 画面を開き、ツリーの docs を開いて README.md まで出す。 */
async function open(page: Page, overrides: Partial<TauriMockConfig> = {}, snapshot: AppSnapshot = base()): Promise<void> {
  await installTauriMock(
    page,
    {
      get_app_snapshot: { ok: snapshot },
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
  await treeRow(page, "docs").dblclick();
  await expect(treeRow(page, PATH)).toBeVisible();
}

const treeRow = (page: Page, path: string) =>
  page.locator(`[data-testid="tree-row"][data-path=${JSON.stringify(path)}]`);
const panel = (page: Page) => page.getByTestId("editor-panel");
const content = (page: Page) => panel(page).locator(".cm-content");
const lines = (page: Page) => content(page).locator(".cm-line").allTextContents();
const banner = (page: Page) => page.getByTestId("editor-banner");
const editorTab = (page: Page) => page.getByTestId("editor-tab-item");
const editorHasFocus = (page: Page) => content(page).evaluate((el) => el.contains(document.activeElement));

/** ツリーで README.md を開き、Rust が返す写し(エディタを表示中)を流す。 */
async function openDocument(page: Page, snapshot: AppSnapshot = base()): Promise<AppSnapshot> {
  await treeRow(page, PATH).dblclick();
  await expect.poll(() => readCallArgs(page, "open_document")).toEqual([{ projectId: APP.id, path: PATH }]);
  const opened = withEditors(snapshot, APP.id, [tab()]);
  await emitEvent(page, "app-snapshot", opened);
  await expect(content(page)).toBeVisible();
  await expect.poll(() => editorHasFocus(page)).toBe(true);
  return opened;
}

/** 文書の末尾へ移って打つ。 */
async function typeAtEnd(page: Page, text: string): Promise<void> {
  await page.keyboard.press("Meta+ArrowDown");
  await page.keyboard.type(text);
}

/** 選択中のプロジェクトの docs が外で変わった(本物の Rust の監視の知らせ)。 */
const docsChanged = (page: Page) => emitEvent(page, "tree-changed", { projectId: APP.id, dirs: ["docs"], fullReload: false });

// 各テストの終わりに、画面の失敗(例外、通信の失敗の報告)が無かったことを見る(AC-E13 の [arin:frontend-error] の元)。
test.afterEach(async ({ page }) => {
  const reported = await readCallArgs(page, "report_frontend_error");
  expect(reported.map((r) => r.message), "画面の失敗が Rust へ報告された").toEqual([]);
});

test.describe("書いて保存し外の変更とぶつかる(AC-E04・E05・E06・E13)", () => {
  test("開くまでエディタの_JS_を読まず_保存_外の変更の読み直し_未保存なら帯_衝突で読み直すを通す", async ({ page }) => {
    const requested: string[] = [];
    page.on("request", (request) => requested.push(request.url()));
    await open(page);

    // --- 起動とツリーだけではエディタの JS を読まない(AC-E13) ---
    expect(requested.filter(isEditorModule)).toEqual([]);
    const opened = await openDocument(page);
    expect(requested.filter(isEditorModule).length).toBeGreaterThan(0);

    // --- 打つ → 未保存の印 → ⌘S → 印が消える(AC-E04) ---
    await typeAtEnd(page, "追記");
    await expect.poll(() => readCallArgs(page, "set_document_dirty")).toEqual([{ editorId: EDITOR_ID, dirty: true }]);
    await emitEvent(page, "app-snapshot", withEditors(opened, APP.id, [tab({ dirty: true })]));
    await expect(page.getByTestId("editor-tab-dirty")).toBeVisible();
    await page.keyboard.press("Meta+s");
    await expect
      .poll(() => readCallArgs(page, "save_document"))
      .toEqual([{ editorId: EDITOR_ID, text: `${DOCUMENT}追記`, overwriteChanged: false }]);
    await emitEvent(page, "app-snapshot", withEditors(opened, APP.id, [tab({ dirty: false })]));
    await expect(page.getByTestId("editor-tab-dirty")).toHaveCount(0);

    // --- 未保存が無いときに外で変わった → 本文ごと受け取って読み直す(AC-E06) ---
    const dirtyBefore = (await readCallArgs(page, "set_document_dirty")).length;
    await setMockReply(page, "check_document", { ok: { kind: "changed", text: "# 外で書いた\n\n本文\n", lineEnding: "lf" } });
    await docsChanged(page);
    await expect
      .poll(async () => (await readCallArgs(page, "check_document")).at(-1))
      .toEqual({ editorId: EDITOR_ID, withText: true });
    await expect.poll(() => lines(page)).toEqual(["# 外で書いた", "", "本文", ""]);
    await expect(banner(page)).toHaveCount(0);
    // 読み直しは未保存にしない(未保存の印を送らない)
    expect((await readCallArgs(page, "set_document_dirty")).slice(dirtyBefore)).toEqual([]);

    // --- 未保存があるときに外で変わった → 読み直さず帯で知らせる(AC-E06) ---
    await typeAtEnd(page, "下書き");
    await expect
      .poll(async () => (await readCallArgs(page, "set_document_dirty")).at(-1))
      .toEqual({ editorId: EDITOR_ID, dirty: true });
    await emitEvent(page, "app-snapshot", withEditors(opened, APP.id, [tab({ dirty: true })]));
    await setMockReply(page, "check_document", { ok: { kind: "changed", text: null, lineEnding: "lf" } });
    const checksBefore = (await readCallArgs(page, "check_document")).length;
    await docsChanged(page);
    await expect
      .poll(async () => (await readCallArgs(page, "check_document")).slice(checksBefore))
      .toEqual([{ editorId: EDITOR_ID, withText: false }]);
    await expect(banner(page)).toHaveAttribute("data-kind", "changed");
    await expect(banner(page)).toContainText("ほかのアプリで変更されました");
    expect(await lines(page)).toEqual(["# 外で書いた", "", "本文", "下書き"]);

    // --- 保存すると上書きせず確認 → 「読み直す」で外の中身に替わり未保存が消える(AC-E05) ---
    await setMockReply(page, "save_document", { ok: { kind: "changed-outside" } });
    await page.keyboard.press("Meta+s");
    const dialog = page.getByTestId("save-conflict-dialog");
    await expect(dialog).toBeVisible();
    await expect(dialog).toContainText("「README.md」はほかのアプリで変更されています");
    await setMockReply(page, "check_document", { ok: { kind: "changed", text: "# 外の最新\n", lineEnding: "lf" } });
    await dialog.getByRole("button", { name: "読み直す" }).click();

    await expect(dialog).toHaveCount(0);
    await expect
      .poll(async () => (await readCallArgs(page, "check_document")).at(-1))
      .toEqual({ editorId: EDITOR_ID, withText: true });
    await expect.poll(() => lines(page)).toEqual(["# 外の最新", ""]);
    await expect
      .poll(async () => (await readCallArgs(page, "set_document_dirty")).at(-1))
      .toEqual({ editorId: EDITOR_ID, dirty: false });
    await expect(banner(page)).toHaveCount(0);
    expect((await readCallArgs(page, "save_document")).map((a) => a.overwriteChanged)).toEqual([false, false]);
    await expect.poll(() => editorHasFocus(page)).toBe(true);
  });

  test("ウィンドウが前面に戻ると確かめ_消えていれば帯_保存で作るか聞き_キャンセル_作るは上書きしない", async ({ page }) => {
    await open(page);
    const opened = await openDocument(page);
    await typeAtEnd(page, "消える前に書いた");
    await emitEvent(page, "app-snapshot", withEditors(opened, APP.id, [tab({ dirty: true })]));

    // --- ウィンドウが前面に戻った → 確かめる → 消えている(AC-E06) ---
    await setMockReply(page, "check_document", { ok: { kind: "missing" } });
    await page.evaluate(() => window.dispatchEvent(new Event("focus")));
    await expect
      .poll(async () => (await readCallArgs(page, "check_document")).at(-1))
      .toEqual({ editorId: EDITOR_ID, withText: false });
    await expect(banner(page)).toHaveAttribute("data-kind", "missing");
    await expect(banner(page)).toContainText("ファイルが見つかりません");

    // --- 保存 → 作るか聞く → キャンセルでは何も呼ばない(AC-E05) ---
    await setMockReply(page, "save_document", { ok: { kind: "missing" } });
    await page.getByTestId("editor-save").click();
    const dialog = page.getByTestId("save-missing-dialog");
    await expect(dialog).toBeVisible();
    await expect(dialog).toContainText("「README.md」が見つかりません");
    await expect(dialog.getByRole("button", { name: "作る" })).toBeFocused();
    await dialog.getByRole("button", { name: "キャンセル" }).click();
    await expect(dialog).toHaveCount(0);
    expect(await readCallArgs(page, "save_document")).toHaveLength(1);
    await expect.poll(() => editorHasFocus(page)).toBe(true);

    // --- もう一度 ⌘S → 「作る」 → overwriteChanged が真の保存(Rust は RENAME_EXCL で作る)→ 帯が消える ---
    await page.keyboard.press("Meta+s");
    await expect(dialog).toBeVisible();
    await setMockReply(page, "save_document", { ok: { kind: "saved" } });
    await page.keyboard.press("Enter");
    await expect
      .poll(() => readCallArgs(page, "save_document"))
      .toEqual([
        { editorId: EDITOR_ID, text: `${DOCUMENT}消える前に書いた`, overwriteChanged: false },
        { editorId: EDITOR_ID, text: `${DOCUMENT}消える前に書いた`, overwriteChanged: false },
        { editorId: EDITOR_ID, text: `${DOCUMENT}消える前に書いた`, overwriteChanged: true },
      ]);
    await expect(banner(page)).toHaveCount(0);
    await expect
      .poll(async () => (await readCallArgs(page, "set_document_dirty")).at(-1))
      .toEqual({ editorId: EDITOR_ID, dirty: false });
  });

  test("読み取り専用は打っても変わらず未保存にならない_保存は帯で断る", async ({ page }) => {
    await open(page, {
      open_document: { ok: { kind: "opened", editorId: EDITOR_ID, text: DOCUMENT, lineEnding: "lf", readOnly: "not-writable" } },
      save_document: { ok: { kind: "read-only" } },
    });
    await treeRow(page, PATH).dblclick();
    await emitEvent(page, "app-snapshot", withEditors(base(), APP.id, [tab({ readOnly: "not-writable" })]));
    await expect(page.getByTestId("editor-read-only")).toContainText("書き込みの権限が無い");
    await content(page).click();

    await typeAtEnd(page, "書けない");
    await page.keyboard.press("Meta+s");

    expect(await lines(page)).toEqual(["# 題", "", "本文", ""]);
    expect(await readCallArgs(page, "set_document_dirty")).toEqual([]);
    await expect(page.getByTestId("editor-save")).toBeDisabled();
    // ⌘S は Rust に届き、Rust が read-only で断ったことを帯に出す
    await expect.poll(async () => (await readCallArgs(page, "save_document")).length).toBe(1);
    await expect(banner(page)).toContainText("読み取り専用のため保存できません");
  });
});

test.describe("閉じる・終了・外す(AC-E07)", () => {
  test("未保存のタブを閉じるとキャンセルでエディタへ戻り_保存を選ぶと保存してから閉じる", async ({ page }) => {
    await open(page, { close_editor: { ok: { kind: "needs-confirmation" } } });
    const opened = await openDocument(page);
    await typeAtEnd(page, "閉じる前");
    const dirty = withEditors(opened, APP.id, [tab({ dirty: true })]);
    await emitEvent(page, "app-snapshot", dirty);
    const closeButton = editorTab(page).getByRole("button", { name: "「README.md」を閉じる" });
    const dialog = page.getByTestId("unsaved-close-dialog");

    // --- キャンセル: 何も閉じずエディタにフォーカスが戻る ---
    await closeButton.click();
    await expect(dialog).toBeVisible();
    await expect(dialog.getByRole("button", { name: "保存", exact: true })).toBeFocused();
    await dialog.getByRole("button", { name: "キャンセル" }).click();
    await expect(dialog).toHaveCount(0);
    await expect.poll(() => editorHasFocus(page)).toBe(true);
    expect(await readCallArgs(page, "close_editor")).toEqual([{ editorId: EDITOR_ID, discard: false }]);
    expect(await readCallArgs(page, "save_document")).toEqual([]);

    // --- 保存: 保存できたら(破棄せずに)閉じる ---
    await closeButton.click();
    await expect(dialog).toBeVisible();
    await setMockReply(page, "close_editor", { ok: { kind: "closed" } });
    await page.keyboard.press("Enter");
    await expect(dialog).toHaveCount(0);
    await expect
      .poll(() => readCallArgs(page, "save_document"))
      .toEqual([{ editorId: EDITOR_ID, text: `${DOCUMENT}閉じる前`, overwriteChanged: false }]);
    await expect
      .poll(() => readCallArgs(page, "close_editor"))
      .toEqual([
        { editorId: EDITOR_ID, discard: false },
        { editorId: EDITOR_ID, discard: false },
        { editorId: EDITOR_ID, discard: false },
      ]);

    // Rust が閉じた写しを送る → エディタのタブが消えターミナルに戻る
    await emitEvent(page, "app-snapshot", withEditors(opened, APP.id, [], null));
    await expect(editorTab(page)).toHaveCount(0);
    await expect(panel(page)).toHaveCount(0);
    await expect(page.locator(`[data-testid="terminal-host"][data-terminal-id="${TERMINAL_ID}"]`)).toBeVisible();
  });

  test("プロジェクトを外す確認と終了の確認に未保存の文書が並ぶ_動作中のターミナルと一緒にも出る", async ({ page }) => {
    const unsaved = [{ editorId: EDITOR_ID, projectName: APP.name, path: PATH }];
    const running = [sampleRunningTerminal(APP.id, APP.name, TERMINAL_ID, "ターミナル 1")];
    await open(page, { remove_project: { ok: { kind: "needsConfirmation", running: [], unsaved } } });
    const opened = await openDocument(page);
    await typeAtEnd(page, "未保存");
    await emitEvent(page, "app-snapshot", withEditors(opened, APP.id, [tab({ dirty: true })]));

    // --- 外す: 未保存の文書だけでも確認が出る。中止では外さない ---
    await page.getByTestId("project-card").filter({ hasText: APP.name }).click({ button: "right" });
    await page.getByRole("menuitem", { name: "一覧から削除" }).click();
    const remove = page.getByTestId("remove-project-dialog");
    await expect(remove).toBeVisible();
    await expect(remove).toContainText("保存していない文書があります");
    await expect(page.getByTestId("remove-unsaved-list")).toHaveText(PATH);
    await expect(remove).not.toContainText("動作中のターミナルがあります");
    await remove.getByRole("button", { name: "中止" }).click();
    await expect(remove).toHaveCount(0);
    expect(await readCallArgs(page, "remove_project")).toEqual([{ projectId: APP.id, confirmed: false }]);
    // 文書は残っている
    await expect.poll(() => lines(page)).toEqual(["# 題", "", "本文", "未保存"]);

    // --- 終了: 動作中のターミナルと未保存の文書の両方が並ぶ。取り消しでは何も呼ばない ---
    await emitEvent(page, "quit-requested", { running, unsaved });
    const quit = page.getByTestId("quit-dialog");
    await expect(quit).toBeVisible();
    await expect(quit).toContainText("動作中のターミナルがあります");
    await expect(page.getByTestId("quit-unsaved-list")).toHaveText(`${APP.name}: ${PATH}`);
    await quit.getByRole("button", { name: "取り消し" }).click();
    await expect(quit).toHaveCount(0);
    expect(await readCallArgs(page, "confirm_quit")).toEqual([]);

    // --- 終了を選ぶと confirm_quit ---
    await emitEvent(page, "quit-requested", { running, unsaved });
    await expect(quit).toBeVisible();
    await quit.getByRole("button", { name: "終了" }).click();
    await expect.poll(() => readCallArgs(page, "confirm_quit")).toHaveLength(1);
  });
});

test.describe("キーと Vim(AC-E08・E09)", () => {
  test("設定画面で_vim_に切り替えるとエディタを作り直さずに効き_:w_で保存する", async ({ page }) => {
    await open(page);
    const opened = await openDocument(page);
    // 作り直されていないことを、エディタの要素に付けた印で確かめる
    await panel(page).locator(".cm-editor").evaluate((el) => el.setAttribute("data-e2e-mark", "same"));

    await emitEvent(page, "menu-command", "open-settings");
    const sheet = page.getByTestId("settings-dialog");
    await expect(sheet).toBeVisible();
    await page.getByTestId("editor-vim-switch").click();
    await expect.poll(() => readCallArgs(page, "set_editor_vim")).toEqual([{ enabled: true }]);
    const vimOn = { ...opened, editor: { vim: true } };
    await emitEvent(page, "app-snapshot", vimOn);
    await expect(page.getByTestId("editor-vim-switch")).toHaveAttribute("aria-checked", "true");
    await sheet.getByRole("button", { name: "完了" }).click();
    await expect(sheet).toHaveCount(0);

    await content(page).click();
    await expect.poll(() => editorHasFocus(page)).toBe(true);
    await expect(panel(page).locator('.cm-editor[data-e2e-mark="same"]')).toHaveCount(1);
    // 通常のモードでは文字にならない。G で最後の行へ、o で下に行を開いて挿入、Esc で戻る
    await page.keyboard.press("Shift+G");
    await page.keyboard.press("o");
    await page.keyboard.type("vim で書いた");
    await page.keyboard.press("Escape");
    await page.keyboard.type(":w");
    await page.keyboard.press("Enter");

    await expect
      .poll(() => readCallArgs(page, "save_document"))
      .toEqual([{ editorId: EDITOR_ID, text: `${DOCUMENT}\nvim で書いた`, overwriteChanged: false }]);
    expect(await lines(page)).not.toContain(":w");

    // --- 通常に戻すと i は文字になる(作り直さない) ---
    await emitEvent(page, "app-snapshot", { ...opened, editor: { vim: false } });
    await content(page).click();
    await typeAtEnd(page, "i");
    await expect.poll(async () => (await lines(page)).at(-1)).toBe("vim で書いたi");
    await expect(panel(page).locator('.cm-editor[data-e2e-mark="same"]')).toHaveCount(1);
  });

  test("メニューのキーはエディタが受けず_表に無いキーと取り消しと全選択はエディタの中で効く", async ({ page }) => {
    await open(page);
    await openDocument(page);
    const searchPanel = panel(page).locator(".cm-search");

    // ⇧⌘G(表示の切替。menu-item)は CodeMirror の「前を探す」にならない
    await page.keyboard.press("Meta+Shift+g");
    await expect(searchPanel).toHaveCount(0);
    // ⌘F は表に無いので CodeMirror の検索が開く(エディタが受けている印)
    await page.keyboard.press("Meta+f");
    await expect(searchPanel).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(searchPanel).toHaveCount(0);
    await expect.poll(() => editorHasFocus(page)).toBe(true);

    // ⌘Z は CodeMirror の取り消し
    await typeAtEnd(page, "消す");
    await expect.poll(async () => (await lines(page)).at(-1)).toBe("消す");
    await page.keyboard.press("Meta+z");
    await expect.poll(() => lines(page)).toEqual(["# 題", "", "本文", ""]);

    // ⌘A はメニューから menu-command の select-all で届き、エディタの全体を選ぶ
    await emitEvent(page, "menu-command", "select-all");
    await page.keyboard.type("置き換えた");
    await expect.poll(() => lines(page)).toEqual(["置き換えた"]);
    await page.keyboard.press("Meta+s");
    await expect
      .poll(() => readCallArgs(page, "save_document"))
      .toEqual([{ editorId: EDITOR_ID, text: "置き換えた", overwriteChanged: false }]);
  });
});

test.describe("タイル表示とプレビュー(AC-E03・E11)", () => {
  test("タイル表示で開くとタブ表示に替わり_タイルに戻すとエディタは隠れるが閉じず_戻すと文字が残る", async ({ page }) => {
    const tiles = withTileDisplay(base());
    await open(page, {}, tiles);
    await expect(page.getByTestId("tile-grid")).toBeVisible();
    await expect(page.getByTestId("tile")).toHaveCount(2);

    // Rust は開いたらタブ表示に替えた写しを送る(withEditors が写す)
    const opened = await openDocument(page, tiles);
    expect(opened.display.mode).toBe("tabs");
    await expect(page.getByTestId("tile-grid")).toHaveCount(0);
    await expect(editorTab(page).getByRole("tab")).toHaveAttribute("aria-selected", "true");
    await typeAtEnd(page, "タイルの前");

    // タイル表示に戻す: タイルはターミナルだけで、エディタは出ない。閉じない
    await emitEvent(page, "app-snapshot", withTileDisplay(withEditors(opened, APP.id, [tab({ dirty: true })])));
    await expect(page.getByTestId("tile")).toHaveCount(2);
    await expect(panel(page)).toHaveCount(0);
    await expect(page.getByTestId("tile-grid").locator(".cm-editor")).toHaveCount(0);
    expect(await readCallArgs(page, "close_editor")).toEqual([]);

    // タブ表示に戻すと同じ文書が書きかけのまま出る
    await emitEvent(page, "app-snapshot", withEditors(opened, APP.id, [tab({ dirty: true })]));
    await expect(content(page)).toBeVisible();
    await expect.poll(async () => (await lines(page)).at(-1)).toBe("タイルの前");
  });

  test("プレビューは生の_HTML_と危ないリンクを文字のまま出し_要素も外への読み込みも作らない", async ({ page }) => {
    const hostile = [
      "# 危ない文書",
      "",
      "<script>window.__pwned = 1</script>",
      "",
      '<img src="https://example.invalid/a.png" onerror="window.__pwned = 2">',
      "",
      '<iframe src="https://example.invalid/"></iframe>',
      "",
      '段落の <b style="color:red">太字のつもり</b> と &lt;i&gt; と &amp; と &unknown;',
      "",
      "[押すと危ない](javascript:window.__pwned=3) と ![図](https://example.invalid/b.png)",
      "",
      "- [x] <input type=checkbox>",
      "",
    ].join("\n");
    const requested: string[] = [];
    page.on("request", (request) => requested.push(request.url()));
    let dialogs = 0;
    page.on("dialog", (d) => {
      dialogs++;
      void d.dismiss();
    });
    await open(page, {
      open_document: { ok: { kind: "opened", editorId: EDITOR_ID, text: hostile, lineEnding: "lf", readOnly: null } },
    });
    await openDocument(page);
    await page.getByTestId("editor-view-switch").getByRole("radio", { name: "プレビュー" }).click();
    const preview = page.getByTestId("editor-preview");
    await expect(preview.getByRole("heading", { name: "危ない文書" })).toBeVisible();

    await expect(preview).toContainText("<script>window.__pwned = 1</script>");
    await expect(preview).toContainText('<img src="https://example.invalid/a.png"');
    await expect(preview).toContainText("<iframe");
    await expect(preview).toContainText('<b style="color:red">');
    // 決めた文字参照だけを文字に直し、知らない名前は書かれたまま
    await expect(preview).toContainText("<i> と & と &unknown;");
    await expect(preview).toContainText("[画像: 図]");
    await expect(preview.locator(".md-link")).toHaveAttribute("title", /javascript:window\.__pwned=3/);
    await expect(preview).toContainText("☑");

    const md = preview.locator(".md-preview");
    expect(await md.locator("a, img, iframe, script, input, object, embed, [style], [href], [src]").count()).toBe(0);
    await preview.locator(".md-link").click();
    expect(await page.evaluate(() => (window as unknown as { __pwned?: number }).__pwned ?? null)).toBeNull();
    expect(dialogs).toBe(0);
    expect(requested.filter((url) => url.includes("example.invalid"))).toEqual([]);
  });
});
