// scripts/smoke.mjs のテスト。本物の Arin.app は使わず、一時フォルダに作った偽の .app で動かす。
// 偽の実行ファイルは node のスクリプト。環境変数で渡された状態フォルダに印を書くかどうかを振る舞いで変える。

import assert from "node:assert/strict";
import { chmod, mkdir, mkdtemp, readFile, readdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { after, before, describe, it } from "node:test";

import {
  FRONTEND_ERROR_PREFIX,
  MARKER_FILE_NAME,
  TEMP_PREFIX,
  TERMINAL_MARKER_FILE_NAME,
  createOutputWatcher,
  parseArgs,
  readBundleExecutable,
  runSmoke,
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

/** 印を書く本体。names は書く印のファイル名。 */
const writeMarkers = (names) => `
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

  it("形の違う --env、スモークが決める環境変数、知らない引数は失敗する", () => {
    assert.throws(() => parseArgs(["--env=ARIN_RENDERER"]), /<名前>=<値>/);
    assert.throws(() => parseArgs(["--env==webgl"]), /<名前>=<値>/);
    assert.throws(() => parseArgs(["--env=ARIN_STATE_DIR=/tmp/x"]), /指定できない/);
    assert.throws(() => parseArgs(["--env=ARIN_SMOKE=1"]), /指定できない/);
    assert.throws(() => parseArgs(["--renderer=webgl"]), /知らない引数/);
  });
});
