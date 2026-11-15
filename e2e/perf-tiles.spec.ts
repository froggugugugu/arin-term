// 画面側の 9 枚同時表示の計測(計画書 T49b。NFR-009、ADR-001)。
//
// 記録だけで、数値では失敗させない。`npm run e2e` には入らない(playwright.config.ts の testIgnore。
// `npm run perf` が ARIN_PERF_E2E=1 を付けて、この spec だけを 1 ワーカーで動かす)。結果は testreport/perf/e2e-tiles-*.json。
//
// 偽の Rust 側(fixtures/tauriMock.ts)で 10 プロジェクト・各 1 タブのタイル表示(1 ページ目に 9 枚、2 ページ目に 1 枚)を開き、
// 1 ページ目の 9 枚に同時に大量の出力を流す。流し方は製品の流量制御に合わせる(1 枚ごとに未処理の量が 512 KiB に
// 達したら止め、画面の処理済みの通知で再開する。1 回の送信は 64 KiB。4ms ごと)。流量制御は偽物の側で真似るだけで、
// 製品のソースに差し込み口は置かない(通信の関数を包むのはテストのページの中だけ)。
//
// 流しているあいだに記録するもの:
// - 9 枚が取り込み終えるまでの時間(全量の処理済みが返るまで)
// - フレームの間隔(requestAnimationFrame の間隔。描画のコマ落ちの目安)
// - タイルのクリック→focus_terminal、キー入力→write_terminal、次のページのボタン→next_tile_page の、画面がコマンドを
//   呼ぶまでの時間(押した瞬間は捕捉段階の pointerdown・keydown)
// - 写しで 2 ページ目へ送ってから描き終わるまで(9 枚を隠して 1 枚を出す)と、1 ページ目へ戻して描き終わるまで
// - 画面の失敗の報告
//
// RUNS 回、毎回新しいページで開く。描画方式は DOM、全角幅の方式は unicode11(製品の固定の方式)。
// T49b では描画方式(dom・webgl)× 全角幅の方式(unicode11・graphemes)の 4 組を測り、その結果で T50b に DOM・unicode11 に
// 決めた(ADR-001「M4 の実測と案」。そのときの記録は testreport/perf/ の 2026-11 の e2e-tiles-*.json)。
// Playwright の WebKit は WKWebView の実機とは別のビルドなので、数値は回どうし・版どうしの比較に使う。

import { expect, type Browser, type Page, test } from "@playwright/test";

import type { ProjectColor } from "../src/ipc/generated/ProjectColor";
import {
  installTauriMock,
  parseChannelId,
  projectsSnapshot,
  readCallArgs,
  readRecordedCalls,
  type SampleProject,
  withTileDisplay,
} from "./fixtures/tauriMock";

/** 1 枚あたりに流す量(バイト)。流量制御が効いていることを確かめられる最小に近い量(上限 512 KiB の 4 倍)。 */
const BYTES_PER_TERMINAL = 2 * 1024 * 1024;
/** 製品の未処理の量の上限(arin-core の OUTPUT_HIGH_WATER)。 */
const HIGH_WATER = 512 * 1024;
/** 1 回に送る量(arin-core の OUTPUT_BATCH_MAX_BYTES)。 */
const CHUNK_BYTES = 64 * 1024;
/** 送る間隔(arin-core の OUTPUT_BATCH_WINDOW_MS)。 */
const TICK_MS = 4;
/** 流す行(ASCII・全角・絵文字の修飾子・記号を混ぜる)。 */
const LINE = "arin perf tiles 0123456789 日本語の全角の文字 abcdefghij 👍🏽 ✅ ─│┌┐\r\n";
/** 取り込みを待つ上限。 */
const FLOOD_TIMEOUT_MS = 60_000;
const RUNS = 2;
const PROJECTS = 10;
const VISIBLE = 9;

const COLORS: readonly ProjectColor[] = ["blue", "green", "orange", "purple", "teal", "pink", "red", "yellow"];
const LIST: SampleProject[] = Array.from({ length: PROJECTS }, (_, i) => ({
  id: `p${i + 1}`,
  name: `proj${i + 1}`,
  color: COLORS[i % COLORS.length] ?? "blue",
  tabs: ["ターミナル 1"],
}));
const VISIBLE_IDS = LIST.slice(0, VISIBLE).map((p) => `${p.id}-t1`);

/** ページの中の計測の記録(テストの側だけが置く)。 */
interface PerfWindow {
  __ARIN_PERF__?: {
    events: Array<{ type: string; t: number }>;
    commands: Array<{ cmd: string; t: number }>;
    frames: number[];
    flood?: Promise<{ ingestMs: number | null; acked: number; total: number }>;
  };
}

