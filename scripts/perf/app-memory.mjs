// 性能の計測: 起動中のアプリのメモリと、起動からターミナルの印までの時間(T24。NFR-001・NFR-002 の参考)。
// 記録するだけで合否にしない。`scripts/perf/run.mjs` から呼ぶ。
//
// 起動のしかたはスモーク(scripts/smoke.mjs)と同じ: 一時の状態フォルダ(ARIN_STATE_DIR)と ARIN_SMOKE=1 を付けて
// .app の実行ファイルを起動し、準備完了とターミナルの印を待つ。印が揃ってから少し置いて、次のプロセスを測る。
// 状態は NFR-001 の条件の「5 プロジェクト・各 1 タブ」(計画書 T35b): 一時フォルダの中にプロジェクトのフォルダを 5 つ作り、
// bookmark が null の state.json(計画書 M2 の【仮定】13。形はスモークの stateFixture と同じ)を一時の状態フォルダに置く。
// 本物の ~/Library/Application Support/Arin/ は読み書きしない(前後で変わっていないことを確かめる)。
//
// - アプリ本体(起動した実行ファイル)
// - WebKit の補助プロセス(com.apple.WebKit.WebContent・GPU・Networking)。launchd が起動するので
//   アプリの子プロセスにならない(親は 1)。起動の前後で増えたものをこのアプリのものとみなし、
//   アプリの終了と一緒に消えたかも記録する(消えていれば、取り違えていない裏付けになる)
// - アプリの子プロセス(シェル)。NFR-001 は子プロセスを除くので別に記録する
//
// 測り方: `footprint -f bytes --noCategories -j <file> -p <pid> ...`(アクティビティモニタの「メモリ」と同じ
// phys_footprint)。参考に `ps -o rss=` も記録する。`footprint` に PID を位置引数で渡すと名前の部分一致になるので、
// 必ず -p で渡す。
//
// .app が無ければ測らずに skipped を返す(ビルドしない。先に `npm run app`)。
//
// M4(計画書 T49b。NFR-009、ADR-001、計画書 M4 の【仮定】18)で、起動の組(APP_SCENARIOS)を足した:
// - tabs5: 上の 5 プロジェクト・各 1 タブ(タブ表示)。M2・M3 の値と並べる
// - tiles9-<描画方式>: 9 プロジェクト・各 1 タブで `ui.displayMode: "tiles"` の state.json。起動から 9 枚のシェルが
//   揃うまでの時間と、落ち着いたあとのメモリ。描画方式は ARIN_RENDERER で渡す(dom・webgl)
// - tiles9-<描画方式>-flood: 同じ 9 枚で、各シェルが起動直後に `yes | head -c` で FLOOD_BYTES だけ流す。
//   シェルに流させるため、一時フォルダの HOME(.zshrc だけを置く)をアプリに渡す(HOME はシェルへ引き継がれる)。
//   流し終えたシェルが HOME に印のファイルを書くので、9 枚が流し終えるまでの時間と、そのあいだの本体・WebKit の
//   常駐メモリ(ps の RSS)の最大を記録する。量に上限があるので流しっぱなしにはならない
// どの組でも標準エラーの `[arin:frontend-error]`(WebGL を作れず DOM に戻ったときなど)の行を数える。
// 起動の前ごとにシステムのメモリの空き(memory_pressure)を見て、下限を下回ったら残りを測らずに止める
// (このマシンはメモリに余裕が無い。2026-11-15 に計測の途中でコマンドが止められた)。

