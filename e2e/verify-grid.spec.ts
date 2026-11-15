// 9 枚の検証画面の画面の自動テスト(T22。設計書 §2.2「9 枚の簡易確認」「非表示のインスタンス」)。
//
// 偽の Rust 側(fixtures/tauriMock.ts)の get_launch_options で verifyGrid を有効にし、写しに 9 枚を載せる。
// 9 枚ぶんの偽のチャネルに出力を流し、「1 枚だけ表示」で隠した 8 枚にも write が届き続ける(処理済みの量が返る)ことと、
// xterm.js の実物で setVisible の往復(WebGL のアドオンの破棄と作り直し)が動くことを確かめる。
// M1 の検証用の画面のテストで、M4(T49)で画面と一緒に外す。

import { expect, test, type Locator, type Page } from "@playwright/test";

import type { LaunchOptions } from "../src/ipc/generated/LaunchOptions";
import {
  installTauriMock,
  parseChannelId,
  readCallArgs,
  sampleSnapshot,
  sampleTerminal,
  sendChannelBytes,
} from "./fixtures/tauriMock";

const IDS = Array.from({ length: 9 }, (_, i) => `terminal-${i + 1}`);
const DOM: LaunchOptions = { renderer: "dom", unicode: "unicode11", verifyGrid: true };
const WEBGL: LaunchOptions = { renderer: "webgl", unicode: "unicode11", verifyGrid: true };

/** 9 枚の検証画面を開き、9 枚の接続が届くまで待つ。ID → チャネルの番号を返す。 */
async function openGrid(page: Page, options: LaunchOptions): Promise<Map<string, number>> {
  await installTauriMock(page, {
    get_launch_options: { ok: options },
    get_app_snapshot: {
      ok: sampleSnapshot({ terminals: IDS.map((id, i) => sampleTerminal(id, { kind: "running" }, { label: `ターミナル ${i + 1}` })) }),
    },
  });
  await page.goto("/");
  await expect(page.getByTestId("verify-cell")).toHaveCount(9);
  await expect.poll(async () => (await readCallArgs(page, "connect_terminal")).length).toBe(9);
  const connects = await readCallArgs(page, "connect_terminal");
  return new Map(connects.map((c) => [c.terminalId, parseChannelId(c.onOutput)]));
}

const cell = (page: Page, id: string): Locator => page.locator(`[data-testid="verify-cell"][data-terminal-id="${id}"]`);
const toggle = (page: Page, id: string): Locator => cell(page, id).getByTestId("verify-cell-toggle");

async function ackedBytes(page: Page, id: string): Promise<number> {
  const acks = await readCallArgs(page, "ack_terminal_output");
  return acks.filter((a) => a.terminalId === id).reduce((sum, a) => sum + a.bytes, 0);
}

/** 9 枚それぞれに `text(id)` を流し、9 枚とも取り込みが済んだ量が追いつくまで待つ。 */
async function sendToAll(page: Page, channels: Map<string, number>, text: (id: string) => string): Promise<void> {
  const expected = new Map<string, number>();
  for (const id of IDS) {
    const before = await ackedBytes(page, id);
    const channel = channels.get(id);
    if (channel === undefined) throw new Error(`${id} のチャネルが無い`);
    expected.set(id, before + (await sendChannelBytes(page, channel, [text(id)])));
  }
  for (const id of IDS) {
    await expect.poll(() => ackedBytes(page, id), { message: `${id} の取り込み` }).toBe(expected.get(id));
  }
}

async function reportedErrors(page: Page): Promise<string[]> {
  return (await readCallArgs(page, "report_frontend_error")).map((r) => r.message);
}

/** 行・桁が 0 の寸法を送っていないこと。 */
async function expectNoZeroSizes(page: Page): Promise<void> {
  const sizes = [...(await readCallArgs(page, "connect_terminal")), ...(await readCallArgs(page, "resize_terminal"))];
  expect(sizes.filter((s) => s.rows <= 0 || s.cols <= 0)).toEqual([]);
}

/** そのマスの中の、WebGL2 のコンテキストを持つ canvas の数。 */
async function webglCanvasCount(page: Page, id: string): Promise<number> {
  return cell(page, id)
    .locator(".xterm-screen canvas")
    .evaluateAll((canvases) => canvases.filter((c) => (c as HTMLCanvasElement).getContext("webgl2") !== null).length);
}