interface RunResult {
  run: number;
  bytesPerTerminal: number;
  terminals: number;
  ingestMs: number | null;
  mibPerS: number | null;
  ackedBytes: number;
  totalBytes: number;
  frames: { count: number; medianMs: number; p95Ms: number; maxMs: number; over50Percent: number } | null;
  clickToCommandMs: number | null;
  keyToCommandMs: number | null;
  nextPageToCommandMs: number | null;
  pageAwayMs: number | null;
  pageBackMs: number | null;
  frontendErrors: number;
  frontendErrorMessages: string[];
}

const round = (value: number | null) => (value === null ? null : Math.round(value * 10) / 10);

function percentile(values: readonly number[], p: number): number {
  const sorted = [...values].sort((a, b) => a - b);
  if (sorted.length === 0) return Number.NaN;
  return sorted[Math.min(sorted.length - 1, Math.floor((sorted.length - 1) * p))] ?? Number.NaN;
}

/** 捕捉段階の入力の記録と、通信の関数を包んでコマンドの時刻を残す(テストのページの中だけ)。 */
async function instrument(page: Page): Promise<void> {
  await page.evaluate(() => {
    const w = window as unknown as PerfWindow & { __TAURI_INTERNALS__: { invoke: (c: string, a: unknown, o?: unknown) => unknown } };
    const perf = { events: [] as Array<{ type: string; t: number }>, commands: [] as Array<{ cmd: string; t: number }>, frames: [] as number[] };
    w.__ARIN_PERF__ = perf;
    for (const type of ["pointerdown", "keydown"]) {
      document.addEventListener(type, () => perf.events.push({ type, t: performance.now() }), { capture: true });
    }
    const watched = new Set(["focus_terminal", "write_terminal", "write_terminal_bytes", "next_tile_page"]);
    const internals = w.__TAURI_INTERNALS__;
    const original = internals.invoke.bind(internals);
    internals.invoke = (cmd, args, options) => {
      if (watched.has(cmd)) perf.commands.push({ cmd, t: performance.now() });
      return original(cmd, args, options);
    };
  });
}

/** 1 ページ目の 9 枚へ流し始める(待たない)。フレームの間隔も記録し始める。 */
async function startFlood(page: Page, channels: Array<{ id: string; channel: number }>): Promise<void> {
  await page.evaluate(
    ({ channels, total, high, chunkBytes, tickMs, line, timeoutMs }) => {
      const w = window as unknown as PerfWindow & {
        __TAURI_INTERNALS__: { invoke: (c: string, a: unknown, o?: unknown) => unknown };
        __ARIN_E2E__: { sendChannel(channelId: number, messages: unknown[]): boolean };
      };
      const perf = w.__ARIN_PERF__;
      if (perf === undefined) throw new Error("計測の記録が無い");
      const acked = new Map(channels.map((c) => [c.id, 0]));
      const internals = w.__TAURI_INTERNALS__;
      const original = internals.invoke.bind(internals);
      internals.invoke = (cmd, args, options) => {
        if (cmd === "ack_terminal_output") {
          const a = args as { terminalId: string; bytes: number };
          if (acked.has(a.terminalId)) acked.set(a.terminalId, (acked.get(a.terminalId) ?? 0) + a.bytes);
        }
        return original(cmd, args, options);
      };
      const encoded = new TextEncoder().encode(line);
      const chunk = new Uint8Array(chunkBytes);
      for (let i = 0; i < chunkBytes; i++) chunk[i] = encoded[i % encoded.length] ?? 0x20;
      const sent = new Map(channels.map((c) => [c.id, 0]));
      let frameRunning = true;
      let lastFrame = performance.now();
      const frame = (t: number) => {
        perf.frames.push(t - lastFrame);
        lastFrame = t;
        if (frameRunning) requestAnimationFrame(frame);
      };
      requestAnimationFrame((t) => {
        lastFrame = t;
        requestAnimationFrame(frame);
      });
      const started = performance.now();
      perf.flood = new Promise((resolve) => {
        const tick = () => {
          let done = true;
          for (const c of channels) {
            let s = sent.get(c.id) ?? 0;
            const a = acked.get(c.id) ?? 0;
            // 製品の流量制御と同じく、未処理の量が上限に達するまで送る(最後の 1 回は上限を少し超えうる)。
            while (s < total && s - a < high) {
              const n = Math.min(chunkBytes, total - s);
              w.__ARIN_E2E__.sendChannel(c.channel, [chunk.slice(0, n).buffer]);
              s += n;
            }
            sent.set(c.id, s);
            if (a < total) done = false;
          }
          const elapsed = performance.now() - started;
          const ackedTotal = [...acked.values()].reduce((x, y) => x + y, 0);
          if (done || elapsed > timeoutMs) {
            frameRunning = false;
            resolve({ ingestMs: done ? elapsed : null, acked: ackedTotal, total: total * channels.length });
            return;
          }
          setTimeout(tick, tickMs);
        };
        tick();
      });
    },
    { channels, total: BYTES_PER_TERMINAL, high: HIGH_WATER, chunkBytes: CHUNK_BYTES, tickMs: TICK_MS, line: LINE, timeoutMs: FLOOD_TIMEOUT_MS },
  );
}

