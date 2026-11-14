// scripts/smoke.mjs のテスト。本物の Arin.app は使わず、一時フォルダに作った偽の .app で動かす。
// 偽の実行ファイルは node のスクリプト。環境変数で渡された状態フォルダに印を書くかどうかを振る舞いで変える。

import assert from "node:assert/strict";
import { chmod, mkdir, mkdtemp, readdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { after, before, describe, it } from "node:test";

import { MARKER_FILE_NAME, TEMP_PREFIX, readBundleExecutable, runSmoke } from "./smoke.mjs";

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

const WRITE_MARKER = `
setTimeout(() => {
  if (process.env.ARIN_SMOKE === "1" && process.env.ARIN_STATE_DIR) {
    fs.writeFileSync(path.join(process.env.ARIN_STATE_DIR, "${MARKER_FILE_NAME}"), process.pid + "\\n");
  }
}, 100);
setInterval(() => {}, 1000);
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
      `fs.writeFileSync(path.join(process.env.ARIN_STATE_DIR, "${MARKER_FILE_NAME}"), "1\\n");\nsetInterval(() => {}, 1000);`,
    );

    const result = await runSmoke(options(app));

    assert.equal(result.ok, false);
    assert.match(result.message, /プロセス ID/);
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
