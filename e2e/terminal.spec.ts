// ターミナルの画面の自動テスト(T19。FR-043、設計書 §10.3、ADR-006)。
//
// xterm.js の実物を Vite の画面で動かし、偽の Rust 側(fixtures/tauriMock.ts)のチャネルから出力を流す。
// 画面は本物の経路(main.tsx → TerminalManager → createTauriIpc → @tauri-apps/api)を通り、
// 最後の __TAURI_INTERNALS__ だけが偽物になる。製品のソースにテスト用の口は置かない。
//
// 表示は xterm.js の DOM 描画の要素(.xterm-rows の行)から読む。これは xterm.js 6.0.0 の内部の構造なので、
// xterm.js を上げたときや WebGL 描画(T21)では読み方を見直す。

import { expect, test, type Locator, type Page } from "@playwright/test";

import type { TerminalStatus } from "../src/ipc/generated/TerminalStatus";
import {
  emitEvent,
  installTauriMock,
  type OutputChunk,
  parseChannelId,
  readCallArgs,
  type RecordedArgs,
  sampleSnapshot,
  sendChannelBytes,
} from "./fixtures/tauriMock";

const TERMINAL_ID = "t-e2e-1";
/** 製品のスクロールバックの行数(src/terminal/xterm/XtermEngine.ts の SCROLLBACK_LINES。FR-043)。 */
const SCROLLBACK_LINES = 10_000;

function snapshotWith(status: TerminalStatus) {
  return sampleSnapshot({ terminals: [{ id: TERMINAL_ID, status }] });
}

interface StartedTerminal {
  channelId: number;
  connect: RecordedArgs<"connect_terminal">;
}

/** 写しにターミナルを 1 枚載せて画面を開き、最初の接続が届くまで待つ。 */
async function openTerminal(page: Page, status: TerminalStatus = { kind: "running" }): Promise<StartedTerminal> {
  await installTauriMock(page, { get_app_snapshot: { ok: snapshotWith(status) } });
  await page.goto("/");
  await expect(page.getByTestId("terminal-host")).toBeVisible();
  await expect.poll(async () => (await readCallArgs(page, "connect_terminal")).length).toBe(1);
  const [connect] = await readCallArgs(page, "connect_terminal");
  if (connect === undefined) throw new Error("connect_terminal が届いていない");
  return { channelId: parseChannelId(connect.onOutput), connect };
}

/** xterm.js が描いている画面の行(DOM 描画。表示中の行だけで、行数は端末の rows と同じ)。 */
function screenRows(page: Page): Locator {
  return page.getByTestId("terminal-host").locator(".xterm-rows > div");
}

/** 取り込みが済んだと Rust へ返した量の合計。 */
async function ackedBytes(page: Page): Promise<number> {
  const acks = await readCallArgs(page, "ack_terminal_output");
  return acks.reduce((sum, ack) => sum + ack.bytes, 0);
}

/** 出力を流し、取り込みが済んだ量が流した量に追いつくまで待つ(xterm.js のパーサーが全部を読み終えた印)。 */
async function sendAndWait(page: Page, channelId: number, chunks: readonly OutputChunk[]): Promise<number> {
  const before = await ackedBytes(page);
  const sent = await sendChannelBytes(page, channelId, chunks);
  await expect.poll(() => ackedBytes(page)).toBe(before + sent);
  return sent;
}

// 各テストの終わりに、画面の失敗(xterm.js の例外、未処理の失敗、通信の失敗の報告)が無かったことを見る。
test.afterEach(async ({ page }) => {
  const reported = await readCallArgs(page, "report_frontend_error");
  expect(reported.map((r) => r.message), "画面の失敗が Rust へ報告された").toEqual([]);
});