test.describe("9 枚の検証画面(DOM)", () => {
  test.afterEach(async ({ page }) => {
    expect(await reportedErrors(page), "画面の失敗が Rust へ報告された").toEqual([]);
    await expectNoZeroSizes(page);
  });

  test("9 枚が 3 × 3 に並び、それぞれ 0 でない行・桁で接続し、見出しに番号と描画方式が出る", async ({ page }) => {
    const channels = await openGrid(page, DOM);

    expect([...channels.keys()].sort()).toEqual([...IDS].sort());
    for (const c of await readCallArgs(page, "connect_terminal")) {
      expect(c.rows, c.terminalId).toBeGreaterThan(0);
      expect(c.cols, c.terminalId).toBeGreaterThan(0);
    }
    const boxes = [];
    for (const [i, id] of IDS.entries()) {
      await expect(cell(page, id)).toBeVisible();
      await expect(cell(page, id).getByTestId("verify-cell-focus")).toHaveText(String(i + 1));
      await expect(cell(page, id).getByTestId("terminal-renderer")).toHaveText("dom");
      const box = await cell(page, id).getByTestId("terminal-host").boundingBox();
      if (box === null) throw new Error(`${id} の置き場所の大きさが取れない`);
      expect(box.width, id).toBeGreaterThan(0);
      expect(box.height, id).toBeGreaterThan(0);
      boxes.push(box);
    }
    // 3 列 × 3 行(左上から右へ順に並ぶ)。
    const distinct = (values: number[]) => new Set(values.map(Math.round)).size;
    expect(distinct(boxes.map((b) => b.x))).toBe(3);
    expect(distinct(boxes.map((b) => b.y))).toBe(3);
    expect(boxes[1]!.x).toBeGreaterThan(boxes[0]!.x);
    expect(boxes[3]!.y).toBeGreaterThan(boxes[0]!.y);

    // 番号を押すと、そのマスにフォーカスが移る。
    await cell(page, "terminal-5").getByTestId("verify-cell-focus").click();
    await expect(cell(page, "terminal-5").locator(".xterm-helper-textarea")).toBeFocused();
  });

  test("1 枚だけ表示で隠した 8 枚にも出力が届き続け、9 枚表示に戻すと隠していた間の文字が画面にある", async ({ page }) => {
    const channels = await openGrid(page, DOM);
    await sendToAll(page, channels, (id) => `before ${id}\r\n`);
    const gridBox = await cell(page, "terminal-1").getByTestId("terminal-host").boundingBox();

    await toggle(page, "terminal-1").click();
    for (const id of IDS.slice(1)) await expect(cell(page, id)).toBeHidden();
    await expect(cell(page, "terminal-1")).toBeVisible();
    await expect(toggle(page, "terminal-1")).toHaveText("9 枚表示に戻す");
    await expect(cell(page, "terminal-1").locator(".xterm-helper-textarea")).toBeFocused();
    // 表示中の 1 枚は全面に広がり、寸法を送り直す。
    await expect
      .poll(async () => (await readCallArgs(page, "resize_terminal")).filter((r) => r.terminalId === "terminal-1").length)
      .toBeGreaterThan(0);
    const singleBox = await cell(page, "terminal-1").getByTestId("terminal-host").boundingBox();
    expect(singleBox!.width).toBeGreaterThan(gridBox!.width * 2);

    // 隠している間に 9 枚とも出力を流す。隠した 8 枚も取り込みが済んだ量を返す。
    await sendToAll(page, channels, (id) => `hidden ${id}\r\n`);
    // 隠している間、隠した 8 枚の寸法は送らない。
    expect((await readCallArgs(page, "resize_terminal")).map((r) => r.terminalId)).not.toContain("terminal-2");

    await toggle(page, "terminal-1").click();
    for (const id of IDS) {
      await expect(cell(page, id)).toBeVisible();
      const rows = cell(page, id).locator(".xterm-rows > div");
      await expect(rows.nth(0), id).toHaveText(`before ${id}`);
      await expect(rows.nth(1), id).toHaveText(`hidden ${id}`);
    }
    await expect(toggle(page, "terminal-1")).toHaveText("1 枚だけ表示");
    // 戻したあとも 9 枚とも取り込みを続ける。
    await sendToAll(page, channels, (id) => `after ${id}\r\n`);
  });

  test("隠している間にウィンドウの大きさを変えると、戻したときに隠していた 8 枚もマスの大きさに合わせ直す", async ({ page }) => {
    await openGrid(page, DOM);
    const connects = new Map((await readCallArgs(page, "connect_terminal")).map((c) => [c.terminalId, c]));

    const singleResizes = async () =>
      (await readCallArgs(page, "resize_terminal")).filter((r) => r.terminalId === "terminal-1").length;
    await toggle(page, "terminal-1").click();
    await expect(cell(page, "terminal-2")).toBeHidden();
    // 全面に広がった寸法が届いてから、ウィンドウを小さくする。
    await expect.poll(singleResizes).toBeGreaterThan(0);
    const before = await singleResizes();
    const viewport = page.viewportSize();
    if (viewport === null) throw new Error("ビューポートの大きさが取れない");
    await page.setViewportSize({ width: Math.round(viewport.width / 2), height: Math.round(viewport.height / 2) });
    await expect.poll(singleResizes).toBeGreaterThan(before);
    expect((await readCallArgs(page, "resize_terminal")).map((r) => r.terminalId)).toEqual(
      expect.not.arrayContaining(IDS.slice(1)),
    );

    await toggle(page, "terminal-1").click();
    // 隠していた 8 枚は、小さくなったマスに合わせた寸法を送る。
    for (const id of IDS.slice(1)) {
      await expect
        .poll(async () => (await readCallArgs(page, "resize_terminal")).filter((r) => r.terminalId === id).length, {
          message: id,
        })
        .toBe(1);
      const resized = (await readCallArgs(page, "resize_terminal")).find((r) => r.terminalId === id);
      expect(resized!.cols, id).toBeLessThan(connects.get(id)!.cols);
      expect(resized!.rows, id).toBeLessThan(connects.get(id)!.rows);
    }
  });
});

