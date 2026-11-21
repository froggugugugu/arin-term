// 設定への追従(T65e): 届いた表と設定の知らせ。
//
// 偽の Rust 側(fixtures/tauriMock.ts)の get_shortcuts・get_settings_notice の応答を置いて画面を開く。動作中の保存は、
// 応答を setMockReply で替えてから settings-changed(中身は null)を emitEvent で流して代える(本物の Rust は保存を検知して
// 読み直し、同じイベントを送る)。メニューの項目で受けるキーは OS が受けるので、ここではツールチップと読み上げのキー、
// 画面で受けるツリーのキーを見る。
// 知らせの見た目は、ライトとダーク × 知らせ 1 行と 2 行の画像を testreport/e2e/settings/ に撮る(人が目で確かめる)。

import { expect, type Locator, type Page, test } from "@playwright/test";

import type { AppSnapshot } from "../src/ipc/generated/AppSnapshot";
import type { ShortcutTable } from "../src/ipc/generated/ShortcutTable";
import { DEFAULT_SHORTCUT_TABLE } from "../src/logic/keys/defaultShortcuts";
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
  setMockReply,
  type TauriMockConfig,
} from "./fixtures/tauriMock";

const APP: SampleProject = { id: "p-app", name: "app", color: "green", displayPath: "~/dev/app" };

const LISTINGS: Record<string, MockListing> = {
  [listingKey(APP.id, "")]: listed([
    ["src", "directory"],
    ["README.md", "file"],
  ]),
};

const STATE_NOTICE = "状態ファイルを読めなかったため state.json.broken-20261115T093000Z に退避し、空の状態で起動しました";
const SETTINGS_NOTICE =
  "ショートカットの設定の 2 行を使わず、既定のキーのままにしました: terminal.new-tab = Cmd+W(Cmd+W は terminal.close-tab が使っています)、tree.rename = Shift+R(⌘ を含むキーか、修飾キーの無い Enter・Space・Backspace・Delete・F1〜F12 にしてください)";

/** 既定の表の一部の行のキーを替えた表。 */
function withKeys(keys: Readonly<Record<string, string>>): ShortcutTable {
  return DEFAULT_SHORTCUT_TABLE.map((binding) => ({ ...binding, accelerator: keys[binding.command] ?? binding.accelerator }));
}

async function open(page: Page, snapshot: AppSnapshot, overrides: Partial<TauriMockConfig> = {}): Promise<void> {
  await installTauriMock(page, { get_app_snapshot: { ok: snapshot }, ...overrides }, LISTINGS);
  await page.goto("/");
  await expect
    .poll(async () => (await readRecordedCalls(page)).map((call) => call.command))
    .toContain("notify_app_ready");
}

async function box(locator: Locator) {
  const b = await locator.boundingBox();
  if (b === null) throw new Error("要素の位置と大きさが取れない(表示されていない)");
  return b;
}

/** 画像の置き場所(testreport/ は .gitignore 済み)。 */
function shotPath(name: string): string {
  const configFile = test.info().config.configFile;
  if (configFile === undefined) throw new Error("Playwright の設定ファイルの場所が分からない");
  return `${configFile.slice(0, configFile.lastIndexOf("/"))}/testreport/e2e/settings/${name}.png`;
}

const newTabButton = (page: Page) => page.getByRole("button", { name: "新規ターミナル", exact: true });
const stateNotice = (page: Page) => page.getByTestId("notice");
const settingsNotice = (page: Page) => page.getByTestId("settings-notice");
const row = (page: Page, path: string) => page.locator(`[data-testid="tree-row"][data-path=${JSON.stringify(path)}]`);
const nameInput = (page: Page) => page.getByTestId("tree-name-input");

// 各テストの終わりに、画面の失敗(例外、通信の失敗の報告)が無かったことを見る。
test.afterEach(async ({ page }) => {
  const reported = await readCallArgs(page, "report_frontend_error");
  expect(reported.map((r) => r.message), "画面の失敗が Rust へ報告された").toEqual([]);
});

