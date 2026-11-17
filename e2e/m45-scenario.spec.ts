// M4.5 のシナリオの画面の自動テスト(T66a)。
//
// ショートカットの設定(ADR-012 の AC-K01〜K07)を、画面の側で機能をまたいだ流れで通す。
// 設定で替えたキーが、ツリーの右クリックのメニューのキーの表示・ツリーのキー・タブバーと表示モードのツールチップと読み上げの
// キー・ターミナルのキーの振り分け(AC-K06)のすべてにそろって出ることを、起動時と動作中の保存(settings-changed)で確かめる。
// 部品ごとの細かい振る舞い(知らせの寸法・画像など)は settings.spec.ts が見るので、ここでは繰り返さない。
//
// 偽の Rust 側(fixtures/tauriMock.ts)は設定ファイルを読まない。本物の Rust が設定ファイルの問題の無い行を当てた結果の表を
// shortcutTableWith で作って get_shortcuts の応答に置き、知らせは get_settings_notice の応答に置く(文言は arin-core の
// shortcuts/overrides.rs の notice_line と同じ形)。動作中の保存は、応答を setMockReply で替えてから settings-changed
// (中身は null)を emitEvent で流して代える。
//
// メニューバーの項目(⌘T など)は OS が受けるので、ここではメニューバーを見ない(Rust の側は src-tauri/tests/app_menu.rs・
// app_settings.rs、実アプリは node scripts/smoke.mjs --settings と T66c の本人の確認)。ターミナルのキーの振り分けは、
// xterm.js が文字を送るキー(⌘ と Enter の組み合わせは CR を送る)で見る: アプリが受けるキーは write_terminal に届かず、
// ターミナルに任せたキーは届く。

import { expect, type Page, test } from "@playwright/test";

import type { ShortcutTable } from "../src/ipc/generated/ShortcutTable";
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
  setMockReply,
  shortcutTableWith,
  type TauriMockConfig,
} from "./fixtures/tauriMock";

const APP: SampleProject = { id: "p-app", name: "app", color: "green", displayPath: "~/dev/app" };
const TERMINAL_ID = `${APP.id}-t1`;

const LISTINGS: Record<string, MockListing> = {
  [listingKey(APP.id, "")]: listed([
    ["src", "directory"],
    ["README.md", "file"],
  ]),
};

/**
 * 設定ファイルで替えた行(どれも ADR-012 の 3 の規則を通る: メニューの項目は ⌘ を含む、画面の行は ⌘ を含む、予約のキーでない、
 * ほかの行と重ならない)。view.toggle-maximize は、既定の ⇧⌘↩ をターミナルに渡し、新しい ⌥⌘↩ をアプリが受ける形にする(AC-K06)。
 */
const CHANGED_KEYS = {
  "terminal.new-tab": "Cmd+Shift+Y",
  "terminal.close-tab": "Cmd+Alt+W",
  "tree.new-file": "Cmd+Shift+J",
  "tree.copy-relative-path": "Cmd+Alt+P",
  "view.toggle-display-mode": "Cmd+Shift+L",
  "view.toggle-maximize": "Cmd+Alt+Enter",
} as const;
const CHANGED: ShortcutTable = shortcutTableWith(CHANGED_KEYS);
const DEFAULTS: ShortcutTable = shortcutTableWith();

/** 画面に出るはずのキーの表示(ツールチップ・メニュー。修飾キーは ⌃⌥⇧⌘ の順)と読み上げ用のキー。 */
interface ExpectedKeys {
  newTab: string;
  newTabAria: string;
  closeTab: string;
  newFile: string;
  copyRelative: string;
  displayMode: string;
}

const DEFAULT_LABELS: ExpectedKeys = {
  newTab: "⌘T",
  newTabAria: "Meta+T",
  closeTab: "⌘W",
  newFile: "⌘N",
  copyRelative: "⌥⇧⌘C",
  displayMode: "⇧⌘G",
};
const CHANGED_LABELS: ExpectedKeys = {
  newTab: "⇧⌘Y",
  newTabAria: "Meta+Shift+Y",
  closeTab: "⌥⌘W",
  newFile: "⇧⌘J",
  copyRelative: "⌥⌘P",
  displayMode: "⇧⌘L",
};

/**
 * 問題のある行 1 つ(tree.rename = F2)の知らせ(arin-core の notice_line と同じ形。F キーは KeyChord の書式に無いので
 * 「読めないキー」になる。node scripts/smoke.mjs --settings で実アプリが出す標準エラーの行と同じ理由)。
 */
