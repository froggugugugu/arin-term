// scripts/smoke.mjs のテスト。本物の Arin.app は使わず、一時フォルダに作った偽の .app で動かす。
// 偽の実行ファイルは node のスクリプト。環境変数で渡された状態フォルダに印を書くかどうかを振る舞いで変える。
// 偽のアプリは、本物と同じように state.json を扱う(読めればブックマークを入れて書き直し、読めなければ退避して空で書く)。

import assert from "node:assert/strict";
import { chmod, mkdir, mkdtemp, readFile, readdir, realpath, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { after, before, describe, it } from "node:test";

import {
  BROKEN_PREFIX,
  BROKEN_STATE,
  FRONTEND_ERROR_PREFIX,
  MARKER_FILE_NAME,
  SMOKE_PROJECT_ID,
  STATE_FILE_NAME,
  TEMP_PREFIX,
  TERMINAL_MARKER_FILE_NAME,
  TILE_PROJECT_COUNT,
  checkStateAfterExit,
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
      `if (process.env.ARIN_RENDERER !== "webgl") process.stderr.write("${FRONTEND_ERROR_PREFIX} ARIN_RENDERER=" + process.env.ARIN_RENDERER + "\\n");\n${WRITE_MARKER}`,
    );

    const result = await runSmoke(
      options(app, { extraEnv: { ARIN_RENDERER: "webgl", ARIN_STATE_DIR: "/should-not-win", ARIN_SMOKE: "0" } }),
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

describe("createOutputWatcher", () => {
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
    assert.deepEqual(parseArgs(["--env=ARIN_RENDERER=webgl", "--env=ARIN_UNICODE=graphemes", "--env=X=a=b", "--env=EMPTY="]), {
      extraEnv: { ARIN_RENDERER: "webgl", ARIN_UNICODE: "graphemes", X: "a=b", EMPTY: "" },
    });
  });

  it("--timeout-ms と --app もこれまでどおり受ける", () => {
    assert.deepEqual(parseArgs(["--timeout-ms=500", "--app=/tmp/x.app"]), { timeoutMs: 500, appPath: "/tmp/x.app" });
  });

  it("--tiles を受け_ほかの指定と並べられる", () => {
    assert.deepEqual(parseArgs(["--tiles"]), { tiles: true });
    assert.deepEqual(parseArgs(["--tiles", "--env=ARIN_RENDERER=webgl"]), { tiles: true, extraEnv: { ARIN_RENDERER: "webgl" } });
    assert.throws(() => parseArgs(["--tiles=1"]), /知らない引数/);
  });

  it("形の違う --env、スモークが決める環境変数、知らない引数は失敗する", () => {
    assert.throws(() => parseArgs(["--env=ARIN_RENDERER"]), /<名前>=<値>/);
    assert.throws(() => parseArgs(["--env==webgl"]), /<名前>=<値>/);
    assert.throws(() => parseArgs(["--env=ARIN_STATE_DIR=/tmp/x"]), /指定できない/);
    assert.throws(() => parseArgs(["--env=ARIN_SMOKE=1"]), /指定できない/);
    assert.throws(() => parseArgs(["--renderer=webgl"]), /知らない引数/);
  });
});
