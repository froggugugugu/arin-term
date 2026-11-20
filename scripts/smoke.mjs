#!/usr/bin/env node
// 実アプリの起動スモーク(設計書 §10.1)。`npm run smoke` で実行する。
//
// ビルド済みの Arin.app を、一時の状態フォルダを指す環境変数を付けて起動し、Rust が状態フォルダに書く 2 つの印を待つ。
//
// - smoke-ready: 画面が Rust へ準備完了を通知した
// - smoke-terminal: PTY の出力が画面に届き、ターミナルのエンジン(xterm.js)が取り込み、処理済みの量が Rust に戻った
//
// 起動の前に、一時フォルダの中に状態フォルダ(state/)とプロジェクトのフォルダ(project/)を作り、プロジェクト 1 件の
// state.json(bookmark は null、lastKnownPath はプロジェクトのフォルダ。計画書 M2 の【仮定】13)を置く。
// 通常の起動はプロジェクトが無いとターミナルが 0 枚で印が書かれないため(T33a)。
//
// 2 つが現れたら少し待ち(CSP 違反などは描画のときに出るため)、そのあいだも含めてアプリの標準エラーに
// 画面の失敗の行(頭が FRONTEND_ERROR_PREFIX。src-tauri/src/diagnostics.rs)が 1 行でもあれば失敗にする。
// 最後にアプリを終了させ、アプリが起動したシェル(アプリの子プロセス)が残っていないことを確かめて後片付けをする。
// 終了後に状態フォルダを確かめる: state.json にブックマークが入り、プロジェクトのフォルダには何も増えていない。
//
// `npm run smoke` は続けてもう 1 回、壊れた state.json を置いて起動する(準備完了の印だけを待つ)。
// 退避のファイル(state.json.broken-<日時>)が元の中身のままでき、新しい state.json が空の状態で書かれていることを確かめる。
//
// `--tiles` は、M1 の 9 枚の検証画面(ARIN_VERIFY_GRID。計画書 T49c・T49d で撤去)の代わりに、9 プロジェクト・各 1 タブで
// `ui.displayMode: "tiles"` の state.json(計画書 M4 の【仮定】18。形は scripts/perf/app-memory.mjs の計測と同じ)を置いて
// 1 回だけ起動する。準備完了とターミナルの印に加えて、アプリの子プロセス(シェル)が 9 個揃うまで待つ(ターミナルの印は
// 最初の 1 枚の出力で書かれるので、9 枚ぶんは子プロセスの数で確かめる)。終了後は 9 個のシェルが残っていないこと、
// state.json の 9 件にブックマークが入り表示モードがタイルのままであること、9 つのプロジェクトのフォルダに何も
// 増えていないことを確かめる。
//
// `--settings` は、プロジェクト 1 件の state.json に加えて、正しい変更の行と問題のある行を 1 つずつ含む settings.toml
// (SETTINGS_FIXTURE。計画書 T66a、ADR-012)を状態フォルダに置いて 1 回だけ起動する。準備完了とターミナルの印を待ち、
// 標準エラーの設定の行(頭が SETTINGS_DIAGNOSTIC_PREFIX)が問題のある行のコマンドの ID とキーの文字列と理由だけで
// (設定の行の全文・コメント・絶対パスを含まない。AC-K09)、正しい行のことは出ていないことを確かめる。終了後は、
// 設定ファイルの中身(SHA-256)と大きさ・更新日時が置いたときと同じで、settings.toml の名前を含む別のファイル(退避など)が
// できていないことを確かめる(アプリは設定ファイルを読むだけ。AC-K05・AC-K10)。メニューの「設定ファイルを開く…」は選ばない
// (本物の既定のアプリや Finder が開くため)。--settings 以外の起動は settings.toml を置かないので、設定の行が 1 行でも
// 出たら失敗にする(ファイルが無ければ既定で知らせなし。AC-K03)。
//
// `--fonts` は、プロジェクト 1 件の state.json の ui に既定でない文字の大きさ(FONT_SIZES_FIXTURE。ターミナル 20・画面 15。
// 計画書 T71、ADR-013 の 1)を入れて 1 回だけ起動する。準備完了とターミナルの印を待ち、画面の失敗が無いことを確かめる。
// 終了後は、state.json に同じ 2 つの値が残り(既定でないので書かれ続ける。AC-F08)、版が 1 のまま、退避のファイルが
// できておらず、状態フォルダに settings.toml ができていない(文字の大きさは設定ファイルに書かない。AC-F10)ことを確かめる。
// 設定画面は開かず、メニューの「設定ファイルを開く…」も選ばない(本物の既定のアプリや Finder が開くため)。
//
// - ソケットやポートは使わない(PRD N2・NFR-008)。外から知る手段は印のファイルと標準エラーだけ
// - .app が無ければビルドせずに失敗する(先に `npm run app`)
// - 本物の ~/Library/Application Support/Arin/ が作られていない・変わっていないことを前後で確かめる
// - シェルは利用者の設定ファイル(~/.zshrc など)を読む(M1 の仕様どおり)。ホームに書き込む操作はしない
// - Node の標準モジュールだけで書く
//
// 使い方: node scripts/smoke.mjs [--tiles | --settings | --fonts] [--timeout-ms=30000] [--app=<Arin.app のパス>] [--env=<名前>=<値> ...]
//
// --env はアプリに渡す環境変数を足す(何度でも指定できる)。値は特別に扱わず、そのまま渡す。
// 状態フォルダとスモークの印の環境変数(ARIN_STATE_DIR・ARIN_SMOKE)は、このスクリプトが決めるので指定できない。
// M1 の検証用の起動時の指定(ARIN_VERIFY_GRID・ARIN_RENDERER・ARIN_UNICODE)は M4 で外した(T49c・T49d・T50b。ADR-001)。
// 渡してもアプリは読まず、DOM 描画と unicode11 の通常の起動になる(例: --env=ARIN_RENDERER=webgl で確かめられる)。