/** 最後の type の入力のあとに、最初に cmds のどれかが呼ばれるまでの時間。 */
async function latency(page: Page, type: string, cmds: readonly string[]): Promise<number | null> {
  await page.waitForTimeout(150);
  return page.evaluate(
    ({ type, cmds }) => {
      const perf = (window as unknown as PerfWindow).__ARIN_PERF__;
      const event = perf?.events.filter((e) => e.type === type).at(-1);
      if (perf === undefined || event === undefined) return null;
      const command = perf.commands.find((c) => cmds.includes(c.cmd) && c.t >= event.t);
      return command === undefined ? null : command.t - event.t;
    },
    { type, cmds: [...cmds] },
  );
}

/** 写しを流してから、タイルの数とターミナルの要素の数が count になり 2 フレーム描くまでの時間。 */
async function emitAndWaitTiles(page: Page, snapshot: unknown, count: number): Promise<number | null> {
  return page.evaluate(
    ({ snapshot, count }) =>
      new Promise<number | null>((resolve) => {
        const w = window as unknown as { __ARIN_E2E__: { emit(event: string, payload: unknown): number } };
        const start = performance.now();
        w.__ARIN_E2E__.emit("app-snapshot", snapshot);
        const check = () => {
          const tiles = document.querySelectorAll('[data-testid="tile"]').length;
          const hosts = document.querySelectorAll('[data-testid="tile"] .xterm').length;
          if (tiles === count && hosts === count) {
            requestAnimationFrame(() => requestAnimationFrame(() => resolve(performance.now() - start)));
            return;
          }
          if (performance.now() - start > 10_000) {
            resolve(null);
            return;
          }
          requestAnimationFrame(check);
        };
        check();
      }),
    { snapshot, count },
  );
}

async function measureOnce(browser: Browser, run: number): Promise<RunResult> {
  const page = await browser.newPage();
  try {
    const snapshot = withTileDisplay(projectsSnapshot(LIST));
    await installTauriMock(page, { get_app_snapshot: { ok: snapshot } });
    await page.goto("/");
    await expect.poll(async () => (await readRecordedCalls(page)).map((c) => c.command)).toContain("notify_app_ready");
    await expect(page.getByTestId("tile")).toHaveCount(VISIBLE);
    await expect
      .poll(async () => new Set((await readCallArgs(page, "connect_terminal")).map((c) => c.terminalId)).size, { timeout: 20_000 })
      .toBeGreaterThanOrEqual(VISIBLE);
    const connects = await readCallArgs(page, "connect_terminal");
    const channels = VISIBLE_IDS.map((id) => {
      const c = connects.filter((x) => x.terminalId === id).at(-1);
      if (c === undefined) throw new Error(`${id} の接続が無い`);
      return { id, channel: parseChannelId(c.onOutput) };
    });

    await instrument(page);
    await startFlood(page, channels);
    await page.waitForTimeout(200);

    // 流しているあいだの操作。
    await page.getByRole("group", { name: "proj2: ターミナル 1", exact: true }).getByTestId("tile-title").click();
    const clickToCommandMs = await latency(page, "pointerdown", ["focus_terminal"]);
    await page.getByRole("group", { name: "proj1: ターミナル 1", exact: true }).locator(".xterm-helper-textarea").focus();
    await page.keyboard.press("a");
    const keyToCommandMs = await latency(page, "keydown", ["write_terminal", "write_terminal_bytes"]);
    await page.getByTestId("tile-paging").getByRole("button", { name: "次のページ" }).click();
    const nextPageToCommandMs = await latency(page, "pointerdown", ["next_tile_page"]);
    const pageAwayMs = await emitAndWaitTiles(page, withTileDisplay(projectsSnapshot(LIST), { page: 1 }), 1);
    const pageBackMs = await emitAndWaitTiles(page, withTileDisplay(projectsSnapshot(LIST), { page: 0 }), VISIBLE);

    const flood = await page.evaluate(async () => {
      const perf = (window as unknown as PerfWindow).__ARIN_PERF__;
      if (perf?.flood === undefined) throw new Error("流していない");
      const r = await perf.flood;
      return { ...r, frames: perf.frames };
    });
    const frames = flood.frames.filter((f) => Number.isFinite(f) && f > 0);
    const errors = (await readCallArgs(page, "report_frontend_error")).map((r) => r.message);
    return {
      run,
      bytesPerTerminal: BYTES_PER_TERMINAL,
      terminals: VISIBLE,
      ingestMs: round(flood.ingestMs),
      mibPerS: flood.ingestMs === null ? null : round(flood.total / (1024 * 1024) / (flood.ingestMs / 1000)),
      ackedBytes: flood.acked,
      totalBytes: flood.total,
      frames:
        frames.length === 0
          ? null
          : {
              count: frames.length,
              medianMs: round(percentile(frames, 0.5)) ?? Number.NaN,
              p95Ms: round(percentile(frames, 0.95)) ?? Number.NaN,
              maxMs: round(Math.max(...frames)) ?? Number.NaN,
              over50Percent: round((frames.filter((f) => f > 50).length / frames.length) * 100) ?? Number.NaN,
            },
      clickToCommandMs: round(clickToCommandMs),
      keyToCommandMs: round(keyToCommandMs),
      nextPageToCommandMs: round(nextPageToCommandMs),
      pageAwayMs: round(pageAwayMs),
      pageBackMs: round(pageBackMs),
      frontendErrors: errors.length,
      frontendErrorMessages: errors.slice(0, 10),
    };
  } finally {
    await page.close();
  }
}