test.describe("設定の表への追従(AC-K02・AC-K07)", () => {
  test("タブバーの新規のボタンのツールチップと読み上げのキーが表から出る", async ({ page }) => {
    await open(page, projectsSnapshot([APP]), {
      get_shortcuts: { ok: withKeys({ "terminal.new-tab": "Cmd+Shift+Y", "terminal.close-tab": "Cmd+Alt+W" }) },
    });

    await expect(newTabButton(page)).toHaveAttribute("title", "新規ターミナル(⇧⌘Y)");
    await expect(newTabButton(page)).toHaveAttribute("aria-keyshortcuts", "Meta+Shift+Y");
    await expect(page.getByRole("button", { name: "「ターミナル 1」を閉じる" })).toHaveAttribute(
      "title",
      "タブを閉じる(⌥⌘W)",
    );
  });

  test("既定の表ならツールチップと読み上げのキーは_cmd_t", async ({ page }) => {
    await open(page, projectsSnapshot([APP]));

    await expect(newTabButton(page)).toHaveAttribute("title", "新規ターミナル(⌘T)");
    await expect(newTabButton(page)).toHaveAttribute("aria-keyshortcuts", "Meta+T");
  });

  test("表が替わったと届くとツールチップが替わる", async ({ page }) => {
    await open(page, projectsSnapshot([APP]));
    await expect(newTabButton(page)).toHaveAttribute("title", "新規ターミナル(⌘T)");

    await setMockReply(page, "get_shortcuts", { ok: withKeys({ "terminal.new-tab": "Cmd+Shift+Y" }) });
    await emitEvent(page, "settings-changed", null);

    await expect(newTabButton(page)).toHaveAttribute("title", "新規ターミナル(⇧⌘Y)");
    await expect(newTabButton(page)).toHaveAttribute("aria-keyshortcuts", "Meta+Shift+Y");
    // タブの列と「+」の並び(はみ出し・重なりが無い)は変わらない
    const list = await box(page.getByRole("tablist", { name: "ターミナルのタブ" }));
    const plus = await box(newTabButton(page));
    expect(plus.x).toBeGreaterThanOrEqual(list.x + list.width - 1);
  });

  test("設定で替えたツリーのキーで新規ファイルの入力が開き古いキーでは開かない", async ({ page }) => {
    await open(page, projectsSnapshot([APP]));
    await expect(row(page, "README.md")).toBeVisible();

    // 動作中に保存した形: 新規ファイルを ⇧⌘J に替える
    await setMockReply(page, "get_shortcuts", { ok: withKeys({ "tree.new-file": "Cmd+Shift+J" }) });
    await emitEvent(page, "settings-changed", null);
    await expect
      .poll(async () => (await readRecordedCalls(page)).filter((c) => c.command === "get_shortcuts").length)
      .toBe(2);

    await row(page, "README.md").click();
    await expect(page.getByTestId("file-tree")).toBeFocused();

    // 古いキー(⌘N)では開かない。続けて押した ⌘O(開く)が届いた時点で、⌘N の処理は済んでいる
    await page.keyboard.press("Meta+N");
    await page.keyboard.press("Meta+O");
    // README.md は Markdown なので、⌘O は M4.8 から open_document(エディタ)で届く(ADR-015 の 4)
    await expect.poll(async () => (await readCallArgs(page, "open_document")).length).toBe(1);
    await expect(nameInput(page)).toHaveCount(0);

    // 新しいキーで開く
    await page.keyboard.press("Meta+Shift+J");
    await expect(nameInput(page)).toBeFocused();
    await nameInput(page).fill("new.ts");
    await nameInput(page).press("Enter");
    await expect
      .poll(() => readCallArgs(page, "create_entry"))
      .toEqual([{ projectId: APP.id, parent: "", name: "new.ts", kind: "file" }]);
  });
});