const ONE_PROBLEM_NOTICE =
  "ショートカットの設定の 1 行を使わず、既定のキーのままにしました: tree.rename = F2(キーを読めません: 修飾キーでもキーでもない部分があります)";

const newTabButton = (page: Page) => page.getByRole("button", { name: "新規ターミナル", exact: true });
const closeTabButton = (page: Page) => page.getByRole("button", { name: "「ターミナル 1」を閉じる" });
const tilesButton = (page: Page) =>
  page.getByTestId("display-mode-switch").getByRole("button", { name: "タイル表示", exact: true });
const row = (page: Page, path: string) => page.locator(`[data-testid="tree-row"][data-path=${JSON.stringify(path)}]`);
const fileTree = (page: Page) => page.getByTestId("file-tree");
const nameInput = (page: Page) => page.getByTestId("tree-name-input");
const contextMenu = (page: Page) => page.getByTestId("context-menu");
const settingsNotice = (page: Page) => page.getByTestId("settings-notice");

/** 画面を開き、準備完了とターミナルの最初の接続が届くまで待つ。出力のチャネルの番号を返す。 */
async function open(page: Page, overrides: Partial<TauriMockConfig> = {}): Promise<number> {
  await installTauriMock(page, { get_app_snapshot: { ok: projectsSnapshot([APP]) }, ...overrides }, LISTINGS);
  await page.goto("/");
  await expect
    .poll(async () => (await readRecordedCalls(page)).map((call) => call.command))
    .toContain("notify_app_ready");
  await expect.poll(async () => (await readCallArgs(page, "connect_terminal")).length).toBe(1);
  const [connect] = await readCallArgs(page, "connect_terminal");
  if (connect === undefined) throw new Error("connect_terminal が届いていない");
  await expect(row(page, "README.md")).toBeVisible();
  return parseChannelId(connect.onOutput);
}

/** 呼ばれた回数(コマンドごと)。 */
async function callCount(page: Page, command: string): Promise<number> {
  return (await readRecordedCalls(page)).filter((c) => c.command === command).length;
}

/** 設定ファイルを保存した形: 表と知らせの応答を替えてから settings-changed を流し、画面が表を取り直すまで待つ。 */
async function saveSettings(page: Page, table: ShortcutTable, notice: string | null = null): Promise<void> {
  const before = await callCount(page, "get_shortcuts");
  await setMockReply(page, "get_shortcuts", { ok: table });
  await setMockReply(page, "get_settings_notice", { ok: notice });
  await emitEvent(page, "settings-changed", null);
  await expect.poll(() => callCount(page, "get_shortcuts")).toBe(before + 1);
}

/** ツリーの README.md の右クリックのメニューの、項目の名前 → キーの表示。見終わったら閉じる。 */
async function treeMenuShortcuts(page: Page): Promise<Record<string, string>> {
  await row(page, "README.md").click({ button: "right" });
  await expect(contextMenu(page)).toBeVisible();
  const shortcuts = await contextMenu(page)
    .getByRole("menuitem")
    .evaluateAll((elements) =>
      Object.fromEntries(
        elements.map((el) => [
          el.querySelector(".context-menu__label")?.textContent ?? "",
          el.querySelector(".context-menu__shortcut")?.textContent ?? "",
        ]),
      ),
    );
  await page.keyboard.press("Escape");
  await expect(contextMenu(page)).toHaveCount(0);
  return shortcuts;
}

/** メニューのキーの表示・タブバーと表示モードのツールチップ・読み上げのキーが、そろって expected になっている。 */
async function expectKeysShown(page: Page, expected: ExpectedKeys): Promise<void> {
  await expect(newTabButton(page)).toHaveAttribute("title", `新規ターミナル(${expected.newTab})`);
  await expect(newTabButton(page)).toHaveAttribute("aria-keyshortcuts", expected.newTabAria);
  await expect(closeTabButton(page)).toHaveAttribute("title", `タブを閉じる(${expected.closeTab})`);
  await expect(tilesButton(page)).toHaveAttribute("title", `タイル表示(${expected.displayMode})`);
  const menu = await treeMenuShortcuts(page);
  expect(menu).toMatchObject({
    開く: "⌘O",
    "相対パスをコピー": expected.copyRelative,
    "名称変更…": "↩",
    "新規ファイル…": expected.newFile,
    "新規フォルダ…": "⇧⌘N",
  });
}

