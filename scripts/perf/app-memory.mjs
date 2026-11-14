// 性能の計測: 起動中のアプリのメモリと、起動からターミナルの印までの時間(T24。NFR-001・NFR-002 の参考)。
// 記録するだけで合否にしない。`scripts/perf/run.mjs` から呼ぶ。
//
// 起動のしかたはスモーク(scripts/smoke.mjs)と同じ: 一時の状態フォルダ(ARIN_STATE_DIR)と ARIN_SMOKE=1 を付けて
// .app の実行ファイルを起動し、準備完了とターミナルの印を待つ。印が揃ってから少し置いて、次のプロセスを測る。
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

import { spawn, spawnSync } from "node:child_process";
import { rmSync } from "node:fs";
import { access, mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";

import {
  DEFAULT_APP_PATH,
  DEFAULT_REAL_STATE_DIR,
  ENV_SMOKE,
  ENV_STATE_DIR,
  MARKER_FILE_NAME,
  TERMINAL_MARKER_FILE_NAME,
  childPids,
  describeDir,
  isAlive,
  readBundleExecutable,
} from "../smoke.mjs";

export const TEMP_PREFIX = "arin-perf-app-";
const WEBKIT_RE = /\/(com\.apple\.WebKit\.[A-Za-z]+)$/;

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

async function exists(p) {
  try {
    await access(p);
    return true;
  } catch {
    return false;
  }
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

/** 1 回起動して測る。 */
async function measureOnce({ executable, settleMs, timeoutMs, log }) {
  const before = webkitProcesses();
  const base = tmpdir();
  const stateDir = await mkdtemp(path.join(base, TEMP_PREFIX));
  const ownsStateDir = path.dirname(stateDir) === base && path.basename(stateDir).startsWith(TEMP_PREFIX);
  let child;
  let shells = [];
  const killOnExit = () => {
    child?.kill("SIGKILL");
    for (const pid of shells) if (isAlive(pid)) process.kill(pid, "SIGKILL");
    if (ownsStateDir) rmSync(stateDir, { recursive: true, force: true });
  };
  process.on("exit", killOnExit);
  try {
    const started = performance.now();
    child = spawn(executable, [], {
      env: { ...process.env, [ENV_STATE_DIR]: stateDir, [ENV_SMOKE]: "1" },
      stdio: "ignore",
    });
    let spawnError = null;
    child.on("error", (e) => {
      spawnError = e;
    });
    const times = { ready: null, terminal: null };
    while (performance.now() - started < timeoutMs) {
      if (spawnError !== null) throw new Error(`アプリを起動できなかった: ${spawnError.message}`);
      if (child.exitCode !== null || child.signalCode !== null) {
        throw new Error(`印が揃う前にアプリが終了した(終了コード ${child.exitCode}、シグナル ${child.signalCode})`);
      }
      const now = performance.now() - started;
      if (times.ready === null && (await exists(path.join(stateDir, MARKER_FILE_NAME)))) times.ready = now;
      if (times.terminal === null && (await exists(path.join(stateDir, TERMINAL_MARKER_FILE_NAME)))) times.terminal = now;
      if (times.ready !== null && times.terminal !== null) break;
      await sleep(10);
    }
    if (times.ready === null || times.terminal === null) {
      throw new Error(`${timeoutMs} ミリ秒以内に印が揃わなかった(準備完了 ${times.ready !== null}、ターミナル ${times.terminal !== null})`);
    }
    await sleep(settleMs);

    const after = webkitProcesses();
    const webkit = [...after.keys()].filter((pid) => !before.has(pid));
    shells = childPids(child.pid);
    const all = [child.pid, ...webkit, ...shells];
    const fp = await footprintOf(all, stateDir);
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
    log?.(`  アプリ ${child.pid}: ターミナルの印まで ${times.terminal.toFixed(0)} ms`);
    return {
      startup_ready_ms: times.ready,
      startup_terminal_ms: times.terminal,
      settle_ms: settleMs,
      processes,
      footprint_error: fp.error,
      webkit_exited_with_app: webkitLeft.length === 0,
      shells_left_after_exit: shellsLeft,
    };
  } finally {
    if (child !== undefined && child.exitCode === null && child.signalCode === null) {
      child.kill("SIGKILL");
      await waitExit(child, 5_000);
    }
    process.off("exit", killOnExit);
    if (ownsStateDir) await rm(stateDir, { recursive: true, force: true });
  }
}

/**
 * アプリを runs 回起動して測る。
 * @returns {Promise<{ status: "ok" | "skipped" | "error", runs: object[], note?: string, error?: string }>}
 */
export async function measureAppMemory({
  appPath = DEFAULT_APP_PATH,
  realStateDir = DEFAULT_REAL_STATE_DIR,
  runs = 3,
  settleMs = 3_000,
  timeoutMs = 30_000,
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
  for (let i = 1; i <= runs; i += 1) {
    results.push({ run: i, ...(await measureOnce({ executable, settleMs, timeoutMs, log })) });
  }
  const realAfter = await describeDir(realStateDir);
  if (JSON.stringify(realBefore) !== JSON.stringify(realAfter)) {
    return { status: "error", runs: results, error: `本物の状態フォルダ ${realStateDir} が作られたか変わった` };
  }
  const leftover = results.flatMap((r) => r.shells_left_after_exit);
  if (leftover.length > 0) {
    return { status: "error", runs: results, error: `アプリの終了後もシェル ${leftover.join("、")} が残っていた(SIGKILL で終了させた)` };
  }
  return { status: "ok", runs: results, note: `実行ファイル ${path.relative(process.cwd(), executable) || executable}` };
}
