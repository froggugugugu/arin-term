// WebGL 描画の画面の自動テスト(T21。設計書 §2.2「描画方式」)。
//
// 偽の Rust 側(fixtures/tauriMock.ts)の get_launch_options で `webgl` を返し、xterm.js の実物を WebGL で描かせる。
// Playwright の WebKit(ヘッドレス)は WebGL2 のコンテキストを作れる(2026-11-15 に確認。T21 の報告)。
//
// WebGL では文字が canvas に描かれ、DOM 描画の行の要素(.xterm-rows)が無いので、表示の中身(文字・色・幅)は読めない。
// ここでは描画方式に依存しないこと(入力・寸法・処理済みの量・選択とコピー)と、WebGL を作れない・失ったときに
// DOM に戻して報告することを確かめる。見た目の比較(にじみ・罫線)は人間が実機で行う(T25・T26)。
// タイル表示(M4)では、9 枚とも WebGL のコンテキストを作れることと、ページ送りで隠したタイルがコンテキストを手放し、
// 戻すと作り直すこと(DOM 戻りとして報告しない)を確かめる(M1 の検証画面のテストから引き継いだ。T49c)。

import { expect, test, type Page } from "@playwright/test";

import type { LaunchOptions } from "../src/ipc/generated/LaunchOptions";
import {
  emitEvent,
  installTauriMock,
  type OutputChunk,
  parseChannelId,
  projectsSnapshot,
  readCallArgs,
  readRecordedCalls,
  type RecordedArgs,
  projectSnapshot,
  type SampleProject,
  sampleTerminal,
  sendChannelBytes,
  withTileDisplay,
} from "./fixtures/tauriMock";

const TERMINAL_ID = "t-e2e-webgl";
const WEBGL: LaunchOptions = { renderer: "webgl", unicode: "unicode11" };

interface StartedTerminal {
  channelId: number;
  connect: RecordedArgs<"connect_terminal">;
}