/** 書き出しに使う node:fs の部分(e2e/ の tsconfig は Node の型を入れていないので、使う関数の形だけを書く)。 */
interface NodeFsSubset {
  mkdirSync(path: string, options: { recursive: true }): unknown;
  writeFileSync(path: string, data: string): void;
}

async function nodeFs(): Promise<NodeFsSubset> {
  const specifier: string = "node:fs";
  return (await import(specifier)) as NodeFsSubset;
}

function repoRoot(): string {
  const configFile = test.info().config.configFile;
  if (configFile === undefined) throw new Error("Playwright の設定ファイルの場所が分からない");
  return configFile.slice(0, configFile.lastIndexOf("/"));
}

test.describe.configure({ mode: "serial" });

test("9_枚のタイルに同時に流しているあいだの取り込みと反応を記録する", async ({ browser, browserName }) => {
  test.setTimeout(600_000);
  const runs: RunResult[] = [];
  for (let run = 1; run <= RUNS; run++) {
    const r = await measureOnce(browser, run);
    runs.push(r);
    test.info().annotations.push({
      type: "NFR-009",
      description: `${run} 回目: 取り込み ${r.ingestMs}ms、フレーム p95 ${r.frames?.p95Ms}ms、失敗の報告 ${r.frontendErrors}`,
    });
  }

  const result = {
    kind: "e2e-tiles",
    requirement: "NFR-009(画面側。記録だけで合否にしない)",
    recordedAt: new Date().toISOString(),
    browser: `${browserName} ${browser.version()}`,
    bytesPerTerminal: BYTES_PER_TERMINAL,
    terminals: VISIBLE,
    runs,
    note:
      "Vite の画面を Playwright の WebKit で開き、偽の Rust 側(e2e/fixtures/tauriMock.ts)が 9 枚に同時に流す。" +
      "流量制御は製品の値(未処理 512 KiB、64 KiB ずつ、4ms ごと)を偽物の側で真似たもの。" +
      "操作の時間は、捕捉段階の pointerdown・keydown から画面がコマンドを呼ぶまで。ページの時間は写しを流してから 2 フレーム描くまで。",
  };
  const fs = await nodeFs();
  const outDir = `${repoRoot()}/testreport/perf`;
  fs.mkdirSync(outDir, { recursive: true });
  const stamp = result.recordedAt.replace(/[-:]/g, "").replace(/\..*$/, "").replace("T", "-");
  const body = `${JSON.stringify(result, null, 2)}\n`;
  fs.writeFileSync(`${outDir}/e2e-tiles-${stamp}.json`, body);
  fs.writeFileSync(`${outDir}/e2e-tiles-latest.json`, body);

  // 数値では失敗させない。測れたことだけを確かめる。
  expect(runs).toHaveLength(RUNS);
});
