#!/usr/bin/env node
// 実アプリの起動スモーク(設計書 §10.1)。`npm run smoke` で実行する。
//
// ビルド済みの Arin.app を、一時の状態フォルダを指す環境変数を付けて起動し、Rust が状態フォルダに書く 2 つの印を待つ。
//
// - smoke-ready: 画面が Rust へ準備完了を通知した
// - smoke-terminal: PTY の出力が画面に届き、ターミナルのエンジン(xterm.js)が取り込み、処理済みの量が Rust に戻った
//
// 2 つが現れたら少し待ち(CSP 違反などは描画のときに出るため)、そのあいだも含めてアプリの標準エラーに
// 画面の失敗の行(頭が FRONTEND_ERROR_PREFIX。src-tauri/src/diagnostics.rs)が 1 行でもあれば失敗にする。
// 最後にアプリを終了させ、アプリが起動したシェル(アプリの子プロセス)が残っていないことを確かめて後片付けをする。
//
// - ソケットやポートは使わない(PRD N2・NFR-008)。外から知る手段は印のファイルと標準エラーだけ
// - .app が無ければビルドせずに失敗する(先に `npm run app`)
// - 本物の ~/Library/Application Support/Arin/ が作られていない・変わっていないことを前後で確かめる
// - シェルは利用者の設定ファイル(~/.zshrc など)を読む(M1 の仕様どおり)。ホームに書き込む操作はしない
// - Node の標準モジュールだけで書く
//
// 使い方: node scripts/smoke.mjs [--timeout-ms=30000] [--app=<Arin.app のパス>] [--env=<名前>=<値> ...]
//
// --env はアプリに渡す環境変数を足す(何度でも指定できる)。例: --env=ARIN_RENDERER=webgl(T21 の描画方式の指定)。
// WebGL を作れずに DOM へ戻ると画面が診断に 1 行出すので、そのスモークは失敗になる(黙って DOM で通らない)。
// 状態フォルダとスモークの印の環境変数(ARIN_STATE_DIR・ARIN_SMOKE)は、このスクリプトが決めるので指定できない。

