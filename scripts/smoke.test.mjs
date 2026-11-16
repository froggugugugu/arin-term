// scripts/smoke.mjs のテスト。本物の Arin.app は使わず、一時フォルダに作った偽の .app で動かす。
// 偽の実行ファイルは node のスクリプト。環境変数で渡された状態フォルダに印を書くかどうかを振る舞いで変える。
// 偽のアプリは、本物と同じように state.json を扱う(読めればブックマークを入れて書き直し、読めなければ退避して空で書く)。

import assert from "node:assert/strict";
import { chmod, mkdir, mkdtemp, readFile, readdir, realpath, rm, stat, unlink, utimes, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { after, before, describe, it } from "node:test";

import {
  BROKEN_PREFIX,
  BROKEN_STATE,
  FONT_SIZES_FIXTURE,
  FRONTEND_ERROR_PREFIX,
  MARKER_FILE_NAME,
  SETTINGS_BAD_ROW,
  SETTINGS_COMMENT_MARK,
  SETTINGS_DIAGNOSTIC_PREFIX,
  SETTINGS_FILE_NAME,
  SETTINGS_FIXTURE,
  SETTINGS_GOOD_ROW,
  SMOKE_PROJECT_ID,
  STATE_FILE_NAME,
  TEMP_PREFIX,
  TERMINAL_MARKER_FILE_NAME,
  TILE_PROJECT_COUNT,
  checkSettingsAfterExit,
  checkSettingsDiagnostics,
  checkStateAfterExit,
  fingerprintFile,
  createOutputWatcher,
  parseArgs,
  numberedProjectId,
  prepareStateDir,
  readBundleExecutable,
  runSmoke,
  runSmokeSuite,
  stateFixture,
} from "./smoke.mjs";

let root;

before(async () => {
  root = await mkdtemp(path.join(tmpdir(), "arin-smoke-test-"));
});

after(async () => {
  await rm(root, { recursive: true, force: true });
});

const PLIST = (exe) => `<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
<dict>
  <key>CFBundleName</key>
  <string>Fake</string>
  <key>CFBundleExecutable</key>
  <string>${exe}</string>
</dict>
</plist>
`;

/** 偽の .app を作る。body は実行ファイル(node のスクリプト)の中身。 */
async function fakeApp(name, body) {
  const app = path.join(root, `${name}.app`);
  await mkdir(path.join(app, "Contents", "MacOS"), { recursive: true });
  await writeFile(path.join(app, "Contents", "Info.plist"), PLIST("fake"));
  const exe = path.join(app, "Contents", "MacOS", "fake");
  await writeFile(
    exe,
    `#!${process.execPath}\nconst fs = require("node:fs");\nconst path = require("node:path");\n${body}\n`,
  );
  await chmod(exe, 0o755);
  return app;
}

/** 本物と同じように state.json を扱う(ほかの項目は読んだまま書き戻す)。 */
const HANDLE_STATE = `
(() => {
  const dir = process.env.ARIN_STATE_DIR;
  const file = path.join(dir, "${STATE_FILE_NAME}");
  if (!dir || !fs.existsSync(file)) return;
  let state = null;
  try { state = JSON.parse(fs.readFileSync(file, "utf8")); } catch {}
  if (state === null) {
    fs.linkSync(file, path.join(dir, "${BROKEN_PREFIX}20261115T093000Z"));
    fs.unlinkSync(file);
    state = { schemaVersion: 1, projects: [], ui: { selectedProjectId: null } };
  }
  for (const p of state.projects) { p.bookmark = "ZmFrZQ=="; p.lastKnownPath = fs.realpathSync(p.lastKnownPath); }
  fs.writeFileSync(file, JSON.stringify(state));
})();
`;

/** 印を書く本体。names は書く印のファイル名。 */
const writeMarkers = (names, handleState = true) => `
${handleState ? HANDLE_STATE : ""}
setTimeout(() => {
  if (process.env.ARIN_SMOKE === "1" && process.env.ARIN_STATE_DIR) {
    for (const name of ${JSON.stringify(names)}) {
      fs.writeFileSync(path.join(process.env.ARIN_STATE_DIR, name), process.pid + "\\n");
    }
  }
}, 100);
setInterval(() => {}, 1000);
`;

const WRITE_MARKER = writeMarkers([MARKER_FILE_NAME, TERMINAL_MARKER_FILE_NAME]);

/** 親(偽のアプリ)が終わったら自分も終わる子プロセスを起こす(PTY が閉じて SIGHUP で終わるシェルの代わり)。 */
const SPAWN_POLITE_CHILD = `
const { spawn } = require("node:child_process");
spawn(process.execPath, ["-e", "const p = process.ppid; setInterval(() => { try { process.kill(p, 0); } catch { process.exit(0); } }, 50);"], { stdio: "ignore" });
`;

/**
 * 親(偽のアプリ)が終わったら終わる子プロセスを n 個起こす(タイル表示の 9 枚のシェルの代わり)。
 * node より軽い /bin/sh で、親がいなくなるまで待つ。
 */
const spawnPoliteShells = (n) => `
const { spawn: spawnShell } = require("node:child_process");
for (let i = 0; i < ${n}; i += 1) {
  spawnShell("/bin/sh", ["-c", "while kill -0 $PPID 2>/dev/null; do sleep 0.05; done"], { stdio: "ignore" });
}
`;

/** 親が終わっても残る子プロセスを起こす。起こした子の ID を状態フォルダの外(root)に書く。 */
const spawnStubbornChild = (pidFile) => `
const { spawn } = require("node:child_process");
const c = spawn(process.execPath, ["-e", "process.on('SIGHUP', () => {}); setInterval(() => {}, 1000);"], { stdio: "ignore" });
fs.writeFileSync(${JSON.stringify(pidFile)}, String(c.pid));
`;

function isAlive(pid) {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}

/** 一時領域に残った、スモークが作った状態フォルダの数。 */
async function leftoverStateDirs() {
  return (await readdir(tmpdir())).filter((n) => n.startsWith(TEMP_PREFIX)).length;
}

function options(appPath, extra = {}) {
  return {
    appPath,
    realStateDir: path.join(root, "real-state-does-not-exist"),
    timeoutMs: 5000,
    pollMs: 20,
    killGraceMs: 1000,
    settleMs: 100,
    childExitMs: 1000,
    log: () => {},
    ...extra,
  };
}

describe("readBundleExecutable", () => {
  it("Info.plist の CFBundleExecutable から実行ファイルの場所を返す", async () => {
    const app = await fakeApp("plist", "");
    assert.equal(await readBundleExecutable(app), path.join(app, "Contents", "MacOS", "fake"));
  });
});

describe("runSmoke", () => {
  it(".app が無ければ、npm run app を先に実行するよう伝えて失敗する", async () => {
    const result = await runSmoke(options(path.join(root, "missing.app")));

    assert.equal(result.ok, false);
    assert.match(result.message, /npm run app/);
  });

  it("印が現れたら成功し、プロセスを終了させ、状態フォルダを消す", async () => {
    const app = await fakeApp("ready", WRITE_MARKER);
    const before = await leftoverStateDirs();

    const result = await runSmoke(options(app));

    assert.equal(result.ok, true, result.message);
    assert.ok(result.pid > 0);
    assert.equal(isAlive(result.pid), false, "プロセスが残っている");
    assert.equal(await leftoverStateDirs(), before, "状態フォルダが残っている");
  });

  it("印が上限時間までに現れなければ失敗し、プロセスを終了させる", async () => {
    const app = await fakeApp("hang", "setInterval(() => {}, 1000);");

    const result = await runSmoke(options(app, { timeoutMs: 300 }));

    assert.equal(result.ok, false);
    assert.match(result.message, /現れなかった/);
    assert.equal(isAlive(result.pid), false, "プロセスが残っている");
  });

  it("準備完了の印だけでターミナルの印が現れなければ、どちらが無いかを伝えて失敗する", async () => {
    const app = await fakeApp("ready-only", writeMarkers([MARKER_FILE_NAME]));

    const result = await runSmoke(options(app, { timeoutMs: 600 }));

    assert.equal(result.ok, false);
    assert.match(result.message, /ターミナルの印/);
    assert.doesNotMatch(result.message, /準備完了の印/);
  });

  it("標準エラーに画面の失敗の行があれば、印が揃っていても失敗し、その行を出す", async () => {
    const app = await fakeApp(
      "frontend-error",
      `process.stderr.write("ふつうのログ\\n${FRONTEND_ERROR_PREFIX} CSP 違反: style-src-elem が inline を止めた\\n");\n${WRITE_MARKER}`,
    );

    const result = await runSmoke(options(app));

    assert.equal(result.ok, false);
    assert.match(result.message, /CSP 違反: style-src-elem が inline を止めた/);
    assert.doesNotMatch(result.message.split("---")[0], /ふつうのログ/);
  });

  it("印が揃ったあと、待つあいだに出た画面の失敗の行も拾う", async () => {
    const app = await fakeApp(
      "late-error",
      `${WRITE_MARKER}\nsetTimeout(() => process.stderr.write("${FRONTEND_ERROR_PREFIX} 遅れて出た失敗\\n"), 300);`,
    );

    const result = await runSmoke(options(app, { settleMs: 800 }));

    assert.equal(result.ok, false);
    assert.match(result.message, /遅れて出た失敗/);
  });

  it("印が揃わないうちに画面の失敗が出たら、上限時間まで待たずに失敗する", async () => {
    const app = await fakeApp(
      "early-error",
      `process.stderr.write("${FRONTEND_ERROR_PREFIX} 未処理の例外: boom\\n");\nsetInterval(() => {}, 1000);`,
    );

    const result = await runSmoke(options(app, { timeoutMs: 5000 }));

    assert.equal(result.ok, false);
    assert.match(result.message, /未処理の例外: boom/);
    assert.ok(result.seconds < 4, "上限時間まで待ってしまった");
  });

  it("アプリが起動した子プロセスがアプリの終了で終われば成功し、その ID を返す", async () => {
    const app = await fakeApp("polite-child", `${SPAWN_POLITE_CHILD}\n${WRITE_MARKER}`);

    const result = await runSmoke(options(app));

    assert.equal(result.ok, true, result.message);
    assert.equal(result.shells.length, 1);
    assert.equal(isAlive(result.shells[0]), false);
  });

  it("アプリの終了後も子プロセスが残れば失敗にし、取り残さず終了させる", async () => {
    const pidFile = path.join(root, "stubborn-child.pid");
    const app = await fakeApp("stubborn-child", `${spawnStubbornChild(pidFile)}\n${WRITE_MARKER}`);

    const result = await runSmoke(options(app));

    assert.equal(result.ok, false);
    assert.match(result.message, /子プロセスが残っていた/);
    const childPid = Number(await readFile(pidFile, "utf8"));
    await new Promise((r) => setTimeout(r, 200));
    assert.equal(isAlive(childPid), false, "残った子プロセスを終了させていない");
  });

  it("印を書かずに終了したら、待ち切らずに失敗する", async () => {
    const app = await fakeApp("exit", "process.exit(3);");

    const result = await runSmoke(options(app, { timeoutMs: 5000 }));

    assert.equal(result.ok, false);
    assert.match(result.message, /終了した/);
    assert.ok(result.seconds < 4, "上限時間まで待ってしまった");
  });

  it("SIGTERM で終わらなければ SIGKILL で終了させる", async () => {
    const app = await fakeApp("stubborn", `process.on("SIGTERM", () => {});\n${WRITE_MARKER}`);

    const result = await runSmoke(options(app, { killGraceMs: 200 }));

    assert.equal(result.ok, true, result.message);
    assert.equal(isAlive(result.pid), false, "プロセスが残っている");
  });

  it("印の中身が起こしたプロセスの ID と違えば失敗する", async () => {
    const app = await fakeApp(
      "wrong-pid",
      `for (const n of ["${MARKER_FILE_NAME}", "${TERMINAL_MARKER_FILE_NAME}"]) fs.writeFileSync(path.join(process.env.ARIN_STATE_DIR, n), "1\\n");\nsetInterval(() => {}, 1000);`,
    );

    const result = await runSmoke(options(app));

    assert.equal(result.ok, false);
    assert.match(result.message, /プロセス ID/);
  });

  it("追加の環境変数をアプリに渡す(状態フォルダとスモークの印の指定は上書きさせない)", async () => {
    const app = await fakeApp(
      "extra-env",
      `if (process.env.SMOKE_EXTRA !== "yes") process.stderr.write("${FRONTEND_ERROR_PREFIX} SMOKE_EXTRA=" + process.env.SMOKE_EXTRA + "\\n");\n${WRITE_MARKER}`,
    );

    const result = await runSmoke(
      options(app, { extraEnv: { SMOKE_EXTRA: "yes", ARIN_STATE_DIR: "/should-not-win", ARIN_SMOKE: "0" } }),
    );

    assert.equal(result.ok, true, result.message);
  });

  it("本物の状態フォルダが作られたら失敗する", async () => {
    const real = path.join(root, "real-created");
    const app = await fakeApp(
      "touch-real",
      `fs.mkdirSync(${JSON.stringify(real)});\n${WRITE_MARKER}`,
    );

    const result = await runSmoke(options(app, { realStateDir: real }));

    assert.equal(result.ok, false);
    assert.match(result.message, /本物の状態フォルダ/);
  });
});

describe("状態ファイルの用意と終了後の確認", () => {
  async function prepared(name, kind) {
    const dir = path.join(root, name);
    await mkdir(dir);
    const p = await prepareStateDir(dir, kind);
    return { dir, ...p, projectDir: p.projectDirs[0] };
  }

  /** アプリが書いたように、置いた state.json の全件にブックマークを入れ lastKnownPath を正規化して書く。 */
  async function writeBookmarked(p, edit = (state) => state) {
    const state = JSON.parse(p.fixture);
    for (const [i, project] of state.projects.entries()) {
      project.bookmark = "ZmFrZQ==";
      project.lastKnownPath = await realpath(p.projectDirs[i]);
    }
    await writeFile(path.join(p.stateDir, STATE_FILE_NAME), JSON.stringify(edit(state)));
  }

  it("プロジェクト 1 件の state.json は bookmark が null で lastKnownPath がプロジェクトのフォルダ", async () => {
    const { stateDir, projectDir, fixture } = await prepared("fixture-project", "project");

    const text = await readFile(path.join(stateDir, STATE_FILE_NAME), "utf8");
    assert.equal(text, fixture);
    const state = JSON.parse(text);
    assert.equal(state.schemaVersion, 1);
    assert.deepEqual(state.projects, [
      { id: SMOKE_PROJECT_ID, bookmark: null, lastKnownPath: projectDir, customName: null, colorId: "green" },
    ]);
    assert.equal(state.ui.selectedProjectId, SMOKE_PROJECT_ID);
    assert.equal("displayMode" in state.ui, false);
    assert.match(SMOKE_PROJECT_ID, /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/);
    assert.deepEqual(await readdir(projectDir), []);
  });

  it("タイルの指定で_9_プロジェクトとタイル表示の状態ファイルを置く", async () => {
    const { dir, stateDir, projectDirs, fixture } = await prepared("fixture-tiles", "tiles");

    assert.equal(TILE_PROJECT_COUNT, 9);
    assert.deepEqual(projectDirs, Array.from({ length: 9 }, (_, i) => path.join(dir, "projects", `p${i + 1}`)));
    const text = await readFile(path.join(stateDir, STATE_FILE_NAME), "utf8");
    assert.equal(text, fixture);
    const state = JSON.parse(text);
    assert.equal(state.schemaVersion, 1);
    assert.equal(state.ui.displayMode, "tiles");
    assert.deepEqual(
      state.projects.map((p) => [p.id, p.bookmark, p.lastKnownPath]),
      projectDirs.map((d, i) => [numberedProjectId(i + 1), null, d]),
    );
    assert.equal(state.ui.selectedProjectId, numberedProjectId(1));
    for (const p of state.projects) {
      assert.match(p.id, /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/);
    }
    // 色は 8 色のパレットから順に(9 件目は先頭に戻る)。
    assert.equal(new Set(state.projects.slice(0, 8).map((p) => p.colorId)).size, 8);
    for (const d of projectDirs) assert.deepEqual(await readdir(d), []);
  });

  it("壊れた state.json は JSON として読めない", async () => {
    const { stateDir } = await prepared("fixture-broken", "broken");

    const text = await readFile(path.join(stateDir, STATE_FILE_NAME), "utf8");
    assert.equal(text, BROKEN_STATE);
    assert.throws(() => JSON.parse(text));
  });

  it("知らない種類は失敗する", () => {
    assert.throws(() => stateFixture("other", "/x"), /知らない/);
  });

  it("ブックマークが入り lastKnownPath が正規化したパスなら問題なし", async () => {
    const p = await prepared("check-ok", "project");
    const state = JSON.parse(p.fixture);
    state.projects[0].bookmark = "ZmFrZQ==";
    state.projects[0].lastKnownPath = await realpath(p.projectDir);
    await writeFile(path.join(p.stateDir, STATE_FILE_NAME), JSON.stringify(state));

    assert.equal(await checkStateAfterExit({ ...p, kind: "project" }), null);
  });

  it("ブックマークが null のままなら失敗", async () => {
    const p = await prepared("check-no-bookmark", "project");

    assert.match(await checkStateAfterExit({ ...p, kind: "project" }), /ブックマーク/);
  });

  it("プロジェクトのフォルダに何かできたら失敗", async () => {
    const p = await prepared("check-project-touched", "project");
    await writeBookmarked(p);
    await writeFile(path.join(p.projectDir, ".arin"), "x");

    assert.match(await checkStateAfterExit({ ...p, kind: "project" }), /プロジェクトのフォルダ/);
  });

  it("タイル表示は_9_件にブックマークが入り表示モードがタイルのままなら問題なし", async () => {
    const p = await prepared("check-tiles-ok", "tiles");
    await writeBookmarked(p);

    assert.equal(await checkStateAfterExit({ ...p, kind: "tiles" }), null);
  });

  it("タイル表示は_1_件でもブックマークが無い_表示モードが変わった_件数が違う_フォルダに何かできたら失敗", async () => {
    const p = await prepared("check-tiles-bad", "tiles");
    await writeBookmarked(p, (state) => {
      state.projects[8].bookmark = null;
      return state;
    });
    assert.match(await checkStateAfterExit({ ...p, kind: "tiles" }), /9 件目にブックマーク/);

    await writeBookmarked(p, (state) => ({ ...state, ui: { ...state.ui, displayMode: "tabs" } }));
    assert.match(await checkStateAfterExit({ ...p, kind: "tiles" }), /表示モードがタイルでない/);

    await writeBookmarked(p, (state) => ({ ...state, projects: state.projects.slice(0, 8) }));
    assert.match(await checkStateAfterExit({ ...p, kind: "tiles" }), /置いたものと違う/);

    await writeBookmarked(p);
    await writeFile(path.join(p.projectDirs[4], ".arin"), "x");
    assert.match(await checkStateAfterExit({ ...p, kind: "tiles" }), /p5 に \.arin ができた/);
  });

  it("壊れたものは退避のファイルが無ければ失敗_元の中身で退避され空で書かれていれば問題なし", async () => {
    const p = await prepared("check-broken", "broken");
    const statePath = path.join(p.stateDir, STATE_FILE_NAME);
    assert.match(await checkStateAfterExit({ ...p, kind: "broken" }), /版 1 の形でない/);

    await writeFile(statePath, JSON.stringify({ schemaVersion: 1, projects: [] }));
    assert.match(await checkStateAfterExit({ ...p, kind: "broken" }), /退避のファイル/);

    await writeFile(path.join(p.stateDir, `${BROKEN_PREFIX}20261115T093000Z`), p.fixture);
    assert.equal(await checkStateAfterExit({ ...p, kind: "broken" }), null);
  });
});

describe("runSmoke と状態ファイル", () => {
  it("アプリが state.json にブックマークを書かなければ失敗する", async () => {
    const app = await fakeApp("no-state", writeMarkers([MARKER_FILE_NAME, TERMINAL_MARKER_FILE_NAME], false));

    const result = await runSmoke(options(app));

    assert.equal(result.ok, false);
    assert.match(result.message, /ブックマーク/);
  });

  it("9_枚の印が揃うまで待つ_シェルが_9_個揃えば成功し_終了後に_9_個とも残らない", async () => {
    const app = await fakeApp("tiles-nine", `${spawnPoliteShells(TILE_PROJECT_COUNT)}\n${WRITE_MARKER}`);

    const result = await runSmoke(options(app, { stateKind: "tiles" }));

    assert.equal(result.ok, true, result.message);
    assert.match(result.message, /シェル 9 個/);
    assert.equal(result.shells.length, TILE_PROJECT_COUNT);
    for (const pid of result.shells) assert.equal(isAlive(pid), false, `子プロセス ${pid} が残っている`);
  });

  it("タイル表示でシェルが_9_個揃わなければ印があっても上限時間で失敗し_数えた数を伝える", async () => {
    const app = await fakeApp("tiles-eight", `${spawnPoliteShells(TILE_PROJECT_COUNT - 1)}\n${WRITE_MARKER}`);

    const result = await runSmoke(options(app, { stateKind: "tiles", timeoutMs: 1500 }));

    assert.equal(result.ok, false);
    assert.match(result.message, /シェル 9 個\(数えたのは 8 個\)/);
    assert.doesNotMatch(result.message, /準備完了の印|ターミナルの印/);
  });

  it("タイル表示でアプリが表示モードをタブに書き換えたら失敗する", async () => {
    const app = await fakeApp(
      "tiles-mode-lost",
      `${spawnPoliteShells(TILE_PROJECT_COUNT)}\n${WRITE_MARKER}\nsetTimeout(() => {
  const file = path.join(process.env.ARIN_STATE_DIR, "${STATE_FILE_NAME}");
  const state = JSON.parse(fs.readFileSync(file, "utf8"));
  state.ui.displayMode = "tabs";
  fs.writeFileSync(file, JSON.stringify(state));
}, 50);`,
    );

    const result = await runSmoke(options(app, { stateKind: "tiles" }));

    assert.equal(result.ok, false);
    assert.match(result.message, /表示モードがタイルでない/);
  });

  it("壊れた state.json の起動は準備完了の印だけで判定し_退避と空の書き直しを確かめる", async () => {
    const app = await fakeApp("broken-state", writeMarkers([MARKER_FILE_NAME]));

    const result = await runSmoke(options(app, { stateKind: "broken" }));

    assert.equal(result.ok, true, result.message);
  });

  it("一揃いは通常と壊れたものの 2 回_タイルの指定では_9_プロジェクトのタイル表示で_1_回だけ起動する", async () => {
    const app = await fakeApp("suite", WRITE_MARKER);

    const normal = await runSmokeSuite(options(app));
    assert.deepEqual(
      normal.map((r) => [r.kind, r.result.ok]),
      [
        ["project", true],
        ["broken", true],
      ],
    );
    const nine = await fakeApp("suite-tiles", `${spawnPoliteShells(TILE_PROJECT_COUNT)}\n${WRITE_MARKER}`);
    const tiles = await runSmokeSuite(options(nine, { tiles: true }));
    assert.deepEqual(
      tiles.map((r) => [r.kind, r.result.ok, r.result.shells.length]),
      [["tiles", true, TILE_PROJECT_COUNT]],
    );
  });
});

/** 本物のアプリが問題のある行(tree.rename = F2)に出す設定の行(src-tauri/src/settings.rs・arin-core の overrides.rs の形)。 */
const BAD_ROW_LINE = `${SETTINGS_DIAGNOSTIC_PREFIX} ショートカットの設定: ${SETTINGS_BAD_ROW.id} = ${SETTINGS_BAD_ROW.key}(キーを読めません: 修飾キーでもキーでもない部分があります)`;

/** 標準エラーに行を書く偽のアプリの本体の一部。 */
const writeStderr = (line) => `process.stderr.write(${JSON.stringify(`${line}\n`)});`;

describe("設定ファイルを置いた起動(--settings)", () => {
  async function preparedSettings(name) {
    const dir = path.join(root, name);
    await mkdir(dir);
    return { dir, ...(await prepareStateDir(dir, "settings")) };
  }

  it("設定の指定で一時の状態フォルダに_settings_toml_を置く", async () => {
    const p = await preparedSettings("fixture-settings");
    const file = path.join(p.stateDir, SETTINGS_FILE_NAME);

    // 設定ファイルは正しい変更の行と問題のある行を 1 つずつ [shortcuts] の下に持ち、0600 で置かれ、指紋を返す
    assert.equal(await readFile(file, "utf8"), SETTINGS_FIXTURE);
    assert.equal((await stat(file)).mode & 0o777, 0o600);
    assert.deepEqual(p.settings, await fingerprintFile(file));
    assert.match(p.settings.sha256, /^[0-9a-f]{64}$/);
    const rows = SETTINGS_FIXTURE.split("\n").filter((l) => l !== "" && !l.startsWith("#"));
    assert.deepEqual(rows, [
      "[shortcuts]",
      `"${SETTINGS_GOOD_ROW.id}" = "${SETTINGS_GOOD_ROW.key}"`,
      `"${SETTINGS_BAD_ROW.id}" = "${SETTINGS_BAD_ROW.key}"`,
    ]);
    assert.ok(SETTINGS_FIXTURE.includes(SETTINGS_COMMENT_MARK));
    // state.json はプロジェクト 1 件の起動と同じ
    const state = JSON.parse(await readFile(path.join(p.stateDir, STATE_FILE_NAME), "utf8"));
    assert.deepEqual(state.projects.map((x) => x.id), [SMOKE_PROJECT_ID]);
    assert.deepEqual(await readdir(p.projectDirs[0]), []);

    // ほかの種類では設定ファイルを置かない
    const other = path.join(root, "fixture-no-settings");
    await mkdir(other);
    const plain = await prepareStateDir(other, "project");
    assert.equal(plain.settings, null);
    assert.deepEqual((await readdir(plain.stateDir)).sort(), [STATE_FILE_NAME]);
  });

  it("終わったあと設定ファイルの中身と更新日時が同じか確かめる", async () => {
    const p = await preparedSettings("check-settings");
    const file = path.join(p.stateDir, SETTINGS_FILE_NAME);
    const check = () => checkSettingsAfterExit({ stateDir: p.stateDir, expected: p.settings });
    assert.equal(await check(), null);

    // 中身が変わった(大きさが同じでも)
    await writeFile(file, SETTINGS_FIXTURE.replace("F2", "F3"));
    assert.match(await check(), /中身が変わった/);

    // 中身は同じで更新日時だけが変わった(書き直した)
    await writeFile(file, SETTINGS_FIXTURE);
    await utimes(file, new Date(2026, 0, 1), new Date(2026, 0, 1));
    assert.match(await check(), /更新日時が変わった/);

    // settings.toml の名前を含む別のファイル(退避など)ができた
    const backup = path.join(p.stateDir, `${SETTINGS_FILE_NAME}.broken-20261116T000000Z`);
    await writeFile(backup, SETTINGS_FIXTURE);
    assert.match(await check(), /別のファイルができた: settings\.toml\.broken-20261116T000000Z/);
    await unlink(backup);

    // 消えた
    await unlink(file);
    assert.match(await check(), /読めなかった/);
  });

  it("設定の診断の行に設定の行の全文が無いか確かめる", () => {
    const opts = { expectBadRow: true, forbiddenPaths: ["/private/var/folders/xx/arin-smoke-abc"] };
    assert.equal(checkSettingsDiagnostics([BAD_ROW_LINE], opts), null);

    // 設定ファイルの行の全文・コメント・絶対パス・正しい行のことが出たら失敗
    const prefix = `${SETTINGS_DIAGNOSTIC_PREFIX} ショートカットの設定: `;
    assert.match(
      checkSettingsDiagnostics([`${prefix}"${SETTINGS_BAD_ROW.id}" = "${SETTINGS_BAD_ROW.key}"`], opts),
      /行の全文/,
    );
    assert.match(checkSettingsDiagnostics([`${BAD_ROW_LINE} ${SETTINGS_COMMENT_MARK}`], opts), /コメント/);
    assert.match(
      checkSettingsDiagnostics([`${prefix}/private/var/folders/xx/arin-smoke-abc/state/settings.toml`], opts),
      /絶対パス/,
    );
    assert.match(
      checkSettingsDiagnostics([BAD_ROW_LINE, `${prefix}${SETTINGS_GOOD_ROW.id} = ${SETTINGS_GOOD_ROW.key}(知らないコマンドです)`], opts),
      /正しい行/,
    );
    // 形が違う(理由が無い・ID が違う・キーに無い文字が足された)
    assert.match(checkSettingsDiagnostics([`${prefix}${SETTINGS_BAD_ROW.id} = ${SETTINGS_BAD_ROW.key}`], opts), /形でない/);
    assert.match(checkSettingsDiagnostics([`${prefix}tree.open = F2(理由)`], opts), /形でない/);
    assert.match(checkSettingsDiagnostics([`${prefix}${SETTINGS_BAD_ROW.id} = ${SETTINGS_BAD_ROW.key}(/etc/passwd)`], opts), /形でない/);
    // 1 行も出なければ失敗
    assert.match(checkSettingsDiagnostics([], opts), /出なかった/);

    // 設定ファイルを置かない起動では、1 行も出ないのが正しい(AC-K03)
    assert.equal(checkSettingsDiagnostics([], { expectBadRow: false }), null);
    assert.match(checkSettingsDiagnostics([BAD_ROW_LINE], { expectBadRow: false }), /設定ファイルが無いのに/);
  });

  it("設定の行を拾い_問題のある行だけが出て設定ファイルが変わらなければ成功する", async () => {
    const app = await fakeApp("settings-ok", `${writeStderr("ふつうのログ")}\n${writeStderr(BAD_ROW_LINE)}\n${WRITE_MARKER}`);
    const lines = [];

    const result = await runSmoke(options(app, { stateKind: "settings", log: (l) => lines.push(l) }));

    assert.equal(result.ok, true, result.message);
    assert.ok(lines.some((l) => l.includes(BAD_ROW_LINE)), "設定の行をログに出していない");
  });

  it("設定の行が出ない_全文を出す_設定ファイルを書き換えるアプリは失敗する", async () => {
    const silent = await fakeApp("settings-silent", WRITE_MARKER);
    assert.match((await runSmoke(options(silent, { stateKind: "settings" }))).message, /出なかった/);

    const leaky = await fakeApp(
      "settings-leaky",
      `${writeStderr(`${SETTINGS_DIAGNOSTIC_PREFIX} ショートカットの設定: "${SETTINGS_BAD_ROW.id}" = "${SETTINGS_BAD_ROW.key}"`)}\n${WRITE_MARKER}`,
    );
    assert.match((await runSmoke(options(leaky, { stateKind: "settings" }))).message, /行の全文/);

    const rewriting = await fakeApp(
      "settings-rewrite",
      `${writeStderr(BAD_ROW_LINE)}\nfs.writeFileSync(path.join(process.env.ARIN_STATE_DIR, "${SETTINGS_FILE_NAME}"), "[shortcuts]\\n");\n${WRITE_MARKER}`,
    );
    assert.match((await runSmoke(options(rewriting, { stateKind: "settings" }))).message, /中身が変わった/);

    const renaming = await fakeApp(
      "settings-rename",
      `${writeStderr(BAD_ROW_LINE)}\n{ const d = process.env.ARIN_STATE_DIR; fs.copyFileSync(path.join(d, "${SETTINGS_FILE_NAME}"), path.join(d, "${SETTINGS_FILE_NAME}.bak")); }\n${WRITE_MARKER}`,
    );
    assert.match((await runSmoke(options(renaming, { stateKind: "settings" }))).message, /別のファイルができた/);
  });

  it("設定ファイルを置かない起動で設定の行が出たら失敗する", async () => {
    const app = await fakeApp("settings-unexpected", `${writeStderr(BAD_ROW_LINE)}\n${WRITE_MARKER}`);

    const result = await runSmoke(options(app));

    assert.equal(result.ok, false);
    assert.match(result.message, /設定ファイルが無いのに/);
  });

  it("一揃いは設定の指定で_settings_toml_を置いて_1_回だけ起動する", async () => {
    const app = await fakeApp("suite-settings", `${writeStderr(BAD_ROW_LINE)}\n${WRITE_MARKER}`);

    const results = await runSmokeSuite(options(app, { settings: true }));

    assert.deepEqual(
      results.map((r) => [r.kind, r.result.ok]),
      [["settings", true]],
    );
    await assert.rejects(runSmokeSuite(options(app, { settings: true, tiles: true })), /同時に指定できない/);
  });
});

describe("文字の大きさを入れた起動(--fonts)", () => {
  async function preparedFonts(name) {
    const dir = path.join(root, name);
    await mkdir(dir);
    return { dir, ...(await prepareStateDir(dir, "fonts")) };
  }

  /** アプリが書いたように、ブックマークを入れ lastKnownPath を正規化して書く(edit で ui などを替えられる)。 */
  async function writeAsApp(p, edit = (state) => state) {
    const state = JSON.parse(p.fixture);
    state.projects[0].bookmark = "ZmFrZQ==";
    state.projects[0].lastKnownPath = await realpath(p.projectDirs[0]);
    await writeFile(path.join(p.stateDir, STATE_FILE_NAME), JSON.stringify(edit(state)));
  }

  it("文字の大きさの指定で一時の状態フォルダに_state_json_を置く", async () => {
    const p = await preparedFonts("fixture-fonts");

    // 既定(15・13)でなく範囲(9〜28・11〜18)の中の値
    assert.deepEqual({ ...FONT_SIZES_FIXTURE }, { terminalFontSize: 20, interfaceFontSize: 15 });
    const text = await readFile(path.join(p.stateDir, STATE_FILE_NAME), "utf8");
    assert.equal(text, p.fixture);
    const state = JSON.parse(text);
    assert.equal(state.schemaVersion, 1);
    // プロジェクト 1 件の起動と同じ state.json の ui に 2 つの値が足される
    assert.deepEqual(state.projects, JSON.parse(stateFixture("project", p.projectDirs[0])).projects);
    assert.deepEqual(state.ui, { selectedProjectId: SMOKE_PROJECT_ID, terminalFontSize: 20, interfaceFontSize: 15 });
    // 一時の状態フォルダは mkdtemp の下に作り、設定ファイルは置かない
    assert.equal(p.stateDir, path.join(p.dir, "state"));
    assert.equal(p.settings, null);
    assert.deepEqual((await readdir(p.stateDir)).sort(), [STATE_FILE_NAME]);
    assert.deepEqual(await readdir(p.projectDirs[0]), []);
    // ほかの種類の ui には文字の大きさを入れない(既定のあいだは書かない形)
    assert.equal("terminalFontSize" in JSON.parse(stateFixture("project", "/x")).ui, false);
  });

  it("終わったあと文字の大きさが残り退避が無いか確かめる", async () => {
    const p = await preparedFonts("check-fonts");
    const check = () => checkStateAfterExit({ ...p, kind: "fonts" });

    await writeAsApp(p);
    assert.equal(await check(), null);

    // 既定に戻して書かなかった(キーが消えた)・違う値・数でない値になったら失敗
    await writeAsApp(p, (state) => {
      delete state.ui.terminalFontSize;
      return state;
    });
    assert.match(await check(), /ui\.terminalFontSize が置いた 20 でない/);
    await writeAsApp(p, (state) => ({ ...state, ui: { ...state.ui, interfaceFontSize: 18 } }));
    assert.match(await check(), /ui\.interfaceFontSize が置いた 15 でない\(18\)/);
    await writeAsApp(p, (state) => ({ ...state, ui: { ...state.ui, terminalFontSize: "20" } }));
    assert.match(await check(), /ui\.terminalFontSize/);

    // 版が変わったら失敗
    await writeAsApp(p, (state) => ({ ...state, schemaVersion: 2 }));
    assert.match(await check(), /版 1 の形でない/);

    // 退避のファイルができたら失敗
    await writeAsApp(p);
    const backup = path.join(p.stateDir, `${BROKEN_PREFIX}20261116T000000Z`);
    await writeFile(backup, p.fixture);
    assert.match(await check(), /退避した/);
    await unlink(backup);

    // settings.toml ができたら失敗
    const settings = path.join(p.stateDir, SETTINGS_FILE_NAME);
    await writeFile(settings, "");
    assert.match(await check(), /settings\.toml ができた/);
    await unlink(settings);
    assert.equal(await check(), null);
  });

  it("文字の大きさを書き戻すアプリは成功し_落とすアプリと設定ファイルを作るアプリは失敗する", async () => {
    const ok = await fakeApp("fonts-ok", WRITE_MARKER);
    const lines = [];
    const result = await runSmoke(options(ok, { stateKind: "fonts", log: (l) => lines.push(l) }));
    assert.equal(result.ok, true, result.message);
    assert.ok(lines.some((l) => l.includes("ui.terminalFontSize・ui.interfaceFontSize は 20・15 のまま")), "結果をログに出していない");

    // 既定でない値を書き戻さない(落とす)
    const dropping = await fakeApp(
      "fonts-drop",
      `${WRITE_MARKER}\nsetTimeout(() => {
  const file = path.join(process.env.ARIN_STATE_DIR, "${STATE_FILE_NAME}");
  const state = JSON.parse(fs.readFileSync(file, "utf8"));
  delete state.ui.terminalFontSize;
  delete state.ui.interfaceFontSize;
  fs.writeFileSync(file, JSON.stringify(state));
}, 50);`,
    );
    assert.match((await runSmoke(options(dropping, { stateKind: "fonts" }))).message, /ui\.terminalFontSize が置いた 20 でない/);

    // 設定ファイルを作る
    const writing = await fakeApp(
      "fonts-settings",
      `fs.writeFileSync(path.join(process.env.ARIN_STATE_DIR, "${SETTINGS_FILE_NAME}"), "");\n${WRITE_MARKER}`,
    );
    assert.match((await runSmoke(options(writing, { stateKind: "fonts" }))).message, /settings\.toml ができた/);
  });

  it("一揃いは文字の大きさの指定で_1_回だけ起動し_ほかの_1_回だけの指定とは並べられない", async () => {
    const app = await fakeApp("suite-fonts", WRITE_MARKER);

    const results = await runSmokeSuite(options(app, { fonts: true }));

    assert.deepEqual(
      results.map((r) => [r.kind, r.result.ok]),
      [["fonts", true]],
    );
    await assert.rejects(runSmokeSuite(options(app, { fonts: true, tiles: true })), /同時に指定できない/);
    await assert.rejects(runSmokeSuite(options(app, { fonts: true, settings: true })), /同時に指定できない/);
  });
});

describe("createOutputWatcher", () => {
  it("設定の行は画面の失敗とは別に集める", () => {
    const w = createOutputWatcher();

    w.feed("stderr", Buffer.from(`${SETTINGS_DIAGNOSTIC_PREFIX} a\n${FRONTEND_ERROR_PREFIX} b\n  ${SETTINGS_DIAGNOSTIC_PREFIX} 字下げ\n`));
    w.end();

    assert.deepEqual(w.settingsLines, [`${SETTINGS_DIAGNOSTIC_PREFIX} a`]);
    assert.deepEqual(w.frontendErrors, [`${FRONTEND_ERROR_PREFIX} b`]);
  });

  it("塊の切れ目をまたいだ行も、改行で終わらない最後の行も拾い、頭が一致する行だけを集める", () => {
    const w = createOutputWatcher();

    w.feed("stderr", Buffer.from(`ok\n${FRONTEND_ERROR_PREFIX} a`));
    w.feed("stdout", Buffer.from(`${FRONTEND_ERROR_PREFIX} out\n`));
    w.feed("stderr", Buffer.from(`bc\r\n  ${FRONTEND_ERROR_PREFIX} 字下げは数えない\n${FRONTEND_ERROR_PREFIX} last`));
    w.end();

    assert.deepEqual(w.frontendErrors, [
      `${FRONTEND_ERROR_PREFIX} out`,
      `${FRONTEND_ERROR_PREFIX} abc`,
      `${FRONTEND_ERROR_PREFIX} last`,
    ]);
  });
});

describe("parseArgs", () => {
  it("--env を何度でも受け、名前と値に分ける(値の中の = と空の値も受ける)", () => {
    assert.deepEqual(parseArgs(["--env=SMOKE_A=1", "--env=SMOKE_B=two", "--env=X=a=b", "--env=EMPTY="]), {
      extraEnv: { SMOKE_A: "1", SMOKE_B: "two", X: "a=b", EMPTY: "" },
    });
  });

  it("--timeout-ms と --app もこれまでどおり受ける", () => {
    assert.deepEqual(parseArgs(["--timeout-ms=500", "--app=/tmp/x.app"]), { timeoutMs: 500, appPath: "/tmp/x.app" });
  });

  it("--tiles を受け_ほかの指定と並べられる", () => {
    assert.deepEqual(parseArgs(["--tiles"]), { tiles: true });
    assert.deepEqual(parseArgs(["--tiles", "--env=SMOKE_A=1"]), { tiles: true, extraEnv: { SMOKE_A: "1" } });
    assert.throws(() => parseArgs(["--tiles=1"]), /知らない引数/);
  });

  it("--settings を受け_ほかの指定と並べられるが_--tiles とは並べられない", () => {
    assert.deepEqual(parseArgs(["--settings"]), { settings: true });
    assert.deepEqual(parseArgs(["--settings", "--timeout-ms=500"]), { settings: true, timeoutMs: 500 });
    assert.throws(() => parseArgs(["--settings", "--tiles"]), /同時に指定できない/);
    assert.throws(() => parseArgs(["--settings=1"]), /知らない引数/);
  });

  it("--fonts を受け_ほかの指定と並べられるが_--tiles_--settings とは並べられない", () => {
    assert.deepEqual(parseArgs(["--fonts"]), { fonts: true });
    assert.deepEqual(parseArgs(["--fonts", "--timeout-ms=500"]), { fonts: true, timeoutMs: 500 });
    assert.throws(() => parseArgs(["--fonts", "--tiles"]), /同時に指定できない/);
    assert.throws(() => parseArgs(["--settings", "--fonts"]), /同時に指定できない/);
    assert.throws(() => parseArgs(["--fonts=1"]), /知らない引数/);
  });

  it("形の違う --env、スモークが決める環境変数、知らない引数は失敗する", () => {
    assert.throws(() => parseArgs(["--env=SMOKE_A"]), /<名前>=<値>/);
    assert.throws(() => parseArgs(["--env==1"]), /<名前>=<値>/);
    assert.throws(() => parseArgs(["--env=ARIN_STATE_DIR=/tmp/x"]), /指定できない/);
    assert.throws(() => parseArgs(["--env=ARIN_SMOKE=1"]), /指定できない/);
    assert.throws(() => parseArgs(["--renderer=dom"]), /知らない引数/);
  });
});