/**
 * ツリーにフォーカスして、古いキー(oldKey)では新規ファイルの入力が開かず、新しいキー(newKey)で開くことを確かめる。
 * 古いキーの処理が済んだことは、続けて押した ⌘O(開く。替えていない)が届いたことで見る。入力は Escape で閉じる。
 * README.md は Markdown なので、⌘O は M4.8 から open_document(エディタ)で届く(ADR-015 の 4)。
 */
async function expectNewFileKey(page: Page, oldKey: string, newKey: string): Promise<void> {
  const opened = (await readCallArgs(page, "open_document")).length;
  await row(page, "README.md").click();
  await expect(fileTree(page)).toBeFocused();
  await page.keyboard.press(oldKey);
  await page.keyboard.press("Meta+O");
  await expect.poll(async () => (await readCallArgs(page, "open_document")).length).toBe(opened + 1);
  await expect(nameInput(page)).toHaveCount(0);

  await page.keyboard.press(newKey);
  await expect(nameInput(page)).toBeFocused();
  await nameInput(page).press("Escape");
  await expect(nameInput(page)).toHaveCount(0);
}

/** ターミナルに書いた文字(write_terminal の中身をつないだもの)。 */
async function typedIntoTerminal(page: Page): Promise<string> {
  return (await readCallArgs(page, "write_terminal")).map((w) => w.data).join("");
}

/**
 * ターミナルにフォーカスして appKey と terminalKey を押し、最後に印の文字 mark を打つ。appKey(アプリが受けるキー)は
 * write_terminal に届かず、terminalKey(ターミナルに任せるキー)は xterm.js が sent に変えて届く(AC-K06)。
 */
async function expectTerminalRouting(
  page: Page,
  keys: { appKey: string; terminalKey: string; sent: string; mark: string },
): Promise<void> {
  const before = await typedIntoTerminal(page);
  await page.getByTestId("terminal-host").locator(".xterm-screen").click();
  await expect(page.locator(".xterm-helper-textarea")).toBeFocused();
  await page.keyboard.press(keys.appKey);
  await page.keyboard.press(keys.terminalKey);
  await page.keyboard.type(keys.mark);
  await expect.poll(() => typedIntoTerminal(page)).toBe(`${before}${keys.sent}${keys.mark}`);
  const writes = await readCallArgs(page, "write_terminal");
  expect(writes.every((w) => w.terminalId === TERMINAL_ID)).toBe(true);
}

/** ターミナルの画面の行(xterm.js の DOM 描画)。 */
const screenRows = (page: Page) => page.getByTestId("terminal-host").locator(".xterm-rows > div");

// 各テストの終わりに、画面の失敗(例外、通信の失敗の報告)が無かったことを見る。
test.afterEach(async ({ page }) => {
  const reported = await readCallArgs(page, "report_frontend_error");
  expect(reported.map((r) => r.message), "画面の失敗が Rust へ報告された").toEqual([]);
});