import { spawn, spawnSync } from "node:child_process";
import { rmSync } from "node:fs";
import { access, mkdir, mkdtemp, readdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";

import {
  createOutputWatcher,
  DEFAULT_APP_PATH,
  DEFAULT_REAL_STATE_DIR,
  ENV_SMOKE,
  ENV_STATE_DIR,
  MARKER_FILE_NAME,
  STATE_FILE_NAME,
  TERMINAL_MARKER_FILE_NAME,
  childPids,
  describeDir,
  isAlive,
  numberedProjectId,
  projectsStateFixture,
  readBundleExecutable,
} from "../smoke.mjs";

export const TEMP_PREFIX = "arin-perf-app-";
/** 計測のときのプロジェクトの数(各 1 タブ。NFR-001 の条件)。 */
export const PERF_PROJECT_COUNT = 5;
const WEBKIT_RE = /\/(com\.apple\.WebKit\.[A-Za-z]+)$/;
/** 9 枚の計測のプロジェクトの数(NFR-009。各 1 タブ)。 */
export const TILE_PROJECT_COUNT = 9;
/** 流す組で、シェル 1 枚が流す量(バイト。`head -c` に渡す)。 */
export const FLOOD_BYTES = 4 * 1024 * 1024;
/** 流す組で、シェルが流し終えたときに HOME に書く印のファイルの名前の頭(後ろにシェルの PID)。 */
export const FLOOD_DONE_PREFIX = "flood-done.";
/** 起動の前に要るシステムのメモリの空き(%)。これを下回ったら残りを測らない。 */
export const DEFAULT_MIN_FREE_PERCENT = 50;

/**
 * 起動の組。name は結果の scenario、projects はプロジェクトの数、displayMode は state.json の ui.displayMode
 * (null なら書かない=タブ表示)、env はアプリに足す環境変数、flood は各シェルに流させるか、runs は起動の回数。
 */
export const APP_SCENARIOS = [
  { name: "tabs5", projects: PERF_PROJECT_COUNT, displayMode: null, env: {}, flood: false, runs: 3 },
  { name: "tiles9-dom", projects: TILE_PROJECT_COUNT, displayMode: "tiles", env: { ARIN_RENDERER: "dom" }, flood: false, runs: 2 },
  { name: "tiles9-webgl", projects: TILE_PROJECT_COUNT, displayMode: "tiles", env: { ARIN_RENDERER: "webgl" }, flood: false, runs: 2 },
  { name: "tiles9-dom-flood", projects: TILE_PROJECT_COUNT, displayMode: "tiles", env: { ARIN_RENDERER: "dom" }, flood: true, runs: 2 },
  { name: "tiles9-webgl-flood", projects: TILE_PROJECT_COUNT, displayMode: "tiles", env: { ARIN_RENDERER: "webgl" }, flood: true, runs: 2 },
];

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

async function exists(p) {
  try {
    await access(p);
    return true;
  } catch {
    return false;
  }
}

/** 計測用のプロジェクトの ID(UUID v4 の形。n は 1 から)。スモークの --tiles と同じ(scripts/smoke.mjs)。 */
export const perfProjectId = numberedProjectId;

/**
 * 計測用の state.json の中身。projectDirs の順にプロジェクトにし(bookmark は null、各 1 タブはアプリが開く)、先頭を選択中にする。
 * displayMode を渡すと ui.displayMode に書く(T46c。"tiles" でタイル表示で開く)。形はスモークの --tiles と共有する
 * (scripts/smoke.mjs の projectsStateFixture)。
 */
export function perfStateFixture(projectDirs, options = {}) {
  return projectsStateFixture(projectDirs, options);
}

/**
 * root の下に状態フォルダ(state/)とプロジェクトのフォルダ(projects/p1〜)を作り、state.json を置く。
 * @returns {Promise<{ stateDir: string, projectDirs: string[], fixture: string }>}
 */
export async function preparePerfState(root, count = PERF_PROJECT_COUNT, { displayMode = null } = {}) {
  const stateDir = path.join(root, "state");
  await mkdir(stateDir, { mode: 0o700 });
  const projectDirs = [];
  for (let i = 1; i <= count; i += 1) {
    const dir = path.join(root, "projects", `p${i}`);
    await mkdir(dir, { recursive: true });
    projectDirs.push(dir);
  }
  const fixture = perfStateFixture(projectDirs, { displayMode });
  await writeFile(path.join(stateDir, STATE_FILE_NAME), fixture, { mode: 0o600 });
  return { stateDir, projectDirs, fixture };
}

/**
 * 流す組の .zshrc。起動直後に `yes | head -c bytes` を流し、流し終えたら開始と終了の時刻(秒。EPOCHREALTIME)を
 * `$HOME/flood-done.<PID>` に書く(書きかけを読まれないよう、別の名前で書いてから移す)。
 */
export function floodZshrc(bytes = FLOOD_BYTES) {
  if (!Number.isSafeInteger(bytes) || bytes <= 0) throw new Error(`流す量が正の整数でない: ${bytes}`);
  return [
    "# Arin の性能の計測(scripts/perf/app-memory.mjs)が一時フォルダに置いたもの。",
    "zmodload zsh/datetime",
    "__arin_t0=$EPOCHREALTIME",
    `yes 'arin perf tiles 0123456789 abcdefghijklmnopqrstuvwxyz ABCDEFGHIJKLMNOPQRSTUVWXYZ' | head -c ${bytes}`,
    `print -r -- "$__arin_t0 $EPOCHREALTIME" > "$HOME/${FLOOD_DONE_PREFIX}$$.tmp" && mv "$HOME/${FLOOD_DONE_PREFIX}$$.tmp" "$HOME/${FLOOD_DONE_PREFIX}$$"`,
    "",
  ].join("\n");
}

/** root の下に流す組の HOME(home/。.zshrc だけ)を作る。 */
export async function prepareFloodHome(root, bytes = FLOOD_BYTES) {
  const home = path.join(root, "home");
  await mkdir(home, { mode: 0o700 });
  await writeFile(path.join(home, ".zshrc"), floodZshrc(bytes), { mode: 0o600 });
  return home;
}

/** HOME の印のファイルを読み、流し終えたシェルごとの所要時間(ms)を返す。 */
export async function readFloodDone(home) {
  const names = (await readdir(home)).filter((n) => n.startsWith(FLOOD_DONE_PREFIX) && !n.endsWith(".tmp"));
  const out = [];
  for (const name of names) {
    const [t0, t1] = (await readFile(path.join(home, name), "utf8")).trim().split(/\s+/).map(Number);
    out.push({ pid: Number(name.slice(FLOOD_DONE_PREFIX.length)), ms: Number.isFinite(t0) && Number.isFinite(t1) ? (t1 - t0) * 1000 : null });
  }
  return out;
}

/** `memory_pressure` の最後の行のシステムのメモリの空き(%)。読めなければ null。 */
export function systemFreePercent() {
  const r = spawnSync("memory_pressure", [], { encoding: "utf8" });
  if (r.error || r.status !== 0) return null;
  const m = /free percentage:\s*(\d+)%/.exec(r.stdout);
  return m === null ? null : Number(m[1]);
}

/** いま動いている WebKit の補助プロセス(PID → 種類)。 */
function webkitProcesses() {
  const r = spawnSync("ps", ["-axo", "pid=,comm="], { encoding: "utf8" });
  if (r.status !== 0) throw new Error(`ps が失敗した: ${r.stderr}`);
  const map = new Map();
  for (const line of r.stdout.split("\n")) {
    const m = /^\s*(\d+)\s+(.+)$/.exec(line);
    if (m === null) continue;
    const w = WEBKIT_RE.exec(m[2].trim());
    if (w !== null) map.set(Number(m[1]), w[1]);
  }
  return map;
}

/** `footprint` で PID ごとの phys_footprint(バイト)を測る。測れなければ空の Map と理由。 */
async function footprintOf(pids, dir) {
  const file = path.join(dir, "footprint.json");
  const args = ["-f", "bytes", "--noCategories", "-j", file, ...pids.flatMap((p) => ["-p", String(p)])];
  const r = spawnSync("footprint", args, { encoding: "utf8" });
  if (r.error || r.status !== 0) {
    return { bytes: new Map(), error: `footprint が失敗した: ${r.error?.message ?? r.stderr.trim()}` };
  }
  const json = JSON.parse(await readFile(file, "utf8"));
  return { bytes: new Map(json.processes.map((p) => [p.pid, p.footprint])), error: null };
}

/** `ps -o rss=` で PID ごとの常駐メモリ(バイト)。 */
function rssOf(pids) {
  const r = spawnSync("ps", ["-o", "pid=,rss=", "-p", pids.join(",")], { encoding: "utf8" });
  const map = new Map();
  for (const line of r.stdout.split("\n")) {
    const m = /^\s*(\d+)\s+(\d+)/.exec(line);
    if (m !== null) map.set(Number(m[1]), Number(m[2]) * 1024);
  }
  return map;
}

function waitExit(child, ms) {
  if (child.exitCode !== null || child.signalCode !== null) return Promise.resolve(true);
  return new Promise((resolve) => {
    const timer = setTimeout(() => {
      child.off("exit", onExit);
      resolve(false);
    }, ms);
    const onExit = () => {
      clearTimeout(timer);
      resolve(true);
    };
    child.once("exit", onExit);
  });
}

/** pids がすべて終わるまで待つ(上限 ms)。残った PID を返す。 */
async function waitGone(pids, ms) {
  const deadline = Date.now() + ms;
  let left = pids.filter(isAlive);
  while (left.length > 0 && Date.now() < deadline) {
    await sleep(50);
    left = left.filter(isAlive);
  }
  return left;
}

/** pids の常駐メモリ(ps の RSS。バイト)の合計。 */
function rssSum(pids) {
  if (pids.length === 0) return 0;
  let total = 0;
  for (const v of rssOf(pids).values()) total += v;
  return total;
}

/**
 * 1 回起動して測る。scenario は APP_SCENARIOS の 1 つ。
 * 9 枚の組(projects が 2 以上でタイル表示)は、印に加えてアプリの子プロセス(シェル)が projects 個揃うまで待つ。
 */
async function measureOnce({ executable, scenario, settleMs, timeoutMs, floodTimeoutMs, floodBytes, log }) {
  const before = webkitProcesses();
  const base = tmpdir();
  const tempRoot = await mkdtemp(path.join(base, TEMP_PREFIX));
  // 自分が mkdtemp で作ったフォルダか(消してよいか)。
  const ownsTempRoot = path.dirname(tempRoot) === base && path.basename(tempRoot).startsWith(TEMP_PREFIX);
  let child;
  let shells = [];
  const killOnExit = () => {
    child?.kill("SIGKILL");
    for (const pid of shells) if (isAlive(pid)) process.kill(pid, "SIGKILL");
    if (ownsTempRoot) rmSync(tempRoot, { recursive: true, force: true });
  };
  process.on("exit", killOnExit);
  const watcher = createOutputWatcher();
  const waitShells = scenario.displayMode === "tiles" ? scenario.projects : null;
  try {
    const { stateDir, projectDirs } = await preparePerfState(tempRoot, scenario.projects, { displayMode: scenario.displayMode });
    const home = scenario.flood ? await prepareFloodHome(tempRoot, floodBytes) : null;
    const started = performance.now();
    child = spawn(executable, [], {
      env: {
        ...process.env,
        ...scenario.env,
        ...(home === null ? {} : { HOME: home }),
        [ENV_STATE_DIR]: stateDir,
        [ENV_SMOKE]: "1",
      },
      stdio: ["ignore", "pipe", "pipe"],
    });
    child.stdout.on("data", (chunk) => watcher.feed("stdout", chunk));
    child.stderr.on("data", (chunk) => watcher.feed("stderr", chunk));
    let spawnError = null;
    child.on("error", (e) => {
      spawnError = e;
    });
    const times = { ready: null, terminal: null, shells: null };
    let lastShellPoll = -Infinity;
    while (performance.now() - started < timeoutMs) {
      if (spawnError !== null) throw new Error(`アプリを起動できなかった: ${spawnError.message}`);
      if (child.exitCode !== null || child.signalCode !== null) {
        throw new Error(`印が揃う前にアプリが終了した(終了コード ${child.exitCode}、シグナル ${child.signalCode})`);
      }
      const now = performance.now() - started;
      if (times.ready === null && (await exists(path.join(stateDir, MARKER_FILE_NAME)))) times.ready = now;
      if (times.terminal === null && (await exists(path.join(stateDir, TERMINAL_MARKER_FILE_NAME)))) times.terminal = now;
      // シェルの数は pgrep で数えるので、100ms ごとにだけ見る。
      if (waitShells !== null && times.shells === null && now - lastShellPoll >= 100) {
        lastShellPoll = now;
        if (childPids(child.pid).length >= waitShells) times.shells = now;
      }
      if (times.ready !== null && times.terminal !== null && (waitShells === null || times.shells !== null)) break;
      await sleep(10);
    }
    if (times.ready === null || times.terminal === null || (waitShells !== null && times.shells === null)) {
      throw new Error(
        `${timeoutMs} ミリ秒以内に印が揃わなかった(準備完了 ${times.ready !== null}、ターミナル ${times.terminal !== null}` +
          `${waitShells === null ? "" : `、シェル ${waitShells} 個 ${times.shells !== null}`})`,
      );
    }

    // 流す組: 9 枚が流し終えるまで待ち、そのあいだの本体と WebKit の常駐メモリの最大を取る。
    let flood = null;
    if (home !== null) {
      const peak = { app: 0, webkit: 0 };
      let done = [];
      const floodStarted = performance.now();
      while (performance.now() - floodStarted < floodTimeoutMs) {
        const webkitNow = [...webkitProcesses().keys()].filter((pid) => !before.has(pid));
        peak.app = Math.max(peak.app, rssSum([child.pid]));
        peak.webkit = Math.max(peak.webkit, rssSum(webkitNow));
        done = await readFloodDone(home);
        if (done.length >= scenario.projects) break;
        if (child.exitCode !== null || child.signalCode !== null) throw new Error("流しているあいだにアプリが終了した");
        await sleep(250);
      }
      const durations = done.map((d) => d.ms).filter((v) => v !== null);
      flood = {
        bytes_per_terminal: floodBytes,
        done_count: done.length,
        all_done_ms: done.length >= scenario.projects ? performance.now() - started : null,
        terminal_ms: durations,
        peak_app_rss: peak.app,
        peak_webkit_rss: peak.webkit,
      };
      if (done.length < scenario.projects) {
        log?.(`  ${floodTimeoutMs} ミリ秒以内に流し終えたのは ${done.length}/${scenario.projects} 枚`);
      }
    }
    await sleep(settleMs);

    const after = webkitProcesses();
    const webkit = [...after.keys()].filter((pid) => !before.has(pid));
    shells = childPids(child.pid);
    const all = [child.pid, ...webkit, ...shells];
    const fp = await footprintOf(all, tempRoot);
    const rss = rssOf(all);
    const proc = (pid, kind) => ({ pid, kind, footprint: fp.bytes.get(pid) ?? null, rss: rss.get(pid) ?? null });
    const processes = [
      proc(child.pid, "app"),
      ...webkit.map((pid) => proc(pid, after.get(pid))),
      ...shells.map((pid) => proc(pid, "shell")),
    ];

    child.kill("SIGTERM");
    if (!(await waitExit(child, 5_000))) {
      child.kill("SIGKILL");
      await waitExit(child, 5_000);
    }
    const webkitLeft = await waitGone(webkit, 5_000);
    const shellsLeft = await waitGone(shells, 5_000);
    for (const pid of shellsLeft) {
      try {
        process.kill(pid, "SIGKILL");
      } catch {
        // 既に終わっていればよい。
      }
    }
    watcher.end();
    const shellNote = times.shells === null ? "" : `、シェル ${scenario.projects} 個まで ${times.shells.toFixed(0)} ms`;
    const floodNote = flood?.all_done_ms == null ? "" : `、流し終えるまで ${flood.all_done_ms.toFixed(0)} ms`;
    log?.(`  ${scenario.name} アプリ ${child.pid}: ターミナルの印まで ${times.terminal.toFixed(0)} ms${shellNote}${floodNote}`);
    return {
      scenario: scenario.name,
      renderer: scenario.env.ARIN_RENDERER ?? "dom",
      display_mode: scenario.displayMode ?? "tabs",
      projects: projectDirs.length,
      shell_count: shells.length,
      startup_ready_ms: times.ready,
      startup_terminal_ms: times.terminal,
      startup_shells_ms: times.shells,
      settle_ms: settleMs,
      flood,
      processes,
      footprint_error: fp.error,
      frontend_errors: [...watcher.frontendErrors],
      frontend_errors_dropped: watcher.dropped,
      webkit_exited_with_app: webkitLeft.length === 0,
      shells_left_after_exit: shellsLeft,
    };
  } finally {
    if (child !== undefined && child.exitCode === null && child.signalCode === null) {
      child.kill("SIGKILL");
      await waitExit(child, 5_000);
    }
    process.off("exit", killOnExit);
    if (ownsTempRoot) await rm(tempRoot, { recursive: true, force: true });
  }
}

/**
 * アプリを組(scenarios)ごとに runs 回起動して測る。起動の前ごとにシステムのメモリの空きを見て、
 * minFreePercent を下回ったら残りを測らずに error を返す(それまでの結果は runs に残す)。
 * @returns {Promise<{ status: "ok" | "skipped" | "error", runs: object[], note?: string, error?: string }>}
 */
export async function measureAppMemory({
  appPath = DEFAULT_APP_PATH,
  realStateDir = DEFAULT_REAL_STATE_DIR,
  scenarios = APP_SCENARIOS,
  settleMs = 3_000,
  timeoutMs = 30_000,
  floodTimeoutMs = 60_000,
  floodBytes = FLOOD_BYTES,
  minFreePercent = DEFAULT_MIN_FREE_PERCENT,
  freePercent = systemFreePercent,
  log = console.log,
} = {}) {
  if (!(await exists(appPath))) {
    return { status: "skipped", runs: [], note: `未計測(${appPath} が無い。\`npm run app\` が先)` };
  }
  const executable = await readBundleExecutable(appPath);
  if (!(await exists(executable))) {
    return { status: "skipped", runs: [], note: `未計測(${executable} が無い。\`npm run app\` が先)` };
  }
  const realBefore = await describeDir(realStateDir);
  const results = [];
  let stopped = null;
  outer: for (const scenario of scenarios) {
    for (let i = 1; i <= scenario.runs; i += 1) {
      const free = freePercent();
      if (free !== null && free < minFreePercent) {
        stopped = `システムのメモリの空きが ${free}%(下限 ${minFreePercent}%)なので、${scenario.name} の ${i} 回目から先を測らずに止めた`;
        log?.(`  ${stopped}`);
        break outer;
      }
      results.push({ run: i, free_percent_before: free, ...(await measureOnce({ executable, scenario, settleMs, timeoutMs, floodTimeoutMs, floodBytes, log })) });
    }
  }
  const realAfter = await describeDir(realStateDir);
  if (JSON.stringify(realBefore) !== JSON.stringify(realAfter)) {
    return { status: "error", runs: results, error: `本物の状態フォルダ ${realStateDir} が作られたか変わった` };
  }
  const leftover = results.flatMap((r) => r.shells_left_after_exit);
  if (leftover.length > 0) {
    return { status: "error", runs: results, error: `アプリの終了後もシェル ${leftover.join("、")} が残っていた(SIGKILL で終了させた)` };
  }
  if (stopped !== null) return { status: "error", runs: results, error: stopped };
  const errors = results.reduce((n, r) => n + r.frontend_errors.length + r.frontend_errors_dropped, 0);
  const errorNote = errors > 0 ? `。画面の失敗の行 ${errors} 行(JSON の frontend_errors)` : "";
  return { status: "ok", runs: results, note: `実行ファイル ${path.relative(process.cwd(), executable) || executable}${errorNote}` };
}
