#!/usr/bin/env node
// 実アプリの起動スモーク(設計書 §10.1)。`npm run smoke` で実行する。
//
// ビルド済みの Arin.app を、一時の状態フォルダを指す環境変数を付けて起動し、画面が Rust へ準備完了を通知したときに
// Rust が書く印のファイル(状態フォルダの smoke-ready)が現れるのを待つ。現れたらアプリを終了させて後片付けをする。
//
// - ソケットやポートは使わない(PRD N2・NFR-008)。外から知る手段は印のファイルだけ
// - .app が無ければビルドせずに失敗する(先に `npm run app`)
// - 本物の ~/Library/Application Support/Arin/ が作られていない・変わっていないことを前後で確かめる
// - Node の標準モジュールだけで書く
//
// 使い方: node scripts/smoke.mjs [--timeout-ms=30000] [--app=<Arin.app のパス>]

import { spawn } from "node:child_process";
import { rmSync } from "node:fs";
import { access, mkdtemp, readFile, readdir, rm, stat } from "node:fs/promises";
import { homedir, tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

/** 印のファイル名。crates/arin-core/src/paths/app_paths.rs の SMOKE_READY_MARKER_FILE_NAME と合わせる。 */
export const MARKER_FILE_NAME = "smoke-ready";
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

/**
 * スモークを 1 回行う。
 * @returns {Promise<{ ok: boolean, message: string, seconds: number, pid?: number }>}
 */
export async function runSmoke({
  appPath = DEFAULT_APP_PATH,
  realStateDir = DEFAULT_REAL_STATE_DIR,
  timeoutMs = DEFAULT_TIMEOUT_MS,
  pollMs = 100,
  killGraceMs = 5_000,
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
  const marker = path.join(stateDir, MARKER_FILE_NAME);

  let child;
  let output = "";
  // 自分が mkdtemp で作ったフォルダか(消してよいか)。
  const ownsStateDir = path.dirname(stateDir) === base && path.basename(stateDir).startsWith(TEMP_PREFIX);
  // スクリプトが途中で抜けても(Ctrl+C で process.exit したときを含む)アプリと一時フォルダを取り残さないための最後の砦。
  // exit のハンドラでは非同期の処理を待てないので、同期の API だけを使う。
  const killOnExit = () => {
    child?.kill("SIGKILL");
    if (ownsStateDir) rmSync(stateDir, { recursive: true, force: true });
  };
  process.on("exit", killOnExit);

  let result;
  try {
    log(`起動: ${executable}`);
    log(`状態フォルダ: ${stateDir}`);
    child = spawn(executable, [], {
      env: { ...process.env, [ENV_STATE_DIR]: stateDir, [ENV_SMOKE]: "1" },
      stdio: ["ignore", "pipe", "pipe"],
    });
    const keep = (chunk) => {
      output = (output + chunk.toString()).slice(-4000);
    };
    child.stdout.on("data", keep);
    child.stderr.on("data", keep);
    let spawnError = null;
    child.on("error", (e) => {
      spawnError = e;
    });

    const deadline = started + timeoutMs;
    let outcome = "timeout";
    while (Date.now() < deadline) {
      if (spawnError !== null) {
        outcome = "spawn-error";
        break;
      }
      if (await exists(marker)) {
        outcome = "marker";
        break;
      }
      if (child.exitCode !== null || child.signalCode !== null) {
        outcome = "exited";
        break;
      }
      await sleep(pollMs);
    }

    if (outcome === "marker") {
      const text = (await readFile(marker, "utf8")).trim();
      if (text !== String(child.pid)) {
        result = done(
          false,
          `印の中身(${text})が起動したプロセス ID(${child.pid})と違う。別のプロセスが書いた可能性がある`,
          child.pid,
        );
      } else {
        result = done(true, `準備完了の印を確認した(${seconds().toFixed(1)} 秒)`, child.pid);
      }
    } else if (outcome === "spawn-error") {
      result = done(false, `アプリを起動できなかった: ${spawnError.message}`);
    } else if (outcome === "exited") {
      result = done(
        false,
        `準備完了の印を書く前にアプリが終了した(終了コード ${child.exitCode}、シグナル ${child.signalCode})`,
        child.pid,
      );
    } else {
      result = done(false, `${timeoutMs} ミリ秒以内に準備完了の印が現れなかった`, child.pid);
    }
  } finally {
    if (child?.pid !== undefined) {
      const how = await terminate(child, killGraceMs);
      log(`アプリを終了させた(${how})`);
    }
    process.off("exit", killOnExit);
    // 自分が mkdtemp で作ったフォルダだけを消す。
    if (ownsStateDir) {
      await rm(stateDir, { recursive: true, force: true });
    }
  }

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

  if (!result.ok && output.trim() !== "") {
    result.message += `\n--- アプリの出力(末尾) ---\n${output.trimEnd()}`;
  }
  return result;
}

function parseArgs(argv) {
  const opts = {};
  for (const arg of argv) {
    const m = /^--(timeout-ms|app)=(.+)$/.exec(arg);
    if (m === null) throw new Error(`知らない引数: ${arg}`);
    if (m[1] === "timeout-ms") {
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