test.describe("M4.5 のシナリオ(ショートカットの設定)", () => {
  test("替えた表で起動し_メニューのキーの表示とツリーのキーとタブのツールチップがそろう", async ({ page }) => {
    await open(page, { get_shortcuts: { ok: CHANGED } });

    // AC-K01・K02: 表示はすべて新しい表から出る。替えなかった行(開く・名称変更…・新規フォルダ…)は既定のまま(AC-K03)
    await expectKeysShown(page, CHANGED_LABELS);
    // 問題の無い設定なので、設定の知らせは出ない
    await expect(settingsNotice(page)).toHaveCount(0);

    // AC-K02: ツリーのキーは新しいキーで効き、古いキー(⌘N)では効かない
    await expectNewFileKey(page, "Meta+N", "Meta+Shift+J");

    // AC-K06: 新しいメニューのキー(⌥⌘↩)はターミナルに渡らず、替える前のキー(⇧⌘↩)はターミナルに渡る(CR)
    await expectTerminalRouting(page, { appKey: "Meta+Alt+Enter", terminalKey: "Meta+Shift+Enter", sent: "\r", mark: "a" });
  });

  test("設定が変わったと届くと全部が新しい表に追従する", async ({ page }) => {
    const channelId = await open(page);

    // 既定の表: 表示は既定のキー、⇧⌘↩(拡大)はアプリが受け、⌥⌘↩ はターミナルに渡る(ESC + CR)
    await expectKeysShown(page, DEFAULT_LABELS);
    await expectNewFileKey(page, "Meta+Shift+J", "Meta+N");
    await expectTerminalRouting(page, { appKey: "Meta+Shift+Enter", terminalKey: "Meta+Alt+Enter", sent: "\x1b\r", mark: "1" });

    // AC-K07: 動作中に保存すると、再起動せずに全部が新しい表に替わる
    await saveSettings(page, CHANGED);
    await expectKeysShown(page, CHANGED_LABELS);
    await expectNewFileKey(page, "Meta+N", "Meta+Shift+J");
    await expectTerminalRouting(page, { appKey: "Meta+Alt+Enter", terminalKey: "Meta+Shift+Enter", sent: "\r", mark: "2" });

    // AC-K03: 行を消して保存すると既定に戻る
    await saveSettings(page, DEFAULTS);
    await expectKeysShown(page, DEFAULT_LABELS);
    await expectNewFileKey(page, "Meta+Shift+J", "Meta+N");
    await expectTerminalRouting(page, { appKey: "Meta+Shift+Enter", terminalKey: "Meta+Alt+Enter", sent: "\x1b\r", mark: "3" });

    // AC-K07: シェルは続いている(つなぎ直し・閉じる・新しいタブが無く、同じチャネルの出力がそのまま画面に出る)
    expect(await callCount(page, "connect_terminal")).toBe(1);
    expect(await callCount(page, "close_terminal")).toBe(0);
    expect(await callCount(page, "new_terminal")).toBe(0);
    await sendChannelBytes(page, channelId, ["still-running"]);
    await expect(screenRows(page).first()).toHaveText("still-running");
    await expect(settingsNotice(page)).toHaveCount(0);
  });

  test("問題のある行の知らせが_1_行出て閉じられる", async ({ page }) => {
    // 設定ファイルに正しい行(terminal.new-tab = Cmd+Shift+Y)と問題のある行(tree.rename = F2)を 1 つずつ書いた形:
    // 正しい行だけが効き、問題のある行は既定(↩)のまま、知らせが 1 行(AC-K04)
    await open(page, {
      get_shortcuts: { ok: shortcutTableWith({ "terminal.new-tab": "Cmd+Shift+Y" }) },
      get_settings_notice: { ok: ONE_PROBLEM_NOTICE },
    });

    const text = settingsNotice(page).getByTestId("settings-notice-text");
    await expect(settingsNotice(page)).toBeVisible();
    await expect(text).toHaveText(ONE_PROBLEM_NOTICE);
    await expect(newTabButton(page)).toHaveAttribute("title", "新規ターミナル(⇧⌘Y)");
    expect(await treeMenuShortcuts(page)).toMatchObject({ "名称変更…": "↩", "新規ファイル…": "⌘N" });

    // 既定に戻した行は既定のキーで効く(↩ で名称変更の入力が開く)
    await row(page, "README.md").click();
    await expect(fileTree(page)).toBeFocused();
    await page.keyboard.press("Enter");
    await expect(nameInput(page)).toBeFocused();
    await expect(nameInput(page)).toHaveValue("README.md");
    await nameInput(page).press("Escape");
    await expect(nameInput(page)).toHaveCount(0);

    // 閉じられる。閉じても表はそのまま
    await settingsNotice(page).getByRole("button", { name: "設定の知らせを閉じる" }).click();
    await expect(settingsNotice(page)).toHaveCount(0);
    await expect(newTabButton(page)).toHaveAttribute("title", "新規ターミナル(⇧⌘Y)");

    // 同じ知らせのまま別の行を保存しても出し直さない。問題の行を直して保存すると、知らせは出ないまま表が替わる
    await saveSettings(page, shortcutTableWith({ "terminal.new-tab": "Cmd+Shift+Y", "tree.new-file": "Cmd+Shift+J" }), ONE_PROBLEM_NOTICE);
    await expect(settingsNotice(page)).toHaveCount(0);
    await saveSettings(page, shortcutTableWith({ "terminal.new-tab": "Cmd+Shift+Y", "tree.rename": "Cmd+Alt+E" }));
    await expect(settingsNotice(page)).toHaveCount(0);
    expect(await treeMenuShortcuts(page)).toMatchObject({ "名称変更…": "⌥⌘E", "新規ファイル…": "⌘N" });
  });
});