import { spawn, spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { rmSync } from "node:fs";
import { access, mkdir, mkdtemp, readFile, readdir, realpath, rm, stat, writeFile } from "node:fs/promises";
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
/** --tiles のプロジェクトの数(各 1 タブ。1 ページに並ぶ上限の 9 枚)。 */
export const TILE_PROJECT_COUNT = 9;
/** プロジェクトに割り当てる色(crates/arin-core/src/project/color.rs のパレットの先頭から)。 */
const PROJECT_COLORS = ["red", "orange", "yellow", "green", "teal", "blue", "purple", "pink"];
/** 状態ファイルの名前(crates/arin-core/src/ports/state_file.rs の STATE_FILE_NAME)。 */
export const STATE_FILE_NAME = "state.json";
/** 退避の名前の頭(crates/arin-core/src/persistence/codec.rs の broken_file_name)。 */
export const BROKEN_PREFIX = `${STATE_FILE_NAME}.broken-`;
/** スモークが置くプロジェクトの ID(UUID v4 の形)。 */
export const SMOKE_PROJECT_ID = "5b0c6a2e-1f3d-4e8a-9c7b-2d4e6f8a0b1c";
/** 壊れた state.json の中身(JSON として読めない)。 */
export const BROKEN_STATE = '{ "schemaVersion": 1, "projects": [';
/** 設定ファイルの名前(crates/arin-core/src/paths/app_paths.rs の SETTINGS_FILE_NAME)。 */
export const SETTINGS_FILE_NAME = "settings.toml";
/** 設定の標準エラーの行の頭。src-tauri/src/settings.rs の DIAGNOSTIC_PREFIX と合わせる。 */
export const SETTINGS_DIAGNOSTIC_PREFIX = "[arin:settings]";
/** --settings の設定ファイルのコメントに入れる目印(標準エラーに出たら、設定ファイルの中身が漏れている)。 */
export const SETTINGS_COMMENT_MARK = "arin-smoke-settings-comment";
/** --settings の正しい変更の行(既定の Cmd+T を替える。標準エラーには出ないはず)。 */
export const SETTINGS_GOOD_ROW = { id: "terminal.new-tab", key: "Cmd+Shift+Y" };
/** --settings の問題のある行(F キーは KeyChord の書式に無く読めないキー。既定に戻り、標準エラーに 1 行出るはず)。 */
export const SETTINGS_BAD_ROW = { id: "tree.rename", key: "F13" };
/** --settings で置く settings.toml の中身。 */
export const SETTINGS_FIXTURE = [
  `# スモークの設定(scripts/smoke.mjs --settings)。${SETTINGS_COMMENT_MARK}`,
  "[shortcuts]",
  `"${SETTINGS_GOOD_ROW.id}" = "${SETTINGS_GOOD_ROW.key}"`,
  `"${SETTINGS_BAD_ROW.id}" = "${SETTINGS_BAD_ROW.key}"`,
  "",
].join("\n");

/**
 * --fonts で state.json の ui に置く文字の大きさ(どちらも既定(15・13)でなく範囲(9〜28・11〜18)の中。
 * crates/arin-core/src/layout/font_sizes.rs)。
 */
export const FONT_SIZES_FIXTURE = Object.freeze({ terminalFontSize: 20, interfaceFontSize: 15 });

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
 * 標準出力・標準エラーの流れを行に分け、画面の失敗の行と設定の行(頭が SETTINGS_DIAGNOSTIC_PREFIX)を集める。
 * 末尾の出力も残す(失敗の報告に添える)。
 */
export function createOutputWatcher() {
  const partial = new Map();
  const frontendErrors = [];
  const settingsLines = [];
  let tail = "";
  let dropped = 0;
  const onLine = (line) => {
    if (line.startsWith(SETTINGS_DIAGNOSTIC_PREFIX)) {
      if (settingsLines.length < MAX_FRONTEND_ERRORS) settingsLines.push(line);
      return;
    }
    if (!line.startsWith(FRONTEND_ERROR_PREFIX)) return;
    if (frontendErrors.length < MAX_FRONTEND_ERRORS) frontendErrors.push(line);
    else dropped += 1;
  };
  return {
    frontendErrors,
    settingsLines,
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

/** 複数のプロジェクトを置くときのプロジェクトの ID(UUID v4 の形。n は 1 から)。 */
export function numberedProjectId(n) {
  return `00000000-0000-4000-8000-${String(n).padStart(12, "0")}`;
}

/**
 * プロジェクトを projectDirs の順に並べた state.json の中身(bookmark は null。各 1 タブは起動時にアプリが開く)。
 * 先頭を選択中にする。displayMode を渡すと ui.displayMode に書く("tiles" でタイル表示で開く。計画書 T46c)。
 * scripts/perf/app-memory.mjs の計測もこの形を使う。
 */
export function projectsStateFixture(projectDirs, { displayMode = null } = {}) {
  if (projectDirs.length === 0) throw new Error("プロジェクトのフォルダが 1 つも無い");
  const projects = projectDirs.map((dir, i) => ({
    id: numberedProjectId(i + 1),
    bookmark: null,
    lastKnownPath: dir,
    customName: null,
    colorId: PROJECT_COLORS[i % PROJECT_COLORS.length],
  }));
  const ui = { selectedProjectId: projects[0].id, ...(displayMode === null ? {} : { displayMode }) };
  const state = { schemaVersion: 1, projects, ui };
  return `${JSON.stringify(state, null, 2)}\n`;
}

/**
 * 状態ファイルの中身を作る。kind が "project" ならプロジェクト 1 件(bookmark は null)、"tiles" なら projectDirs の
 * プロジェクトをタイル表示で(projectsStateFixture)、"broken" なら JSON として読めないもの。"fonts" は "project" の ui に
 * 文字の大きさ(FONT_SIZES_FIXTURE)を足したもの。
 * projectDirs はプロジェクトのフォルダの一覧(1 つなら文字列でもよい)。
 */
export function stateFixture(kind, dirs) {
  const projectDirs = Array.isArray(dirs) ? dirs : [dirs];
  if (kind === "broken") return BROKEN_STATE;
  if (kind === "tiles") return projectsStateFixture(projectDirs, { displayMode: "tiles" });
  // "settings" の state.json は "project" と同じ(設定ファイルは prepareStateDir が別に置く)。
  if (kind !== "project" && kind !== "settings" && kind !== "fonts") throw new Error(`知らない状態ファイルの種類: ${kind}`);
  const state = {
    schemaVersion: 1,
    projects: [
      { id: SMOKE_PROJECT_ID, bookmark: null, lastKnownPath: projectDirs[0], customName: null, colorId: "green" },
    ],
    ui: { selectedProjectId: SMOKE_PROJECT_ID, ...(kind === "fonts" ? FONT_SIZES_FIXTURE : {}) },
  };
  return `${JSON.stringify(state, null, 2)}\n`;
}

/** ファイルの中身の SHA-256(16 進)・大きさ・更新日時。比較だけに使う。 */
export async function fingerprintFile(file) {
  const [bytes, s] = await Promise.all([readFile(file), stat(file)]);
  return { sha256: createHash("sha256").update(bytes).digest("hex"), size: s.size, mtimeMs: s.mtimeMs };
}

/**
 * root の下に状態フォルダ(state/)とプロジェクトのフォルダを作り、state.json を置く。
 * プロジェクトのフォルダは "tiles" なら projects/p1〜p9、ほかは project/ の 1 つ。
 * "settings" なら settings.toml(SETTINGS_FIXTURE)も置き、置いたときの中身の指紋(fingerprintFile)を settings に返す
 * (ほかの種類では null)。
 * @returns {Promise<{ stateDir: string, projectDirs: string[], fixture: string,
 *   settings: { sha256: string, size: number, mtimeMs: number } | null }>}
 */
export async function prepareStateDir(root, kind) {
  const stateDir = path.join(root, "state");
  await mkdir(stateDir, { mode: 0o700 });
  const projectDirs =
    kind === "tiles"
      ? Array.from({ length: TILE_PROJECT_COUNT }, (_, i) => path.join(root, "projects", `p${i + 1}`))
      : [path.join(root, "project")];
  for (const dir of projectDirs) await mkdir(dir, { recursive: true });
  const fixture = stateFixture(kind, projectDirs);
  await writeFile(path.join(stateDir, STATE_FILE_NAME), fixture, { mode: 0o600 });
  let settings = null;
  if (kind === "settings") {
    const file = path.join(stateDir, SETTINGS_FILE_NAME);
    await writeFile(file, SETTINGS_FIXTURE, { mode: 0o600 });
    settings = await fingerprintFile(file);
  }
  return { stateDir, projectDirs, fixture, settings };
}

/**
 * 終了後の設定ファイルを確かめる。問題が無ければ null、あれば理由の文字列。
 * settings.toml の中身(SHA-256)・大きさ・更新日時が置いたとき(expected)と同じで、状態フォルダに settings.toml の名前を
 * 含むほかのファイル(退避・一時ファイルなど)ができていない(アプリは設定ファイルを読むだけ。AC-K05)。
 */
export async function checkSettingsAfterExit({ stateDir, expected }) {
  const others = (await readdir(stateDir)).filter((n) => n !== SETTINGS_FILE_NAME && n.includes(SETTINGS_FILE_NAME));
  if (others.length > 0) return `状態フォルダに ${SETTINGS_FILE_NAME} の名前を含む別のファイルができた: ${others.join("、")}`;
  let after;
  try {
    after = await fingerprintFile(path.join(stateDir, SETTINGS_FILE_NAME));
  } catch (e) {
    return `終了後に ${SETTINGS_FILE_NAME} を読めなかった(消えた・置き換わった?): ${e.message}`;
  }
  if (after.sha256 !== expected.sha256 || after.size !== expected.size) {
    return `${SETTINGS_FILE_NAME} の中身が変わった(読むだけのはず)`;
  }
  if (after.mtimeMs !== expected.mtimeMs) {
    return `${SETTINGS_FILE_NAME} の更新日時が変わった(${expected.mtimeMs} → ${after.mtimeMs}。読むだけのはず)`;
  }
  return null;
}

/** 正規表現の特別な文字を逃がす。 */
function escapeRegExp(text) {
  return text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

/**
 * 標準エラーの設定の行(頭が SETTINGS_DIAGNOSTIC_PREFIX)を確かめる。問題が無ければ null、あれば理由の文字列。
 * - expectBadRow が false("settings" 以外の起動。settings.toml が無い): 1 行も無い(AC-K03)
 * - expectBadRow が true: 1 行以上あり、どれも「ショートカットの設定: <問題のある行の ID> = <キー>(<理由>)」の形で、
 *   設定ファイルの行の全文(引用符付きの行)・コメント・正しい行の ID・絶対パス(forbiddenPaths)を含まない(AC-K09)
 */
export function checkSettingsDiagnostics(lines, { expectBadRow, forbiddenPaths = [] }) {
  if (!expectBadRow) {
    return lines.length === 0 ? null : `設定ファイルが無いのに、標準エラーに設定の行が出た:\n${lines.join("\n")}`;
  }
  if (lines.length === 0) {
    return `問題のある行(${SETTINGS_BAD_ROW.id} = ${SETTINGS_BAD_ROW.key})の設定の行(${SETTINGS_DIAGNOSTIC_PREFIX})が標準エラーに出なかった`;
  }
  const shape = new RegExp(
    `^${escapeRegExp(SETTINGS_DIAGNOSTIC_PREFIX)} ショートカットの設定: ${escapeRegExp(SETTINGS_BAD_ROW.id)} = ${escapeRegExp(SETTINGS_BAD_ROW.key)}[(\\uFF08][^()\\uFF08\\uFF09"/]+[)\\uFF09]$`,
  );
  const fixtureRows = SETTINGS_FIXTURE.split("\n").filter((l) => l.trim() !== "");
  for (const line of lines) {
    const leaked = fixtureRows.find((row) => line.includes(row));
    if (leaked !== undefined) return `設定の行に設定ファイルの行の全文が出た(${leaked}): ${line}`;
    if (line.includes(SETTINGS_COMMENT_MARK)) return `設定の行に設定ファイルのコメントが出た: ${line}`;
    const pathLeak = forbiddenPaths.find((p) => p !== "" && line.includes(p));
    if (pathLeak !== undefined) return `設定の行に絶対パス(${pathLeak})が出た: ${line}`;
    if (line.includes(SETTINGS_GOOD_ROW.id)) return `正しい行(${SETTINGS_GOOD_ROW.id})のことが設定の行に出た: ${line}`;
    if (!shape.test(line)) {
      return `設定の行が「ショートカットの設定: ${SETTINGS_BAD_ROW.id} = ${SETTINGS_BAD_ROW.key}(理由)」の形でない: ${line}`;
    }
  }
  return null;
}

/** JSON として読めれば値、読めなければ null。 */
function parseJson(text) {
  try {
    return JSON.parse(text);
  } catch {
    return null;
  }
}

/**
 * アプリの終了後の状態フォルダとプロジェクトのフォルダを確かめる。問題が無ければ null、あれば理由の文字列。
 * - "project": state.json のプロジェクトにブックマークが入り、lastKnownPath は正規化したパス
 * - "tiles": 置いた 9 件が同じ順で残り、どれにもブックマークが入り lastKnownPath は正規化したパス。表示モードはタイルのまま
 * - "broken": 退避のファイルが元の中身のままでき、state.json はプロジェクト 0 件
 * - "fonts": "project" と同じに加えて、ui の文字の大きさが置いた値(FONT_SIZES_FIXTURE)のまま残り、状態フォルダに
 *   settings.toml ができていない(AC-F08・AC-F10)
 * - どれも: プロジェクトのフォルダに何も増えていない(FR-001 の AC 3)
 */
export async function checkStateAfterExit({ stateDir, projectDirs, fixture, kind }) {
  for (const dir of projectDirs) {
    const added = await readdir(dir);
    if (added.length > 0) return `プロジェクトのフォルダ ${dir} に ${added.join("、")} ができた(何も作らないはず)`;
  }
  const names = await readdir(stateDir);
  const backups = names.filter((n) => n.startsWith(BROKEN_PREFIX));
  const statePath = path.join(stateDir, STATE_FILE_NAME);
  let text;
  try {
    text = await readFile(statePath, "utf8");
  } catch (e) {
    return `終了後に ${STATE_FILE_NAME} を読めなかった: ${e.message}`;
  }
  const state = parseJson(text);
  if (state === null || state.schemaVersion !== 1 || !Array.isArray(state.projects)) {
    return `終了後の ${STATE_FILE_NAME} が版 1 の形でない: ${text.slice(0, 200)}`;
  }
  if (kind === "broken") {
    if (backups.length !== 1) return `退避のファイル(${BROKEN_PREFIX}<日時>)が 1 つでない: ${backups.join("、") || "無い"}`;
    const kept = await readFile(path.join(stateDir, backups[0]), "utf8");
    if (kept !== fixture) return `退避のファイル ${backups[0]} の中身が元の state.json と違う`;
    if (state.projects.length !== 0) return `退避のあとの ${STATE_FILE_NAME} が空の状態でない`;
    return null;
  }
  if (backups.length > 0) return `読める ${STATE_FILE_NAME} を退避した: ${backups.join("、")}`;
  const placed = JSON.parse(fixture).projects.map((p) => p.id);
  const ids = state.projects.map((p) => p?.id);
  if (JSON.stringify(ids) !== JSON.stringify(placed)) {
    return `終了後の ${STATE_FILE_NAME} のプロジェクトが置いたものと違う: ${text.slice(0, 200)}`;
  }
  for (const [i, project] of state.projects.entries()) {
    if (typeof project.bookmark !== "string" || project.bookmark === "") {
      return `終了後の ${STATE_FILE_NAME} の ${i + 1} 件目にブックマークが入っていない(bookmark: ${JSON.stringify(project.bookmark)})`;
    }
    const canonical = await realpath(projectDirs[i]);
    if (project.lastKnownPath !== canonical) {
      return `${i + 1} 件目の lastKnownPath が正規化したパス ${canonical} でない: ${project.lastKnownPath}`;
    }
  }
  if (kind === "tiles" && state.ui?.displayMode !== "tiles") {
    return `終了後の ${STATE_FILE_NAME} の表示モードがタイルでない(ui.displayMode: ${JSON.stringify(state.ui?.displayMode)})`;
  }
  if (kind === "fonts") {
    for (const [key, value] of Object.entries(FONT_SIZES_FIXTURE)) {
      if (state.ui?.[key] !== value) {
        return `終了後の ${STATE_FILE_NAME} の ui.${key} が置いた ${value} でない(${JSON.stringify(state.ui?.[key])})`;
      }
    }
    if (names.includes(SETTINGS_FILE_NAME)) {
      return `状態フォルダに ${SETTINGS_FILE_NAME} ができた(文字の大きさは設定ファイルに書かないはず)`;
    }
  }
  return null;
}

/**
 * スモークを 1 回行う。
 * stateKind は起動の前に置く state.json の種類("project"・"tiles"・"broken"・"settings"・"fonts")。"broken" のときは準備完了の印だけを待つ
 * (プロジェクトが無いのでターミナルが無い)。"tiles" のときは、印に加えてアプリの子プロセス(シェル)が 9 個揃うまで待つ。
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
  stateKind = "project",
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
  const tempRoot = await mkdtemp(path.join(base, TEMP_PREFIX));
  const needTerminal = stateKind !== "broken";
  /** 揃うまで待つアプリの子プロセス(シェル)の数。null なら数えない。 */
  const needShells = stateKind === "tiles" ? TILE_PROJECT_COUNT : null;
  let prepared;
  try {
    prepared = await prepareStateDir(tempRoot, stateKind);
  } catch (e) {
    await rm(tempRoot, { recursive: true, force: true });
    return done(false, `状態フォルダを用意できなかった: ${e.message}`);
  }
  const { stateDir } = prepared;
  /** 終了後の状態フォルダの問題(無ければ null)。 */
  let stateProblem = null;
  const readyMarker = path.join(stateDir, MARKER_FILE_NAME);
  const terminalMarker = path.join(stateDir, TERMINAL_MARKER_FILE_NAME);

  let child;
  /** アプリが起動したシェル(アプリの直接の子プロセス)。終了の直前に調べる。 */
  let shells = [];
  /** 子プロセスの ID → 名前(終わったあとでは引けないので、調べたときに引く)。 */
  const names = new Map();
  const watcher = createOutputWatcher();
  // 自分が mkdtemp で作ったフォルダか(消してよいか)。
  const ownsStateDir = path.dirname(tempRoot) === base && path.basename(tempRoot).startsWith(TEMP_PREFIX);
  // スクリプトが途中で抜けても(Ctrl+C で process.exit したときを含む)アプリと一時フォルダを取り残さないための最後の砦。
  // exit のハンドラでは非同期の処理を待てないので、同期の API だけを使う。
  const killOnExit = () => {
    child?.kill("SIGKILL");
    for (const pid of shells) if (isAlive(pid)) process.kill(pid, "SIGKILL");
    if (ownsStateDir) rmSync(tempRoot, { recursive: true, force: true });
  };
  process.on("exit", killOnExit);

  let result;
  try {
    log(`起動: ${executable}`);
    const described = {
      broken: "壊れたもの",
      tiles: `プロジェクト ${TILE_PROJECT_COUNT} 件・タイル表示`,
      project: "プロジェクト 1 件",
      settings: `プロジェクト 1 件。${SETTINGS_FILE_NAME} に正しい行 1 つと問題のある行 1 つ`,
      fonts: `プロジェクト 1 件。ターミナルの文字 ${FONT_SIZES_FIXTURE.terminalFontSize}・画面の文字 ${FONT_SIZES_FIXTURE.interfaceFontSize}`,
    };
    log(`状態フォルダ: ${stateDir}(${STATE_FILE_NAME}: ${described[stateKind] ?? stateKind})`);
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
    const seen = { ready: false, terminal: false, shells: needShells === null };
    /** 最後に数えたアプリの子プロセスの数。 */
    let shellCount = 0;
    while (Date.now() < deadline) {
      if (spawnError !== null) {
        outcome = "spawn-error";
        break;
      }
      seen.ready ||= await exists(readyMarker);
      seen.terminal ||= await exists(terminalMarker);
      if (!seen.shells && seen.ready && seen.terminal && child.exitCode === null && child.signalCode === null) {
        shellCount = childPids(child.pid).length;
        seen.shells = shellCount >= needShells;
      }
      if (seen.ready && (seen.terminal || !needTerminal) && seen.shells) {
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
      const terminal = needTerminal ? (await readFile(terminalMarker, "utf8")).trim() : String(child.pid);
      if (ready !== String(child.pid) || terminal !== String(child.pid)) {
        result = done(
          false,
          `印の中身(準備完了 ${ready}、ターミナル ${terminal})が起動したプロセス ID(${child.pid})と違う。別のプロセスが書いた可能性がある`,
          child.pid,
        );
      } else {
        const what = !needTerminal
          ? "準備完了の印"
          : needShells === null
            ? "準備完了とターミナルの印"
            : `準備完了とターミナルの印、アプリが起動したシェル ${needShells} 個`;
        result = done(true, `${what}を確認し、画面の失敗も無かった(${seconds().toFixed(1)} 秒)`, child.pid);
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
        seen.terminal || !needTerminal ? null : `ターミナルの印(${TERMINAL_MARKER_FILE_NAME}。PTY の出力が画面で取り込まれた印)`,
        seen.shells ? null : `アプリが起動したシェル ${needShells} 個(数えたのは ${shellCount} 個)`,
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
    if (result?.ok) {
      try {
        stateProblem = await checkStateAfterExit({ ...prepared, kind: stateKind });
        if (stateProblem === null && prepared.settings !== null) {
          stateProblem = await checkSettingsAfterExit({ stateDir, expected: prepared.settings });
        }
      } catch (e) {
        stateProblem = `終了後の状態フォルダを調べられなかった: ${e.message}`;
      }
      if (stateProblem === null) {
        watcher.end();
        let forbiddenPaths = [tempRoot, stateDir];
        try {
          forbiddenPaths = [...forbiddenPaths, await realpath(tempRoot)];
        } catch {
          // 消す前なので読めるはず。読めなければ与えたパスだけで調べる。
        }
        stateProblem = checkSettingsDiagnostics(watcher.settingsLines, {
          expectBadRow: stateKind === "settings",
          forbiddenPaths,
        });
      }
    }
    // 自分が mkdtemp で作ったフォルダだけを消す。
    if (ownsStateDir) {
      await rm(tempRoot, { recursive: true, force: true });
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
  if (result.ok && stateProblem !== null) {
    result = { ...done(false, stateProblem, result.pid), shells };
  } else if (result.ok) {
    log(
      stateKind === "broken"
        ? `壊れた ${STATE_FILE_NAME} は元の中身のまま退避され、空の状態で書き直された`
        : stateKind === "tiles"
          ? `${STATE_FILE_NAME} の ${TILE_PROJECT_COUNT} 件にブックマークが書かれ、表示モードはタイルのまま、プロジェクトのフォルダには何も増えていない`
          : `${STATE_FILE_NAME} にブックマークが書かれ、プロジェクトのフォルダには何も増えていない`,
    );
    if (stateKind === "fonts") {
      log(
        `${STATE_FILE_NAME} の ui.terminalFontSize・ui.interfaceFontSize は ${FONT_SIZES_FIXTURE.terminalFontSize}・${FONT_SIZES_FIXTURE.interfaceFontSize} のまま、退避のファイルも ${SETTINGS_FILE_NAME} もできていない`,
      );
    }
    if (stateKind === "settings") {
      log(`標準エラーの設定の行(${watcher.settingsLines.length} 行):`);
      for (const line of watcher.settingsLines) log(`  ${line}`);
      log(`${SETTINGS_FILE_NAME} は中身・大きさ・更新日時とも置いたときのまま、名前を含む別のファイルもできていない`);
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

  if (!result.ok && watcher.tail.trim() !== "") {
    result.message += `\n--- アプリの出力(末尾) ---\n${watcher.tail.trimEnd()}`;
  }
  return result;
}

/**
 * スモークの一揃い。`npm run smoke` はプロジェクト 1 件の state.json で起動し、続けて壊れた state.json でも起動する。
 * tiles が true(--tiles)なら、9 プロジェクトのタイル表示の state.json で 1 回だけ起動する。
 * settings が true(--settings)なら、プロジェクト 1 件の state.json と settings.toml を置いて 1 回だけ起動する。
 * fonts が true(--fonts)なら、文字の大きさを入れたプロジェクト 1 件の state.json を置いて 1 回だけ起動する。
 * @returns {Promise<Array<{ kind: string, result: Awaited<ReturnType<typeof runSmoke>> }>>}
 */
export async function runSmokeSuite({ tiles = false, settings = false, fonts = false, ...opts } = {}) {
  if ([tiles, settings, fonts].filter(Boolean).length > 1) throw new Error(EXCLUSIVE_MESSAGE);
  const kinds = tiles ? ["tiles"] : settings ? ["settings"] : fonts ? ["fonts"] : ["project", "broken"];
  const results = [];
  for (const kind of kinds) {
    results.push({ kind, result: await runSmoke({ ...opts, stateKind: kind }) });
  }
  return results;
}

/** 1 回だけ起動する種類の指定(--tiles・--settings・--fonts)を並べたときの失敗の文。 */
const EXCLUSIVE_MESSAGE = "--tiles・--settings・--fonts は同時に指定できない";

/** コマンドラインの引数を runSmoke の引数にする。不正なら例外。 */
export function parseArgs(argv) {
  const opts = {};
  for (const arg of argv) {
    if (arg === "--tiles" || arg === "--settings" || arg === "--fonts") {
      opts[arg.slice(2)] = true;
      if ([opts.tiles, opts.settings, opts.fonts].filter(Boolean).length > 1) throw new Error(EXCLUSIVE_MESSAGE);
      continue;
    }
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
  let code = 0;
  for (const { kind, result } of await runSmokeSuite(opts)) {
    const labels = {
      broken: "(壊れた state.json)",
      tiles: `(タイル表示の ${TILE_PROJECT_COUNT} プロジェクト)`,
      settings: `(${SETTINGS_FILE_NAME} を置いた起動)`,
      fonts: "(文字の大きさを入れた起動)",
    };
    const label = labels[kind] ?? "";
    if (result.ok) {
      console.log(`スモーク成功${label}: ${result.message}`);
    } else {
      console.error(`スモーク失敗${label}: ${result.message}`);
      code = 1;
    }
  }
  return code;
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