test("偽のチャネルから流した文字が画面に出る(色付きの文字と全角の文字を含む)", async ({ page }) => {
  const { channelId } = await openTerminal(page);

  await sendAndWait(page, channelId, ["hello ", "\x1b[31mRED\x1b[0m 日本語X\r\n", "second line"]);

  const rows = screenRows(page);
  await expect(rows.nth(0)).toHaveText("hello RED 日本語X");
  await expect(rows.nth(1)).toHaveText("second line");

  // 色: SGR 31(赤)は xterm.js の DOM 描画では前景色 1 の class になる。
  await expect(rows.nth(0).locator("span", { hasText: "RED" })).toHaveClass(/\bxterm-fg-1\b/);
  await expect(rows.nth(0).locator("span", { hasText: "hello" })).not.toHaveClass(/\bxterm-fg-/);

  // 全角: 3 文字が 6 桁ぶんの幅を占める(半角 1 文字の幅は "X" の幅で測る)。
  const wide = await rows.nth(0).locator("span", { hasText: "日本語" }).boundingBox();
  const narrow = await rows.nth(0).locator("span", { hasText: /^X$/ }).boundingBox();
  if (wide === null || narrow === null) throw new Error("文字の要素の大きさが取れない");
  expect(narrow.width).toBeGreaterThan(0);
  expect(wide.width / narrow.width).toBeCloseTo(6, 1);
});

test("キー入力が write_terminal に届く", async ({ page }) => {
  const { channelId } = await openTerminal(page);
  await sendAndWait(page, channelId, ["$ "]);

  // 画面をクリックすると xterm.js の入力の要素にフォーカスが移る。
  await page.getByTestId("terminal-host").locator(".xterm-screen").click();
  await expect(page.locator(".xterm-helper-textarea")).toBeFocused();
  await page.keyboard.type("echo ok");
  await page.keyboard.press("Enter");
  await page.keyboard.press("Control+c");

  await expect
    .poll(async () => (await readCallArgs(page, "write_terminal")).map((w) => w.data).join(""))
    .toBe("echo ok\r\x03");
  const writes = await readCallArgs(page, "write_terminal");
  expect(writes.every((w) => w.terminalId === TERMINAL_ID)).toBe(true);
});

test("最初の接続の行・桁は 0 でなく、ウィンドウの大きさを変えると resize_terminal が届く", async ({ page }) => {
  const { connect } = await openTerminal(page);
  expect(connect.terminalId).toBe(TERMINAL_ID);
  expect(connect.rows).toBeGreaterThan(0);
  expect(connect.cols).toBeGreaterThan(0);
  // 接続の行数は画面に描かれた行数と同じ。
  await expect(screenRows(page)).toHaveCount(connect.rows);

  const viewport = page.viewportSize();
  if (viewport === null) throw new Error("ビューポートの大きさが取れない");
  await page.setViewportSize({ width: Math.round(viewport.width / 2), height: Math.round(viewport.height / 2) });

  await expect.poll(async () => (await readCallArgs(page, "resize_terminal")).length).toBeGreaterThan(0);
  const resizes = await readCallArgs(page, "resize_terminal");
  const last = resizes.at(-1);
  if (last === undefined) throw new Error("resize_terminal が届いていない");
  expect(last.terminalId).toBe(TERMINAL_ID);
  expect(last.rows).toBeGreaterThan(0);
  expect(last.cols).toBeGreaterThan(0);
  expect(last.rows).toBeLessThan(connect.rows);
  expect(last.cols).toBeLessThan(connect.cols);
  await expect(screenRows(page)).toHaveCount(last.rows);
});

