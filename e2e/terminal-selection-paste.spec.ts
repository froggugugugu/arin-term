// ターミナルの選択を真ん中クリックで貼り付け、右クリックのメニューでコピー・ペーストする(ADR-020)。
//
// xterm.js の実物を Vite の画面で動かし、偽の Rust 側(fixtures/tauriMock.ts)を通す。本物のクリップボードは
// 使わない(write_clipboard_text・read_clipboard_text の呼び出しと応答で確かめる)。

import { expect, type Page, test } from "@playwright/test";

import type { AppSnapshot } from "../src/ipc/generated/AppSnapshot";
import {
  emitEvent,
  installTauriMock,
  type OutputChunk,
  parseChannelId,
  projectSnapshot,
  readCallArgs,
  sampleTerminal,
  sendChannelBytes,
  setMockReply,
} from "./fixtures/tauriMock";

const TERMINAL_ID = "t-e2e-1";

async function openTerminal(page: Page, overrides: Partial<AppSnapshot> = {}): Promise<number> {
  const snapshot = projectSnapshot([sampleTerminal(TERMINAL_ID, { kind: "running" })], overrides);
  await installTauriMock(page, { get_app_snapshot: { ok: snapshot } });
  await page.goto("/");
  await expect(page.getByTestId("terminal-host")).toBeVisible();
  await expect.poll(async () => (await readCallArgs(page, "connect_terminal")).length).toBe(1);
  const [connect] = await readCallArgs(page, "connect_terminal");
  if (connect === undefined) throw new Error("connect_terminal が届いていない");
  return parseChannelId(connect.onOutput);
}

async function ackedBytes(page: Page): Promise<number> {
  return (await readCallArgs(page, "ack_terminal_output")).reduce((sum, ack) => sum + ack.bytes, 0);
}

async function sendAndWait(page: Page, channelId: number, chunks: readonly OutputChunk[]): Promise<void> {
  const before = await ackedBytes(page);
  const sent = await sendChannelBytes(page, channelId, chunks);
  await expect.poll(() => ackedBytes(page)).toBe(before + sent);
}

const rows = (page: Page) => page.getByTestId("terminal-host").locator(".xterm-rows > div");

async function writtenInput(page: Page): Promise<string> {
  return (await readCallArgs(page, "write_terminal")).map((w) => w.data).join("");
}

/** 1 行目の先頭の語の上の位置。 */
async function firstWord(page: Page): Promise<{ x: number; y: number }> {
  const box = await rows(page).nth(0).boundingBox();
  if (box === null) throw new Error("1 行目の大きさが取れない");
  return { x: box.x + 4, y: box.y + box.height / 2 };
}

/** 1 行目に hello world を出し、先頭の語をダブルクリックで選ぶ。 */
async function selectHello(page: Page, channelId: number): Promise<void> {
  await sendAndWait(page, channelId, ["hello world\r\n", "$ "]);
  await expect(rows(page).nth(0)).toHaveText("hello world");
  const { x, y } = await firstWord(page);
  await page.mouse.dblclick(x, y);
  await expect(page.getByTestId("terminal-host").locator(".xterm-selection > div").first()).toBeVisible();
}

/** 2 行目(プロンプトの行)を真ん中クリックする。 */
async function middleClick(page: Page): Promise<void> {
  const box = await rows(page).nth(1).boundingBox();
  if (box === null) throw new Error("2 行目の大きさが取れない");
  await page.mouse.click(box.x + 40, box.y + box.height / 2, { button: "middle" });
}

const menu = (page: Page) => page.getByRole("menu", { name: "ターミナル" });
const menuItem = (page: Page, name: string) => menu(page).getByRole("menuitem", { name, exact: true });

test.afterEach(async ({ page }) => {
  const reported = await readCallArgs(page, "report_frontend_error");
  expect(reported.map((r) => r.message), "画面の失敗が Rust へ報告された").toEqual([]);
});

test("選んだ文字を真ん中クリックで貼り、クリップボードには書かない", async ({ page }) => {
  const channelId = await openTerminal(page);
  await selectHello(page, channelId);

  await middleClick(page);

  await expect.poll(() => writtenInput(page)).toBe("hello");
  expect(await readCallArgs(page, "write_clipboard_text")).toEqual([]);
});

