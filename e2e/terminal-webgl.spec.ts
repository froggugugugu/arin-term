// WebGL 描画の画面の自動テスト(T21。設計書 §2.2「描画方式」)。
//
// 偽の Rust 側(fixtures/tauriMock.ts)の get_launch_options で `webgl` を返し、xterm.js の実物を WebGL で描かせる。
// Playwright の WebKit(ヘッドレス)は WebGL2 のコンテキストを作れる(2026-11-15 に確認。T21 の報告)。
//
// WebGL では文字が canvas に描かれ、DOM 描画の行の要素(.xterm-rows)が無いので、表示の中身(文字・色・幅)は読めない。
// ここでは描画方式に依存しないこと(入力・寸法・処理済みの量・選択とコピー)と、WebGL を作れない・失ったときに
// DOM に戻して報告することを確かめる。見た目の比較(にじみ・罫線)は人間が実機で行う(T25・T26)。

import { expect, test, type Page } from "@playwright/test";

import type { LaunchOptions } from "../src/ipc/generated/LaunchOptions";
import {
  installTauriMock,
  type OutputChunk,
  parseChannelId,
  readCallArgs,
  type RecordedArgs,
  sampleSnapshot,
  sampleTerminal,
  sendChannelBytes,
} from "./fixtures/tauriMock";

const TERMINAL_ID = "t-e2e-webgl";
const WEBGL: LaunchOptions = { renderer: "webgl", unicode: "unicode11", verifyGrid: false };

interface StartedTerminal {
  channelId: number;
  connect: RecordedArgs<"connect_terminal">;
}

/** 起動時の指定を付けてターミナルを 1 枚開き、最初の接続が届くまで待つ。 */
async function openTerminal(page: Page, options: LaunchOptions = WEBGL): Promise<StartedTerminal> {
  await installTauriMock(page, {
    get_launch_options: { ok: options },
    get_app_snapshot: { ok: sampleSnapshot({ terminals: [sampleTerminal(TERMINAL_ID, { kind: "running" })] }) },
  });
  await page.goto("/");
  await expect(page.getByTestId("terminal-host")).toBeVisible();
  await expect.poll(async () => (await readCallArgs(page, "connect_terminal")).length).toBe(1);
  const [connect] = await readCallArgs(page, "connect_terminal");
  if (connect === undefined) throw new Error("connect_terminal が届いていない");
  return { channelId: parseChannelId(connect.onOutput), connect };
}

async function ackedBytes(page: Page): Promise<number> {
  const acks = await readCallArgs(page, "ack_terminal_output");
  return acks.reduce((sum, ack) => sum + ack.bytes, 0);
}

async function sendAndWait(page: Page, channelId: number, chunks: readonly OutputChunk[]): Promise<number> {
  const before = await ackedBytes(page);
  const sent = await sendChannelBytes(page, channelId, chunks);
  await expect.poll(() => ackedBytes(page)).toBe(before + sent);
  return sent;
}

async function reportedErrors(page: Page): Promise<string[]> {
  return (await readCallArgs(page, "report_frontend_error")).map((r) => r.message);
}

/** フォーカスのある要素へ copy のイベントを発火し、書かれた text/plain を返す(terminal.spec.ts と同じ形)。 */
async function fireCopy(page: Page): Promise<{ text: string; prevented: boolean }> {
  return page.evaluate(() => {
    const data = new DataTransfer();
    const event = new ClipboardEvent("copy", { clipboardData: data, bubbles: true, cancelable: true });
    (document.activeElement ?? document.body).dispatchEvent(event);
    return { text: data.getData("text/plain"), prevented: event.defaultPrevented };
  });
}

/** 描画用の canvas(WebGL)と、DOM 描画の行の要素。 */
const webglCanvas = (page: Page) => page.getByTestId("terminal-host").locator(".xterm-screen canvas");
const domRows = (page: Page) => page.getByTestId("terminal-host").locator(".xterm-rows > div");