test("10,000 行を超えて流したあと、10,000 行までさかのぼれ、それより前は残っていない", async ({ page }) => {
  const { channelId, connect } = await openTerminal(page);
  const rows = connect.rows;
  const lineCount = SCROLLBACK_LINES + rows + 500;
  const label = (n: number) => `line-${String(n).padStart(5, "0")}`;

  // 100 行ずつのかたまりで流す(実物の PTY の出力も小分けに届く)。
  const chunks: string[] = [];
  for (let start = 1; start <= lineCount; start += 100) {
    let chunk = "";
    for (let n = start; n < start + 100 && n <= lineCount; n++) chunk += `${label(n)}\r\n`;
    chunks.push(chunk);
  }
  await sendAndWait(page, channelId, chunks);

  // バッファーは「line-1 … line-N」と、最後の改行のあとのカーソルの空行で N + 1 行。
  // 残るのは rows + SCROLLBACK_LINES 行なので、画面の先頭の行と最古の行は次のとおり。
  const totalLines = lineCount + 1;
  const firstVisible = totalLines - rows + 1;
  const oldestKept = totalLines - (rows + SCROLLBACK_LINES) + 1;
  expect(firstVisible - oldestKept).toBe(SCROLLBACK_LINES);

  const screenTop = screenRows(page).first();
  await expect(screenTop).toHaveText(label(firstVisible));
  await expect(screenRows(page).nth(rows - 2)).toHaveText(label(lineCount));

  // ホイールで一番上までさかのぼる。上に着いたら、それ以上は戻らない。
  const screen = page.getByTestId("terminal-host").locator(".xterm-screen");
  await screen.hover();
  await expect
    .poll(async () => {
      await page.mouse.wheel(0, -1_000_000);
      return screenTop.textContent();
    })
    .toBe(label(oldestKept));
  await page.mouse.wheel(0, -1_000_000);
  await expect(screenTop).toHaveText(label(oldestKept));
  await expect(screenRows(page).nth(1)).toHaveText(label(oldestKept + 1));
  await expect(screenRows(page).filter({ hasText: label(oldestKept - 1) })).toHaveCount(0);
});

test("取り込みが済んだ量の合計が、流したバイト数と同じになる(文字の途中で切れた出力を含む)", async ({ page }) => {
  const { channelId } = await openTerminal(page);
  const utf8 = Array.from(new TextEncoder().encode("日本"));
  // "日" の 3 バイトを 2 回に分けて送る(PTY の読み取りは文字の切れ目を知らない)。
  const chunks: OutputChunk[] = ["abc ", utf8.slice(0, 2), utf8.slice(2), "\x1b[32mgreen\x1b[0m\r\n", "x".repeat(5_000)];

  const sent = await sendAndWait(page, channelId, chunks);

  expect(sent).toBe(4 + 6 + "\x1b[32mgreen\x1b[0m\r\n".length + 5_000);
  const acks = await readCallArgs(page, "ack_terminal_output");
  expect(acks.every((ack) => ack.terminalId === TERMINAL_ID && ack.bytes > 0)).toBe(true);
  await expect(screenRows(page).nth(0)).toHaveText("abc 日本green");
});

test("シェルの終了と起動の失敗が terminal-status に出る(写しの更新のイベントで)", async ({ page }) => {
  await openTerminal(page);
  const status = page.getByTestId("terminal-status");
  // 実行中は何も出さない。
  await expect(status).toHaveCount(0);

  await emitEvent(page, "app-snapshot", snapshotWith({ kind: "exited", code: 0, signal: null }));
  await expect(status).toHaveText("シェルが終了した(終了コード 0)");

  await emitEvent(page, "app-snapshot", snapshotWith({ kind: "exited", code: null, signal: "SIGKILL" }));
  await expect(status).toHaveText("シェルが終了した(シグナル SIGKILL)");

  await emitEvent(page, "app-snapshot", snapshotWith({ kind: "failed", message: "zsh が見つからない" }));
  await expect(status).toHaveText("シェルを起動できなかった: zsh が見つからない");

  // 状態が変わってもターミナルは作り直さない(接続は最初の 1 回だけ)。
  await expect(page.getByTestId("terminal-host")).toBeVisible();
  expect(await readCallArgs(page, "connect_terminal")).toHaveLength(1);
});

test("起動の失敗の写しで開くと、最初から terminal-status に理由が出る", async ({ page }) => {
  await openTerminal(page, { kind: "failed", message: "PTY を開けなかった" });
  await expect(page.getByTestId("terminal-status")).toHaveText("シェルを起動できなかった: PTY を開けなかった");
});