test("マウスを使うアプリが動いているあいだは真ん中クリックを渡し、Option を押していれば貼る", async ({ page }) => {
  const channelId = await openTerminal(page);
  await selectHello(page, channelId);
  // アプリがマウスの報告(DECSET 1000)を有効にした。
  await sendAndWait(page, channelId, ["\x1b[?1000h"]);

  await middleClick(page);
  // xterm.js がアプリへ真ん中ボタンの報告を送る(貼り付けではない)。
  // (X10 の形式の報告は onBinary で届くので、文字列とバイト列の両方を数える。)
  await expect
    .poll(
      async () =>
        (await readCallArgs(page, "write_terminal")).length + (await readCallArgs(page, "write_terminal_bytes")).length,
    )
    .toBeGreaterThan(0);
  expect(await writtenInput(page)).not.toContain("hello");

  // Option なしの右クリックもアプリに渡す: メニューを出さず、語も選ばない(持っている選択は hello のまま)。
  const { x, y } = await firstWord(page);
  await page.mouse.click(x + 60, y, { button: "right" });
  await expect(menu(page)).toHaveCount(0);

  await page.keyboard.down("Alt");
  await middleClick(page);
  await page.keyboard.up("Alt");
  await expect.poll(() => writtenInput(page)).toContain("hello");
  expect(await writtenInput(page)).not.toContain("world");
});

test("設定が OFF なら真ん中クリックで貼らない", async ({ page }) => {
  const channelId = await openTerminal(page, { middleClickPaste: false });
  await selectHello(page, channelId);

  await middleClick(page);

  // 届くのを待つ相手が無いので、少し後に何も送っていないことを見る。
  await page.waitForTimeout(200);
  expect(await writtenInput(page)).toBe("");
});

test("右クリックのメニューのコピーは選択をクリップボードに書き、選択が無ければ薄く出す", async ({ page }) => {
  const channelId = await openTerminal(page);
  await sendAndWait(page, channelId, ["hello world\r\n", "$ "]);

  // 空いた場所を右クリック: 選択が無いので「コピー」は選べない。
  const box = await rows(page).nth(3).boundingBox();
  if (box === null) throw new Error("4 行目の大きさが取れない");
  await page.mouse.click(box.x + 200, box.y + box.height / 2, { button: "right" });
  await expect(menu(page).getByRole("menuitem")).toHaveText(["コピー", "ペースト", "すべてを選択"]);
  await expect(menuItem(page, "コピー")).toHaveAttribute("aria-disabled", "true");
  await page.keyboard.press("Escape");
  await expect(menu(page)).toHaveCount(0);

  // 語の上を右クリックすると、その語が選ばれ、コピーできる。
  const { x, y } = await firstWord(page);
  await page.mouse.click(x, y, { button: "right" });
  await menuItem(page, "コピー").click();

  await expect.poll(() => readCallArgs(page, "write_clipboard_text")).toEqual([{ text: "hello" }]);
  await expect(menu(page)).toHaveCount(0);
  await expect(page.locator(".xterm-helper-textarea")).toBeFocused();
});

test("右クリックのメニューのペーストはクリップボードの文字列を貼る。すべてを選択も効く", async ({ page }) => {
  const channelId = await openTerminal(page);
  await sendAndWait(page, channelId, ["hello world\r\n", "$ "]);
  await setMockReply(page, "read_clipboard_text", { ok: "npm test" });

  const { x, y } = await firstWord(page);
  await page.mouse.click(x + 200, y, { button: "right" });
  await menuItem(page, "ペースト").click();
  await expect.poll(() => writtenInput(page)).toBe("npm test");

  await page.mouse.click(x + 200, y, { button: "right" });
  await menuItem(page, "すべてを選択").click();
  await expect(page.getByTestId("terminal-host").locator(".xterm-selection > div").first()).toBeVisible();
});

test("設定画面の_選択して真ん中クリックで貼り付け_は既定でオン_押すと_set_middle_click_paste_を送る", async ({ page }) => {
  await openTerminal(page);
  await emitEvent(page, "menu-command", "open-settings");
  await expect(page.getByTestId("settings-dialog")).toBeVisible();

  const toggle = page.getByTestId("middle-click-paste-switch");
  await expect(toggle).toHaveAttribute("aria-checked", "true");
  await toggle.click();
  await expect.poll(() => readCallArgs(page, "set_middle_click_paste")).toEqual([{ enabled: false }]);
});