test.describe("9 枚の検証画面(WebGL)", () => {
  test.afterEach(async ({ page }) => {
    expect(await reportedErrors(page), "画面の失敗が Rust へ報告された").toEqual([]);
    await expectNoZeroSizes(page);
  });

  test("9 枚とも WebGL のコンテキストを作って描き、見出しが webgl になる", async ({ page }) => {
    const channels = await openGrid(page, WEBGL);

    for (const id of IDS) {
      await expect(cell(page, id).getByTestId("terminal-renderer"), id).toHaveText("webgl");
      expect(await webglCanvasCount(page, id), id).toBe(1);
      await expect(cell(page, id).locator(".xterm-rows > div"), id).toHaveCount(0);
    }
    await sendToAll(page, channels, (id) => `webgl ${id}\r\n`);
  });

  test("1 枚だけ表示で隠した 8 枚は WebGL のコンテキストを手放し、戻すと作り直す(DOM 戻りとして報告しない)", async ({ page }) => {
    const channels = await openGrid(page, WEBGL);
    for (const id of IDS) await expect(cell(page, id).getByTestId("terminal-renderer")).toHaveText("webgl");
    // 最初の canvas に印を付けて、戻したあとに新しく作られたかを見分ける。
    await page.locator(".xterm-screen canvas").evaluateAll((canvases) => {
      for (const c of canvases) c.setAttribute("data-e2e-first", "1");
    });

    await toggle(page, "terminal-1").click();
    for (const id of IDS.slice(1)) {
      await expect(cell(page, id)).toBeHidden();
      // アドオンを破棄すると canvas が外れる。
      await expect(cell(page, id).locator(".xterm-screen canvas"), id).toHaveCount(0);
    }
    // 表示中の 1 枚は WebGL のまま。
    await expect(cell(page, "terminal-1").getByTestId("terminal-renderer")).toHaveText("webgl");
    expect(await webglCanvasCount(page, "terminal-1")).toBe(1);
    await sendToAll(page, channels, (id) => `hidden ${id}\r\n`);

    await toggle(page, "terminal-1").click();
    for (const id of IDS) {
      await expect(cell(page, id)).toBeVisible();
      await expect(cell(page, id).getByTestId("terminal-renderer"), id).toHaveText("webgl");
      await expect.poll(() => webglCanvasCount(page, id), { message: id }).toBe(1);
      await expect(cell(page, id).locator(".xterm-rows > div"), id).toHaveCount(0);
    }
    // 隠していた 8 枚の canvas は作り直したもの。表示し続けた 1 枚は同じもの。
    for (const id of IDS.slice(1)) {
      await expect(cell(page, id).locator(".xterm-screen canvas[data-e2e-first]"), id).toHaveCount(0);
    }
    await expect(cell(page, "terminal-1").locator(".xterm-screen canvas[data-e2e-first]").first()).toBeAttached();
    await sendToAll(page, channels, (id) => `after ${id}\r\n`);

    // 往復をもう 1 回しても同じ(作り直しが 1 回きりでない)。
    await toggle(page, "terminal-5").click();
    await expect(cell(page, "terminal-1").locator(".xterm-screen canvas")).toHaveCount(0);
    await toggle(page, "terminal-5").click();
    for (const id of IDS) {
      await expect(cell(page, id).getByTestId("terminal-renderer"), id).toHaveText("webgl");
      await expect.poll(() => webglCanvasCount(page, id), { message: id }).toBe(1);
    }
  });
});
