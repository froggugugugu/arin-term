// scripts/perf/run.mjs のテスト。重い計測(cargo test、アプリの起動)は動かさず、計測の本体を偽物に差し替えて、
// 結果のファイルを書くことだけを確かめる。`npm test` の `node --test "scripts/*.test.mjs"` が拾うよう、
// scripts/perf/ の下ではなくここに置く(グロブの * は / をまたがない)。

import assert from "node:assert/strict";
import { mkdir, mkdtemp, readFile, readdir, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { after, before, describe, it } from "node:test";

import { PERF_PROJECT_COUNT, measureAppMemory, perfStateFixture, preparePerfState } from "./perf/app-memory.mjs";
import { parsePerfLines, runPerf, stats } from "./perf/run.mjs";
import { STATE_FILE_NAME, describeDir, stateFixture } from "./smoke.mjs";

let root;

before(async () => {
  root = await mkdtemp(path.join(tmpdir(), "arin-perf-test-"));
});

after(async () => {
  await rm(root, { recursive: true, force: true });
});

const ENV = () => ({ commit: "abc", model: "Mac", chip: "M", macos: "26", rustc: "rustc", node: "v24", build: "test" });
const NOW = () => new Date(2026, 10, 15, 4, 5, 6);
const quiet = () => {};

const FAKE = {
  pty_read: () => ({
    status: "ok",
    runs: [1, 2, 3].map((run) => ({ kind: "pty_read", run, first_to_last_ms: 40 + run, mib_per_s: 180, lines_per_s: 2e6, bytes: 7_800_000, reads: 7_600 })),
  }),
  sessions: () => ({
    status: "ok",
    runs: [
      { kind: "session_ack", run: 1, first_to_last_ms: 45, mib_per_s: 160, sends: 117, avg_send_bytes: 66_000, max_send_bytes: 66_400, max_unacked: 68_000, rss_before_kib: 1, rss_after_kib: 2 },
      { kind: "session_stall", run: 1, stalled_unacked: 525_184, stalled_received: 525_184, paused_after_ms: 8, ended_while_stalled: false, resumed_to_end_ms: 50, rss_before_kib: 1, rss_stalled_kib: 1 },
    ],
  }),
  app: () => ({ status: "skipped", runs: [], note: "未計測(`npm run app` が先)" }),
};

describe("runPerf", () => {
  it("日時の入った JSON と Markdown、latest.json と latest.md を書き、終了コードは 0", async () => {
    const outDir = path.join(root, "ok");
    const { exitCode, files } = await runPerf({ measures: FAKE, environment: ENV, outDir, now: NOW, log: quiet });
    assert.equal(exitCode, 0);
    assert.deepEqual((await readdir(outDir)).sort(), [
      "latest.json",
      "latest.md",
      "perf-20261115-040506.json",
      "perf-20261115-040506.md",
    ]);
    assert.equal(files.length, 4);

    const json = JSON.parse(await readFile(path.join(outDir, "latest.json"), "utf8"));
    assert.equal(json.environment.commit, "abc");
    assert.equal(json.measures.pty_read.status, "ok");
    assert.equal(json.measures.app.status, "skipped");
    const first = json.summary.pty_read[0];
    assert.deepEqual([first.median, first.min, first.max, first.n], [42, 41, 43, 3]);

    const md = await readFile(path.join(outDir, "latest.md"), "utf8");
    assert.equal(md, await readFile(path.join(outDir, "perf-20261115-040506.md"), "utf8"));
    assert.match(md, /## 環境/);
    assert.ok(md.includes("| 42.0 | 41.0 | 43.0 | ms | 3 |"), md);
    assert.match(md, /\| app \| 未計測 \| 0 \|/);
    assert.match(md, /## 人間が見る項目/);
  });

  it("計測が失敗・例外のときも結果を書き、終了コードは 1", async () => {
    const outDir = path.join(root, "error");
    const { exitCode, result } = await runPerf({
      measures: {
        ...FAKE,
        pty_read: () => ({ status: "error", runs: [], error: "cargo test が失敗した" }),
        app: () => {
          throw new Error("壊れた");
        },
      },
      environment: ENV,
      outDir,
      now: NOW,
      log: quiet,
    });
    assert.equal(exitCode, 1);
    assert.equal(result.measures.app.status, "error");
    assert.match(result.measures.app.error, /壊れた/);
    const md = await readFile(path.join(outDir, "latest.md"), "utf8");
    assert.match(md, /\| pty_read \| 計測できなかった \|/);
  });

  it("数値がどれだけ悪くても失敗にしない", async () => {
    const { exitCode } = await runPerf({
      measures: { pty_read: () => ({ status: "ok", runs: [{ kind: "pty_read", run: 1, first_to_last_ms: 1e9, mib_per_s: 0.001, bytes: 1, reads: 1 }] }) },
      environment: ENV,
      outDir: path.join(root, "slow"),
      now: NOW,
      log: quiet,
    });
    assert.equal(exitCode, 0);
  });
});

describe("parsePerfLines と stats", () => {
  it("ARIN_PERF の行だけを読み、中央値と最小・最大を出す", () => {
    const text = 'running 1 test\ntest x ... ARIN_PERF {"kind":"a","v":3}\nARIN_PERF {"kind":"a","v":1}\nok\n';
    assert.deepEqual(parsePerfLines(text), [
      { kind: "a", v: 3 },
      { kind: "a", v: 1 },
    ]);
    assert.deepEqual(stats([3, 1, 2]), { median: 2, min: 1, max: 3, n: 3 });
    assert.deepEqual(stats([4, 1]), { median: 2.5, min: 1, max: 4, n: 2 });
    assert.equal(stats([]), null);
  });
});

describe("5 プロジェクト・各 1 タブの状態(T35b)", () => {
  it("一時フォルダの中にプロジェクトのフォルダ 5 つと、bookmark が null の state.json を置く", async () => {
    const work = path.join(root, "state-5");
    await mkdir(work);
    const { stateDir, projectDirs, fixture } = await preparePerfState(work);
    assert.equal(PERF_PROJECT_COUNT, 5);
    assert.equal(stateDir, path.join(work, "state"));
    assert.equal(projectDirs.length, 5);
    for (const dir of projectDirs) {
      assert.ok(dir.startsWith(`${work}${path.sep}`), dir);
      assert.deepEqual(await readdir(dir), [], "プロジェクトのフォルダは空");
    }
    assert.deepEqual(await readdir(stateDir), [STATE_FILE_NAME]);
    assert.equal(await readFile(path.join(stateDir, STATE_FILE_NAME), "utf8"), fixture);

    const state = JSON.parse(fixture);
    assert.equal(state.schemaVersion, 1);
    assert.deepEqual(
      state.projects.map((p) => p.lastKnownPath),
      projectDirs,
    );
    assert.ok(state.projects.every((p) => p.bookmark === null));
    const ids = state.projects.map((p) => p.id);
    assert.equal(new Set(ids).size, 5, "ID が重ならない");
    for (const id of ids) assert.match(id, /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/);
    assert.equal(state.ui.selectedProjectId, ids[0]);
  });

  it("プロジェクトの形はスモークの state.json(アプリが読めると確かめたもの)と同じキーと型", () => {
    const smoke = JSON.parse(stateFixture("project", "/tmp/arin-x"));
    const perf = JSON.parse(perfStateFixture(["/tmp/arin-a", "/tmp/arin-b"]));
    const shape = (o) => Object.fromEntries(Object.entries(o).map(([k, v]) => [k, v === null ? "null" : typeof v]));
    assert.deepEqual(Object.keys(perf).sort(), Object.keys(smoke).sort());
    assert.deepEqual(Object.keys(perf.ui).sort(), Object.keys(smoke.ui).sort());
    for (const p of perf.projects) assert.deepEqual(shape(p), shape(smoke.projects[0]));
    const colors = ["red", "orange", "yellow", "green", "teal", "blue", "purple", "pink"];
    assert.ok(perf.projects.every((p) => colors.includes(p.colorId)));
    assert.throws(() => perfStateFixture([]));
  });

  it(".app が無ければ起動も状態フォルダの読み書きもせず未計測を返す", async () => {
    const realStateDir = path.join(root, "real-state");
    const before = await describeDir(realStateDir);
    const r = await measureAppMemory({ appPath: path.join(root, "no-such", "Arin.app"), realStateDir, log: quiet });
    assert.equal(r.status, "skipped");
    assert.deepEqual(r.runs, []);
    assert.deepEqual(await describeDir(realStateDir), before);
    assert.equal(before, null, "本物の代わりのフォルダを作っていない");
  });
});