test.describe("設定の知らせ(AC-K04)", () => {
  test("状態の知らせと設定の知らせが_2_行出て別々に閉じられる", async ({ page }) => {
    await page.setViewportSize({ width: 960, height: 600 });
    await open(page, projectsSnapshot([APP], { notice: STATE_NOTICE }), {
      get_settings_notice: { ok: SETTINGS_NOTICE },
    });

    await expect(stateNotice(page)).toBeVisible();
    await expect(settingsNotice(page)).toBeVisible();
    await expect(settingsNotice(page).getByTestId("settings-notice-text")).toHaveText(SETTINGS_NOTICE);
    await expect(settingsNotice(page).getByTestId("settings-notice-text")).toHaveAttribute("title", SETTINGS_NOTICE);

    // 状態の知らせの下に、同じ高さ・同じ幅で並ぶ(重ならない。長くても 1 行で、ウィンドウからはみ出さない)
    const first = await box(stateNotice(page));
    const second = await box(settingsNotice(page));
    expect(second.y).toBeGreaterThanOrEqual(first.y + first.height - 0.5);
    expect(Math.abs(second.height - first.height)).toBeLessThan(1);
    expect(second.height).toBeLessThan(40);
    expect([second.x, second.width]).toEqual([first.x, first.width]);
    expect(second.x + second.width).toBeLessThanOrEqual(960);
    const text = settingsNotice(page).getByTestId("settings-notice-text");
    expect(await text.evaluate((el) => getComputedStyle(el).whiteSpace)).toBe("nowrap");
    // 閉じるボタンは文と重ならない
    const close = await box(settingsNotice(page).getByRole("button", { name: "設定の知らせを閉じる" }));
    const textBox = await box(text);
    expect(close.x).toBeGreaterThanOrEqual(textBox.x + textBox.width);

    // 設定の知らせを閉じても、状態の知らせは残る
    await settingsNotice(page).getByRole("button", { name: "設定の知らせを閉じる" }).click();
    await expect(settingsNotice(page)).toHaveCount(0);
    await expect(stateNotice(page)).toBeVisible();

    // 同じ知らせが届き直しても(表だけが変わったなど)出さない。違う知らせなら出し直す
    await emitEvent(page, "settings-changed", null);
    await expect
      .poll(async () => (await readRecordedCalls(page)).filter((c) => c.command === "get_settings_notice").length)
      .toBe(2);
    await expect(settingsNotice(page)).toHaveCount(0);
    const next = "設定ファイルの 3 行 5 列目を読めないため、直前の設定のままです";
    await setMockReply(page, "get_settings_notice", { ok: next });
    await emitEvent(page, "settings-changed", null);
    await expect(settingsNotice(page).getByTestId("settings-notice-text")).toHaveText(next);

    // 状態の知らせを閉じても、設定の知らせは残る
    await stateNotice(page).getByRole("button", { name: "知らせを閉じる", exact: true }).click();
    await expect(stateNotice(page)).toHaveCount(0);
    await expect(settingsNotice(page)).toBeVisible();

    // 問題が直れば消える
    await setMockReply(page, "get_settings_notice", { ok: null });
    await emitEvent(page, "settings-changed", null);
    await expect(settingsNotice(page)).toHaveCount(0);
  });

  for (const colorScheme of ["light", "dark"] as const) {
    test(`知らせの画像を撮る_${colorScheme}`, async ({ page }) => {
      await page.setViewportSize({ width: 960, height: 600 });
      await page.emulateMedia({ colorScheme });

      // 1 行: 設定の知らせだけ
      await open(page, projectsSnapshot([APP]), { get_settings_notice: { ok: SETTINGS_NOTICE } });
      await expect(settingsNotice(page)).toBeVisible();
      await expect(stateNotice(page)).toHaveCount(0);
      await page.screenshot({ path: shotPath(`notice-1-line-${colorScheme}`) });

      // 2 行: 状態の知らせと設定の知らせ
      await emitEvent(page, "app-snapshot", projectsSnapshot([APP], { notice: STATE_NOTICE }));
      await expect(stateNotice(page)).toBeVisible();
      // 2 行は同じ地と文字の色(既存の知らせのトークン)
      const color = (l: Locator) => l.evaluate((el) => [getComputedStyle(el).backgroundColor, getComputedStyle(el).color]);
      expect(await color(settingsNotice(page))).toEqual(await color(stateNotice(page)));
      await page.screenshot({ path: shotPath(`notice-2-lines-${colorScheme}`) });
    });
  }
});