/** 起動時の指定を付けてターミナルを 1 枚開き、最初の接続が届くまで待つ。 */
async function openTerminal(page: Page, options: LaunchOptions = WEBGL): Promise<StartedTerminal> {
  await installTauriMock(page, {
    get_launch_options: { ok: options },
    get_app_snapshot: { ok: projectSnapshot([sampleTerminal(TERMINAL_ID, { kind: "running" })]) },
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
  const { channelId } = await openTerminal(page, { renderer: "dom", unicode: "unicode11" });

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

test.describe("タイル表示の WebGL(NFR-009。M1 の検証画面のテストから引き継ぎ。T49c)", () => {
  /** 1 枚ずつタブを持つプロジェクトを n 件。ターミナルの ID は `p<番号>-t1`。 */
  const projects = (n: number): SampleProject[] =>
    Array.from({ length: n }, (_, i) => ({ id: `p${i + 1}`, name: `proj${i + 1}`, tabs: ["ターミナル 1"] }));
  const idsOf = (list: readonly SampleProject[]) => list.map((p) => `${p.id}-t1`);
  const hostOf = (page: Page, id: string) => page.locator(`[data-testid="terminal-host"][data-terminal-id="${id}"]`);

  /** タイル表示の写しで開き、準備完了と、渡した ID の接続が届くまで待つ。ID → チャネルの番号を返す。 */
  async function openTiles(page: Page, snapshot: ReturnType<typeof withTileDisplay>, ids: readonly string[]) {
    await installTauriMock(page, { get_launch_options: { ok: WEBGL }, get_app_snapshot: { ok: snapshot } });
    await page.goto("/");
    await expect.poll(async () => (await readRecordedCalls(page)).map((c) => c.command)).toContain("notify_app_ready");
    await expect
      .poll(async () => new Set((await readCallArgs(page, "connect_terminal")).map((c) => c.terminalId)).size)
      .toBeGreaterThanOrEqual(ids.length);
    const connects = await readCallArgs(page, "connect_terminal");
    return new Map(connects.map((c) => [c.terminalId, parseChannelId(c.onOutput)]));
  }

  /** その置き場所の中の、WebGL2 のコンテキストを持つ canvas の数。 */
  const webglCount = (page: Page, id: string) =>
    hostOf(page, id)
      .locator(".xterm-screen canvas")
      .evaluateAll((canvases) => canvases.filter((c) => (c as HTMLCanvasElement).getContext("webgl2") !== null).length);

  async function ackedOf(page: Page, id: string): Promise<number> {
    const acks = await readCallArgs(page, "ack_terminal_output");
    return acks.filter((a) => a.terminalId === id).reduce((sum, a) => sum + a.bytes, 0);
  }

  /** それぞれに流し、取り込みが済んだ量が追いつくまで待つ。 */
  async function sendToEach(page: Page, channels: Map<string, number>, ids: readonly string[], text: (id: string) => string) {
    for (const id of ids) {
      const before = await ackedOf(page, id);
      const sent = await sendChannelBytes(page, channels.get(id) ?? -1, [text(id)]);
      await expect.poll(() => ackedOf(page, id), { message: id }).toBe(before + sent);
    }
  }

  test.afterEach(async ({ page }) => {
    expect(await reportedErrors(page), "画面の失敗(DOM 戻りを含む)が Rust へ報告された").toEqual([]);
    const sizes = [...(await readCallArgs(page, "connect_terminal")), ...(await readCallArgs(page, "resize_terminal"))];
    expect(sizes.filter((s) => s.rows <= 0 || s.cols <= 0), "行・桁が 0 の寸法を送った").toEqual([]);
  });

  test("タイル表示の_9_枚とも_WebGL_のコンテキストを作って描き_DOM_描画の行は作られない", async ({ page }) => {
    const list = projects(9);
    const ids = idsOf(list);
    const channels = await openTiles(page, withTileDisplay(projectsSnapshot(list)), ids);

    await expect(page.getByTestId("tile")).toHaveCount(9);
    for (const id of ids) {
      await expect.poll(() => webglCount(page, id), { message: id }).toBe(1);
      await expect(hostOf(page, id).locator(".xterm-rows > div"), id).toHaveCount(0);
    }
    await sendToEach(page, channels, ids, (id) => `webgl ${id}\r\n`);
  });

  test("ページ送りで隠したタイルは_WebGL_のコンテキストを手放し_戻すと作り直す_DOM_戻りとして報告しない", async ({ page }) => {
    // 10 件: 1 ページ目に 9 枚、2 ページ目に 1 枚。
    const list = projects(10);
    const ids = idsOf(list);
    const first = ids.slice(0, 9);
    const channels = await openTiles(page, withTileDisplay(projectsSnapshot(list)), ids);
    for (const id of first) await expect.poll(() => webglCount(page, id), { message: id }).toBe(1);
    // 1 ページ目の canvas に印を付け、描画の要素(.xterm-screen)を覚えておく(隠すと DOM の外へ出るので、要素で追う)。
    await page.evaluate((list) => {
      const screens: Record<string, Element> = {};
      for (const id of list) {
        const screen = document.querySelector(`[data-testid="terminal-host"][data-terminal-id="${id}"] .xterm-screen`);
        if (screen === null) throw new Error(`${id} の描画の要素が無い`);
        for (const c of screen.querySelectorAll("canvas")) c.setAttribute("data-e2e-first", "1");
        screens[id] = screen;
      }
      (window as unknown as { __E2E_SCREENS__: Record<string, Element> }).__E2E_SCREENS__ = screens;
    }, first);
    const canvasesInHidden = () =>
      page.evaluate(() => {
        const screens = (window as unknown as { __E2E_SCREENS__: Record<string, Element> }).__E2E_SCREENS__;
        return Object.fromEntries(Object.entries(screens).map(([id, s]) => [id, s.querySelectorAll("canvas").length]));
      });

    // 2 ページ目へ: 1 ページ目の 9 枚は隠れてアドオンを破棄し(canvas が外れる)、2 ページ目の 1 枚は WebGL で描く。
    await emitEvent(page, "app-snapshot", withTileDisplay(projectsSnapshot(list), { page: 1 }));
    await expect(page.getByTestId("tile")).toHaveCount(1);
    await expect.poll(() => webglCount(page, "p10-t1")).toBe(1);
    await expect.poll(canvasesInHidden).toEqual(Object.fromEntries(first.map((id) => [id, 0])));
    // 隠しているあいだも取り込みを続ける。
    await sendToEach(page, channels, first, (id) => `hidden ${id}\r\n`);

    // 1 ページ目へ戻す: 9 枚とも WebGL のコンテキストを作り直す(印の付いた古い canvas は無い)。DOM には戻さない。
    await emitEvent(page, "app-snapshot", withTileDisplay(projectsSnapshot(list), { page: 0 }));
    await expect(page.getByTestId("tile")).toHaveCount(9);
    for (const id of first) {
      await expect.poll(() => webglCount(page, id), { message: id }).toBe(1);
      await expect(hostOf(page, id).locator(".xterm-screen canvas[data-e2e-first]"), id).toHaveCount(0);
      await expect(hostOf(page, id).locator(".xterm-rows > div"), id).toHaveCount(0);
    }
    await sendToEach(page, channels, first, (id) => `after ${id}\r\n`);

    // 往復をもう 1 回しても同じ(作り直しが 1 回きりでない)。
    await emitEvent(page, "app-snapshot", withTileDisplay(projectsSnapshot(list), { page: 1 }));
    await expect.poll(() => webglCount(page, "p10-t1")).toBe(1);
    await expect.poll(canvasesInHidden).toEqual(Object.fromEntries(first.map((id) => [id, 0])));
    await emitEvent(page, "app-snapshot", withTileDisplay(projectsSnapshot(list), { page: 0 }));
    for (const id of first) await expect.poll(() => webglCount(page, id), { message: id }).toBe(1);
    // 接続は作り直さない。
    const connects = await readCallArgs(page, "connect_terminal");
    for (const id of ids) expect(connects.filter((c) => c.terminalId === id), id).toHaveLength(1);
  });
});