test.describe("WebGL の指定", () => {
  test.afterEach(async ({ page }) => {
    expect(await reportedErrors(page), "画面の失敗が Rust へ報告された").toEqual([]);
  });

  test("WebGL で描き、描画方式の表示が webgl になり、DOM 描画の行は作られない", async ({ page }) => {
    const { channelId } = await openTerminal(page);

    await expect(page.getByTestId("terminal-renderer")).toHaveText("webgl");
    await expect(webglCanvas(page).first()).toBeVisible();
    // 作った canvas のどれかが WebGL2 のコンテキストを持っている。
    const hasWebgl2 = await webglCanvas(page).evaluateAll((canvases) =>
      canvases.some((c) => (c as HTMLCanvasElement).getContext("webgl2") !== null),
    );
    expect(hasWebgl2).toBe(true);

    await sendAndWait(page, channelId, ["hello \x1b[31mRED\x1b[0m 日本語\r\n"]);
    await expect(domRows(page)).toHaveCount(0);
  });

  test("キー入力が write_terminal に届く", async ({ page }) => {
    const { channelId } = await openTerminal(page);
    await sendAndWait(page, channelId, ["$ "]);

    await page.getByTestId("terminal-host").locator(".xterm-screen").click();
    await expect(page.locator(".xterm-helper-textarea")).toBeFocused();
    await page.keyboard.type("echo ok");
    await page.keyboard.press("Enter");

    await expect
      .poll(async () => (await readCallArgs(page, "write_terminal")).map((w) => w.data).join(""))
      .toBe("echo ok\r");
  });

  test("最初の接続の行・桁は 0 でなく、ウィンドウの大きさを変えると resize_terminal が届く", async ({ page }) => {
    const { connect } = await openTerminal(page);
    expect(connect.rows).toBeGreaterThan(0);
    expect(connect.cols).toBeGreaterThan(0);

    const viewport = page.viewportSize();
    if (viewport === null) throw new Error("ビューポートの大きさが取れない");
    await page.setViewportSize({ width: Math.round(viewport.width / 2), height: Math.round(viewport.height / 2) });

    await expect.poll(async () => (await readCallArgs(page, "resize_terminal")).length).toBeGreaterThan(0);
    const last = (await readCallArgs(page, "resize_terminal")).at(-1);
    if (last === undefined) throw new Error("resize_terminal が届いていない");
    expect(last.rows).toBeGreaterThan(0);
    expect(last.rows).toBeLessThan(connect.rows);
    expect(last.cols).toBeLessThan(connect.cols);
  });

  test("取り込みが済んだ量の合計が、流したバイト数と同じになる(文字の途中で切れた出力と大量の行を含む)", async ({ page }) => {
    const { channelId } = await openTerminal(page);
    const utf8 = Array.from(new TextEncoder().encode("日本"));
    const lines = Array.from({ length: 2_000 }, (_, i) => `line-${i}\r\n`).join("");

    const sent = await sendAndWait(page, channelId, ["abc ", utf8.slice(0, 2), utf8.slice(2), lines]);

    expect(sent).toBe(4 + 6 + lines.length);
  });

  test("流した文字を選択してコピーすると、その文字列が取れる(バッファの確認)", async ({ page }) => {
    const { channelId } = await openTerminal(page);
    await sendAndWait(page, channelId, ["hello world\r\n", "$ "]);

    // 1 行目の先頭の語をダブルクリックで選ぶ(WebGL では行の要素が無いので、画面の左上からの位置で指す)。
    const box = await page.getByTestId("terminal-host").locator(".xterm-screen").boundingBox();
    if (box === null) throw new Error("画面の大きさが取れない");
    await page.mouse.dblclick(box.x + 4, box.y + 6);
    await expect(page.locator(".xterm-helper-textarea")).toBeFocused();

    expect(await fireCopy(page)).toEqual({ text: "hello", prevented: true });
  });
});

test("既定(dom の指定)では DOM で描き、描画方式の表示が dom になる", async ({ page }) => {
  const { channelId } = await openTerminal(page, { renderer: "dom", unicode: "unicode11", verifyGrid: false });

  await expect(page.getByTestId("terminal-renderer")).toHaveText("dom");
  await sendAndWait(page, channelId, ["dom\r\n"]);
  await expect(domRows(page).first()).toHaveText("dom");
  expect(await reportedErrors(page)).toEqual([]);
});

test("WebGL2 のコンテキストを作れなければ DOM に戻し、表示が dom になり、診断に 1 行だけ出る", async ({ page }) => {
  // WebGL2 を持たない WebView の真似。画面のスクリプトより先に、webgl2 のコンテキストを作れなくする。
  await page.addInitScript(() => {
    const original = HTMLCanvasElement.prototype.getContext;
    HTMLCanvasElement.prototype.getContext = function (this: HTMLCanvasElement, type: string, ...rest: unknown[]) {
      if (type === "webgl2") return null;
      return (original as (...args: unknown[]) => unknown).call(this, type, ...rest);
    } as typeof original;
  });
  const { channelId } = await openTerminal(page);

  await expect(page.getByTestId("terminal-renderer")).toHaveText("dom");
  await sendAndWait(page, channelId, ["fallback\r\n"]);
  await expect(domRows(page).first()).toHaveText("fallback");
  const reported = await reportedErrors(page);
  expect(reported).toHaveLength(1);
  expect(reported[0]).toMatch(
    new RegExp(`^ターミナル ${TERMINAL_ID}: WebGL の描画を作れなかった(.*WebGL2 not supported.*)ので、DOM の描画に戻した$`),
  );
});

test("WebGL のコンテキストを失ったら DOM に戻し、表示が dom になり、診断に 1 行だけ出る", async ({ page }) => {
  const { channelId } = await openTerminal(page);
  await expect(page.getByTestId("terminal-renderer")).toHaveText("webgl");

  // GPU のリセットなどでコンテキストを失った場合の真似。xterm.js は復帰を 3 秒待ってから喪失を知らせる。
  const lost = await webglCanvas(page).evaluateAll((canvases) => {
    for (const c of canvases) {
      const ext = (c as HTMLCanvasElement).getContext("webgl2")?.getExtension("WEBGL_lose_context");
      if (ext) {
        ext.loseContext();
        return true;
      }
    }
    return false;
  });
  expect(lost).toBe(true);

  await expect(page.getByTestId("terminal-renderer")).toHaveText("dom", { timeout: 10_000 });
  await sendAndWait(page, channelId, ["after loss\r\n"]);
  await expect(domRows(page).first()).toHaveText("after loss");
  expect(await reportedErrors(page)).toEqual([
    `ターミナル ${TERMINAL_ID}: WebGL の描画コンテキストを失ったので、DOM の描画に戻した`,
  ]);
});