import { spawn, spawnSync } from "node:child_process";
import { rmSync } from "node:fs";
import { access, mkdtemp, readFile, readdir, rm, stat } from "node:fs/promises";
import { homedir, tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

/** 準備完了の印のファイル名。crates/arin-core/src/paths/app_paths.rs の SMOKE_READY_MARKER_FILE_NAME と合わせる。 */
export const MARKER_FILE_NAME = "smoke-ready";
/** ターミナルの印のファイル名。crates/arin-core/src/paths/app_paths.rs の SMOKE_TERMINAL_MARKER_FILE_NAME と合わせる。 */
export const TERMINAL_MARKER_FILE_NAME = "smoke-terminal";
/** 画面の失敗の行の頭。src-tauri/src/diagnostics.rs の FRONTEND_ERROR_PREFIX と合わせる。 */
export const FRONTEND_ERROR_PREFIX = "[arin:frontend-error]";
/** 状態フォルダの差し替え(src-tauri/src/launch.rs の ENV_STATE_DIR)。 */
export const ENV_STATE_DIR = "ARIN_STATE_DIR";
/** スモークの印を有効にする(src-tauri/src/launch.rs の ENV_SMOKE)。 */
export const ENV_SMOKE = "ARIN_SMOKE";
/** 一時の状態フォルダの名前の頭。 */
export const TEMP_PREFIX = "arin-smoke-";

export const DEFAULT_APP_PATH = path.join(
  REPO_ROOT,
  "target/aarch64-apple-darwin/release/bundle/macos/Arin.app",
);
export const DEFAULT_REAL_STATE_DIR = path.join(homedir(), "Library", "Application Support", "Arin");
export const DEFAULT_TIMEOUT_MS = 30_000;
/** 印が揃ってから判定までに待つ時間(描画のときに出る CSP 違反などを拾うため)。 */
export const DEFAULT_SETTLE_MS = 1_000;
/** アプリの終了のあと、アプリが起動したシェルが終わるのを待つ上限。 */
export const DEFAULT_CHILD_EXIT_MS = 5_000;
/** 失敗の報告に載せる画面の失敗の行の数の上限。 */
const MAX_FRONTEND_ERRORS = 20;

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

async function exists(p) {
  try {
    await access(p);
    return true;
  } catch {
    return false;
  }
}

/** .app の Info.plist から実行ファイルの場所を読む。 */
export async function readBundleExecutable(appPath) {
  const plist = await readFile(path.join(appPath, "Contents", "Info.plist"), "utf8");
  const m = /<key>CFBundleExecutable<\/key>\s*<string>([^<]+)<\/string>/.exec(plist);
  if (m === null) {
    throw new Error(`${appPath} の Info.plist に CFBundleExecutable が無い`);
  }
  const name = m[1].trim();
  if (name === "" || name.includes("/") || name === "." || name === "..") {
    throw new Error(`${appPath} の CFBundleExecutable が不正: ${name}`);
  }
  return path.join(appPath, "Contents", "MacOS", name);
}

/** フォルダの中身(名前・大きさ・更新時刻)を再帰的に写し取る。無ければ null。比較だけに使う。 */
export async function describeDir(dir) {
  let top;
  try {
    top = await stat(dir);
  } catch (e) {
    if (e.code === "ENOENT") return null;
    throw e;
  }
  const entries = [];
  if (top.isDirectory()) {
    for (const name of (await readdir(dir, { recursive: true })).sort()) {
      const s = await stat(path.join(dir, name));
      entries.push(`${name}\t${s.size}\t${s.mtimeMs}`);
    }
  }
  return { mtimeMs: top.mtimeMs, entries };
}

/** 子プロセスが終わるのを待つ。ms 以内に終われば true。 */
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

/** 子プロセスを終了させる。SIGTERM で終わらなければ SIGKILL。 */
async function terminate(child, graceMs) {
  if (await waitExit(child, 0)) return "already";
  child.kill("SIGTERM");
  if (await waitExit(child, graceMs)) return "SIGTERM";
  child.kill("SIGKILL");
  if (await waitExit(child, 5_000)) return "SIGKILL";
  throw new Error(`プロセス ${child.pid} を終了させられなかった`);
}

/** プロセスが生きているか(シグナル 0 を送れるか。権限が無いだけなら生きている)。 */
export function isAlive(pid) {
  try {
    process.kill(pid, 0);
    return true;
  } catch (e) {
    return e.code === "EPERM";
  }
}

/** pid の直接の子プロセスの ID の一覧(pgrep -P)。 */
export function childPids(pid) {
  const r = spawnSync("pgrep", ["-P", String(pid)], { encoding: "utf8" });
  if (r.error) throw new Error(`pgrep を実行できなかった: ${r.error.message}`);
  // 終了コード 1 は「該当なし」。
  if (r.status !== 0 && r.status !== 1) throw new Error(`pgrep が失敗した(終了コード ${r.status}): ${r.stderr}`);
  return r.stdout
    .split("\n")
    .map((l) => Number(l.trim()))
    .filter((n) => Number.isInteger(n) && n > 0);
}

/** プロセスの名前(ps)。分からなければ "?"。 */
function processName(pid) {
  const r = spawnSync("ps", ["-o", "comm=", "-p", String(pid)], { encoding: "utf8" });
  return r.status === 0 && r.stdout.trim() !== "" ? r.stdout.trim() : "?";
}

/**
 * 標準出力・標準エラーの流れを行に分け、画面の失敗の行を集める。末尾の出力も残す(失敗の報告に添える)。
 */
export function createOutputWatcher() {
  const partial = new Map();
  const frontendErrors = [];
  let tail = "";
  let dropped = 0;
  const onLine = (line) => {
    if (!line.startsWith(FRONTEND_ERROR_PREFIX)) return;
    if (frontendErrors.length < MAX_FRONTEND_ERRORS) frontendErrors.push(line);
    else dropped += 1;
  };
  return {
    frontendErrors,
    get dropped() {
      return dropped;
    },
    get tail() {
      return tail;
    },
    /** 流れの名前(stdout・stderr)ごとに、届いた塊を渡す。 */
    feed(stream, chunk) {
      const text = (partial.get(stream) ?? "") + chunk.toString();
      tail = (tail + chunk.toString()).slice(-4000);
      const lines = text.split("\n");
      partial.set(stream, lines.pop());
      for (const line of lines) onLine(line.replace(/\r$/, ""));
    },
    /** 改行で終わっていない最後の行も調べる。 */
    end() {
      for (const rest of partial.values()) if (rest) onLine(rest);
      partial.clear();
    },
  };
}

/**
 * スモークを 1 回行う。
 * @returns {Promise<{ ok: boolean, message: string, seconds: number, pid?: number, shells?: number[] }>}
 */
export async function runSmoke({
  appPath = DEFAULT_APP_PATH,
  realStateDir = DEFAULT_REAL_STATE_DIR,
  timeoutMs = DEFAULT_TIMEOUT_MS,
  settleMs = DEFAULT_SETTLE_MS,
  childExitMs = DEFAULT_CHILD_EXIT_MS,
  pollMs = 100,
  killGraceMs = 5_000,
  extraEnv = {},
  log = console.log,
} = {}) {
  const started = Date.now();
  const seconds = () => (Date.now() - started) / 1000;
  const done = (ok, message, pid) => ({ ok, message, seconds: seconds(), pid });

  if (!(await exists(appPath))) {
    return done(false, `${appPath} が無い。先に \`npm run app\` を実行する(このスクリプトはビルドしない)`);
  }
  let executable;
  try {
    executable = await readBundleExecutable(appPath);
  } catch (e) {
    return done(false, `.app の実行ファイルを決められなかった: ${e.message}`);
  }
  if (!(await exists(executable))) {
    return done(false, `${executable} が無い。\`npm run app\` でビルドし直す`);
  }

  const realBefore = await describeDir(realStateDir);
  const base = tmpdir();
  const stateDir = await mkdtemp(path.join(base, TEMP_PREFIX));
  const readyMarker = path.join(stateDir, MARKER_FILE_NAME);
  const terminalMarker = path.join(stateDir, TERMINAL_MARKER_FILE_NAME);

  let child;
  /** アプリが起動したシェル(アプリの直接の子プロセス)。終了の直前に調べる。 */
  let shells = [];
  /** 子プロセスの ID → 名前(終わったあとでは引けないので、調べたときに引く)。 */
  const names = new Map();
  const watcher = createOutputWatcher();
  // 自分が mkdtemp で作ったフォルダか(消してよいか)。
  const ownsStateDir = path.dirname(stateDir) === base && path.basename(stateDir).startsWith(TEMP_PREFIX);
  // スクリプトが途中で抜けても(Ctrl+C で process.exit したときを含む)アプリと一時フォルダを取り残さないための最後の砦。
  // exit のハンドラでは非同期の処理を待てないので、同期の API だけを使う。
  const killOnExit = () => {
    child?.kill("SIGKILL");
    for (const pid of shells) if (isAlive(pid)) process.kill(pid, "SIGKILL");
    if (ownsStateDir) rmSync(stateDir, { recursive: true, force: true });
  };
  process.on("exit", killOnExit);

  let result;
  try {
    log(`起動: ${executable}`);
    log(`状態フォルダ: ${stateDir}`);
    for (const [name, value] of Object.entries(extraEnv)) log(`追加の環境変数: ${name}=${value}`);
    child = spawn(executable, [], {
      env: { ...process.env, ...extraEnv, [ENV_STATE_DIR]: stateDir, [ENV_SMOKE]: "1" },
      stdio: ["ignore", "pipe", "pipe"],
    });
    child.stdout.on("data", (chunk) => watcher.feed("stdout", chunk));
    child.stderr.on("data", (chunk) => watcher.feed("stderr", chunk));
    let spawnError = null;
    child.on("error", (e) => {
      spawnError = e;
    });

    const deadline = started + timeoutMs;
    let outcome = "timeout";
    const seen = { ready: false, terminal: false };
    while (Date.now() < deadline) {
      if (spawnError !== null) {
        outcome = "spawn-error";
        break;
      }
      seen.ready ||= await exists(readyMarker);
      seen.terminal ||= await exists(terminalMarker);
      if (seen.ready && seen.terminal) {
        outcome = "markers";
        break;
      }
      if (watcher.frontendErrors.length > 0) {
        outcome = "frontend-error";
        break;
      }
      if (child.exitCode !== null || child.signalCode !== null) {
        outcome = "exited";
        break;
      }
      await sleep(pollMs);
    }
    // 印が揃っても、画面の失敗が出ても、少し待ってから判定する(描画のときの CSP 違反などが続けて出るため)。
    if (outcome === "markers" || outcome === "frontend-error") await sleep(settleMs);

    if (outcome === "spawn-error") {
      result = done(false, `アプリを起動できなかった: ${spawnError.message}`);
    } else if (watcher.frontendErrors.length > 0) {
      const more = watcher.dropped > 0 ? `\n(ほか ${watcher.dropped} 行)` : "";
      result = done(
        false,
        `画面で失敗が起きた(標準エラーの ${FRONTEND_ERROR_PREFIX} の行):\n${watcher.frontendErrors.join("\n")}${more}`,
        child.pid,
      );
    } else if (outcome === "markers") {
      const ready = (await readFile(readyMarker, "utf8")).trim();
      const terminal = (await readFile(terminalMarker, "utf8")).trim();
      if (ready !== String(child.pid) || terminal !== String(child.pid)) {
        result = done(
          false,
          `印の中身(準備完了 ${ready}、ターミナル ${terminal})が起動したプロセス ID(${child.pid})と違う。別のプロセスが書いた可能性がある`,
          child.pid,
        );
      } else {
        result = done(
          true,
          `準備完了とターミナルの印を確認し、画面の失敗も無かった(${seconds().toFixed(1)} 秒)`,
          child.pid,
        );
      }
    } else if (outcome === "exited") {
      result = done(
        false,
        `印が揃う前にアプリが終了した(終了コード ${child.exitCode}、シグナル ${child.signalCode})`,
        child.pid,
      );
    } else {
      const missing = [
        seen.ready ? null : `準備完了の印(${MARKER_FILE_NAME})`,
        seen.terminal ? null : `ターミナルの印(${TERMINAL_MARKER_FILE_NAME}。PTY の出力が画面で取り込まれた印)`,
      ].filter((m) => m !== null);
      result = done(false, `${timeoutMs} ミリ秒以内に ${missing.join("と")} が現れなかった`, child.pid);
    }
  } finally {
    if (child?.pid !== undefined) {
      if (child.exitCode === null && child.signalCode === null) {
        try {
          shells = childPids(child.pid);
          for (const pid of shells) names.set(pid, processName(pid));
        } catch (e) {
          log(`アプリの子プロセスを調べられなかった: ${e.message}`);
        }
      }
      const how = await terminate(child, killGraceMs);
      log(`アプリを終了させた(${how})`);
    }
    process.off("exit", killOnExit);
    // 自分が mkdtemp で作ったフォルダだけを消す。
    if (ownsStateDir) {
      await rm(stateDir, { recursive: true, force: true });
    }
  }
  watcher.end();

  // アプリの終了で PTY が閉じ、シェルは SIGHUP で終わるはず。残っていたら失敗にして、取り残さないよう終了させる。
  const exitDeadline = Date.now() + childExitMs;
  let leftover = shells.filter(isAlive);
  while (leftover.length > 0 && Date.now() < exitDeadline) {
    await sleep(pollMs);
    leftover = leftover.filter(isAlive);
  }
  if (leftover.length > 0) {
    for (const pid of leftover) {
      try {
        process.kill(pid, "SIGKILL");
      } catch {
        // 既に終わっていればよい。
      }
    }
    const list = leftover.map((pid) => `${pid}(${names.get(pid)})`).join("、");
    result = { ...done(false, `アプリの終了後も、アプリが起動した子プロセスが残っていた: ${list}(SIGKILL で終了させた)`, result.pid) };
  } else if (shells.length > 0) {
    log(`アプリが起動した子プロセス ${shells.map((pid) => `${pid}(${names.get(pid)})`).join("、")} はアプリの終了後に終わった`);
  }
  result.shells = shells;

  const realAfter = await describeDir(realStateDir);
  if (JSON.stringify(realBefore) !== JSON.stringify(realAfter)) {
    const what = realBefore === null ? "作られた" : "変わった";
    return done(false, `本物の状態フォルダ ${realStateDir} が${what}(スモークは一時フォルダだけを使うはず)`, result.pid);
  }
  log(
    realAfter === null
      ? `本物の状態フォルダ ${realStateDir} は作られていない`
      : `本物の状態フォルダ ${realStateDir} は変わっていない`,
  );

  if (!result.ok && watcher.tail.trim() !== "") {
    result.message += `\n--- アプリの出力(末尾) ---\n${watcher.tail.trimEnd()}`;
  }
  return result;
}

/** コマンドラインの引数を runSmoke の引数にする。不正なら例外。 */
export function parseArgs(argv) {
  const opts = {};
  for (const arg of argv) {
    const m = /^--(timeout-ms|app|env)=(.+)$/.exec(arg);
    if (m === null) throw new Error(`知らない引数: ${arg}`);
    if (m[1] === "env") {
      const e = /^([A-Za-z_][A-Za-z0-9_]*)=(.*)$/.exec(m[2]);
      if (e === null) throw new Error(`--env は <名前>=<値> の形: ${m[2]}`);
      if (e[1] === ENV_STATE_DIR || e[1] === ENV_SMOKE) {
        throw new Error(`${e[1]} はスモークが決めるので --env で指定できない`);
      }
      opts.extraEnv = { ...opts.extraEnv, [e[1]]: e[2] };
    } else if (m[1] === "timeout-ms") {
      const n = Number(m[2]);
      if (!Number.isInteger(n) || n <= 0) throw new Error(`--timeout-ms は正の整数: ${m[2]}`);
      opts.timeoutMs = n;
    } else {
      opts.appPath = path.resolve(m[2]);
    }
  }
  return opts;
}

async function main() {
  let opts;
  try {
    opts = parseArgs(process.argv.slice(2));
  } catch (e) {
    console.error(`スモーク: ${e.message}`);
    return 2;
  }
  // Ctrl+C などで止められたときも、exit のハンドラでアプリを終了させてから抜ける。
  for (const sig of ["SIGINT", "SIGTERM", "SIGHUP"]) {
    process.once(sig, () => {
      console.error(`スモーク: ${sig} を受けたので中断する`);
      process.exit(130);
    });
  }
  const result = await runSmoke(opts);
  if (result.ok) {
    console.log(`スモーク成功: ${result.message}`);
    return 0;
  }
  console.error(`スモーク失敗: ${result.message}`);
  return 1;
}

if (process.argv[1] !== undefined && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().then(
    (code) => {
      process.exitCode = code;
    },
    (e) => {
      console.error(`スモーク失敗(予期しない例外): ${e?.stack ?? e}`);
      process.exitCode = 1;
    },
  );
}
