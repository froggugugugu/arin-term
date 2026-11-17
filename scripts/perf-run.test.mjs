// scripts/perf/run.mjs のテスト。重い計測(cargo test、アプリの起動)は動かさず、計測の本体を偽物に差し替えて、
// 結果のファイルを書くことだけを確かめる。`npm test` の `node --test "scripts/*.test.mjs"` が拾うよう、
// scripts/perf/ の下ではなくここに置く(グロブの * は / をまたがない)。

import assert from "node:assert/strict";
import { chmod, mkdir, mkdtemp, readFile, readdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { after, before, describe, it } from "node:test";

import {
  APP_SCENARIOS,
  FLOOD_BYTES,
  FLOOD_DONE_PREFIX,
  PERF_PROJECT_COUNT,
  TILE_PROJECT_COUNT,
  floodZshrc,
  measureAppMemory,
  perfStateFixture,
  prepareFloodHome,
  preparePerfState,
  readFloodDone,
} from "./perf/app-memory.mjs";
import {
  DEFAULT_TREE_FILES,
  PERF_E2E_ENV,
  TILES_E2E_SPEC,
  TREE_FILES_ENV,
  defaultMeasures,
  parseArgs,
  parsePerfLines,
  runPerf,
  runTilesE2e,
  stats,
} from "./perf/run.mjs";
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
  tree: () => ({
    status: "ok",
    runs: [
      ...[1, 2, 3].map((run) => ({ kind: "tree_list", fixture: "flat", run, fixture_files: 100_000, create_s: 4, raw_entries: 100_003, listed: 100_000, read_ms: 40, filter_ms: 0.1, sort_ms: 100 + run, total_ms: 140 + run })),
      { kind: "tree_list", fixture: "nested_root", run: 1, fixture_files: 100_000, create_s: 3, raw_entries: 1003, listed: 1000, read_ms: 0.4, filter_ms: 0, sort_ms: 0.5, total_ms: 0.9 },
      { kind: "tree_list", fixture: "nested_child", run: 1, fixture_files: 100_000, create_s: 3, raw_entries: 100, listed: 100, read_ms: 0.07, filter_ms: 0, sort_ms: 0.05, total_ms: 0.12 },
    ],
  }),
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

describe("ツリーの読み込みの計測(T44a、NFR-005)", () => {
  it("見本ごとに分けて集計し、Markdown に 4. の節を書く", async () => {
    const outDir = path.join(root, "tree");
    const { exitCode, result } = await runPerf({ measures: FAKE, environment: ENV, outDir, now: NOW, log: quiet });
    assert.equal(exitCode, 0);
    const [flat, , , , listed, nestedRoot, , nestedChild] = result.summary.tree_list;
    assert.deepEqual([flat.median, flat.min, flat.max, flat.n], [142, 141, 143, 3], "flat の 3 回だけを集計する");
    assert.equal(listed.median, 100_000);
    assert.deepEqual([nestedRoot.median, nestedRoot.n], [0.9, 1]);
    assert.deepEqual([nestedChild.median, nestedChild.n], [0.12, 1]);
    const md = await readFile(path.join(outDir, "latest.md"), "utf8");
    assert.match(md, /## 4\. ツリーのフォルダの読み込み/);
    assert.ok(md.includes("| 1 フォルダに全ファイル: 読み込み+除外+並べ替え | 142.0 | 141.0 | 143.0 | ms | 3 |"), md);
    assert.match(md, /NFR-005/);
  });

  it("ツリーの計測が無ければ 4. の節は未計測", async () => {
    const { tree: _tree, ...withoutTree } = FAKE;
    const outDir = path.join(root, "no-tree");
    await runPerf({ measures: withoutTree, environment: ENV, outDir, now: NOW, log: quiet });
    const md = await readFile(path.join(outDir, "latest.md"), "utf8");
    assert.match(md, /## 4\. ツリーのフォルダの読み込み[^\n]*\n\n未計測。/);
  });

  it("--tree-files で見本のファイルの数を変えられ、既定は 10 万", () => {
    assert.equal(DEFAULT_TREE_FILES, 100_000);
    assert.equal(TREE_FILES_ENV, "ARIN_PERF_TREE_FILES");
    assert.deepEqual(parseArgs([]), {});
    assert.deepEqual(parseArgs(["--tree-files=20000"]), { treeFiles: 20_000 });
    assert.equal(parseArgs(["--app=/x/Arin.app", "--tree-files=100"]).treeFiles, 100);
    for (const bad of ["--tree-files=99", "--tree-files=abc", "--tree-files=1e5", "--tree-files=-5", "--tree-files="]) {
      assert.throws(() => parseArgs([bad]), bad);
    }
    assert.throws(() => parseArgs(["--unknown"]));
  });

  it("既定の計測にツリーの読み込みが含まれる", () => {
    assert.deepEqual(Object.keys(defaultMeasures()), ["pty_read", "sessions", "app", "tiles_e2e", "tree"]);
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

// ---------------------------------------------------------------------------
// M4 の 9 枚の計測(T49b。NFR-009)
// ---------------------------------------------------------------------------

/** 画面側の 1 回分(e2e/perf-tiles.spec.ts が書く runs の 1 つ)。 */
const e2eRun = (run, ingestMs) => ({
  kind: "tiles_e2e",
  run,
  bytesPerTerminal: 2_097_152,
  terminals: 9,
  ingestMs,
  mibPerS: 18_874_368 / 1_048_576 / (ingestMs / 1000),
  frames: { count: 100, medianMs: 16.7, p95Ms: 33.3, maxMs: 120, over50Percent: 2 },
  clickToCommandMs: 3,
  keyToCommandMs: 2,
  nextPageToCommandMs: 4,
  pageAwayMs: 30,
  pageBackMs: 60,
  frontendErrors: 0,
});

const E2E_RUNS = [e2eRun(1, 1000), e2eRun(2, 1200), e2eRun(3, 1500)];

describe("画面側の 9 枚の計測(e2e/perf-tiles.spec.ts)", () => {
  it("画面側の計測の結果を表にまとめる", async () => {
    const outDir = path.join(root, "tiles-e2e");
    const { exitCode, result } = await runPerf({
      measures: { ...FAKE, tiles_e2e: () => ({ status: "ok", runs: E2E_RUNS }) },
      environment: ENV,
      outDir,
      now: NOW,
      log: quiet,
    });
    assert.equal(exitCode, 0);
    const m = result.matrices.tiles_e2e;
    // 描画方式は DOM・unicode11 に固定したので、列は 1 つ(T50b)。
    assert.deepEqual(
      m.columns.map((c) => [c.key, c.n]),
      [["dom-u11", 3]],
    );
    const ingest = m.rows.find((r) => r.label === "9 枚が取り込み終えるまで");
    assert.deepEqual(ingest.cells, { "dom-u11": 1200 });
    assert.equal(
      m.rows.some((r) => r.label.includes("WebGL")),
      false,
    );

    const md = await readFile(path.join(outDir, "latest.md"), "utf8");
    assert.match(md, /## 5\. 画面側: 9 枚のタイルに同時に流す/);
    assert.ok(md.includes("| 項目 | DOM・unicode11(3 回) | 単位 | 目安・備考 |"), md);
    assert.ok(md.includes("| 9 枚が取り込み終えるまで | 1,200 | ms |"), md);
    assert.match(md, /\| tiles_e2e \| 計測した \| 3 \|/);
  });

  it("画面側の計測が無ければ 5. の節は未計測", async () => {
    const outDir = path.join(root, "no-tiles-e2e");
    await runPerf({ measures: FAKE, environment: ENV, outDir, now: NOW, log: quiet });
    const md = await readFile(path.join(outDir, "latest.md"), "utf8");
    assert.match(md, /## 5\. 画面側[^\n]*\n\n未計測。/);
  });
});

/** perf_sessions.rs の M4 の計測が出す行(形を写したもの)。 */
const MULTI_LINES = [
  ...[1, 2, 3].flatMap((run) => [
    `ARIN_PERF {"kind":"multi_ack","run":${run},"terminals":1,"bel":false,"bytes_per_terminal":4194304,"bytes":4300000,"sends":70,"avg_send_bytes":61000,"max_send_bytes":66000,"max_unacked":66000,"wall_ms":${20 + run},"slowest_ms":${20 + run},"fastest_ms":${20 + run},"mib_per_s_total":190,"rss_before_kib":100,"rss_after_kib":110}`,
    `ARIN_PERF {"kind":"multi_ack","run":${run},"terminals":9,"bel":false,"bytes_per_terminal":4194304,"bytes":38700000,"sends":630,"avg_send_bytes":61000,"max_send_bytes":66000,"max_unacked":70000,"wall_ms":${90 + run},"slowest_ms":${88 + run},"fastest_ms":${60 + run},"mib_per_s_total":400,"rss_before_kib":100,"rss_after_kib":140}`,
  ]),
  'ARIN_PERF {"kind":"multi_ack","run":1,"terminals":9,"bel":true,"bytes_per_terminal":4194304,"bytes":38700000,"sends":630,"avg_send_bytes":61000,"max_send_bytes":66000,"max_unacked":70000,"wall_ms":120,"slowest_ms":118,"fastest_ms":80,"mib_per_s_total":300,"rss_before_kib":100,"rss_after_kib":140}',
  'ARIN_PERF {"kind":"multi_stall","run":1,"terminals":9,"bytes_per_terminal":8388608,"stalled_unacked_max":525184,"stalled_unacked_min":524300,"stalled_unacked_sum":4720000,"stalled_received_sum":4720000,"paused_after_ms":40,"settle_ms":1000,"ended_while_stalled":0,"resumed_to_end_ms":300,"rss_before_kib":100,"rss_stalled_kib":120}',
  'ARIN_PERF {"kind":"attention_scan","run":1,"bytes":7700000,"chunk":16384,"hits":0,"ms":3.5,"mib_per_s":2100.0}',
].join("\n");

describe("9 枚の計測の行(perf_sessions.rs の M4 の分)", () => {
  it("9_枚の計測の行を読める", async () => {
    const runs = parsePerfLines(`running 4 tests\n${MULTI_LINES}\ntest result: ok\n`);
    assert.equal(runs.length, 9);
    const outDir = path.join(root, "multi");
    const { exitCode, result } = await runPerf({
      measures: { ...FAKE, sessions: () => ({ status: "ok", runs: [...FAKE.sessions().runs, ...runs] }) },
      environment: ENV,
      outDir,
      now: NOW,
      log: quiet,
    });
    assert.equal(exitCode, 0);
    const m = result.matrices.multi_ack;
    assert.deepEqual(
      m.columns.map((c) => [c.key, c.n]),
      [
        ["1", 3],
        ["9", 3],
        ["1-bel", 0],
        ["9-bel", 1],
      ],
    );
    const wall = m.rows.find((r) => r.label === "最初の流し始め→最後の流し終わり");
    assert.deepEqual(wall.cells, { 1: 22, 9: 92, "1-bel": null, "9-bel": 120 });
    const [stalled] = result.summary.multi_stall;
    assert.deepEqual([stalled.median, stalled.n], [525_184, 1]);
    assert.deepEqual([result.summary.attention_scan[0].median, result.summary.attention_scan[0].n], [3.5, 1]);
    // 1 枚の 2a・2b は、M4 の行が混ざっても今までどおり集計する。
    assert.equal(result.summary.session_ack[0].n, 1);

    const md = await readFile(path.join(outDir, "latest.md"), "utf8");
    assert.match(md, /## 2c\. 1 枚と 9 枚で同時に `yes` を流す/);
    assert.ok(md.includes("| 最初の流し始め→最後の流し終わり | 22.0 | 92.0 | — | 120.0 | ms |"), md);
    assert.match(md, /## 2d\. 9 枚で処理済みを返さない送り先/);
    assert.match(md, /## 2e\. アテンションの走査器だけ/);
    // 節の順: 2b → 2c → 2d → 2e → 3.
    const order = ["## 2b.", "## 2c.", "## 2d.", "## 2e.", "## 3.", "## 3b.", "## 4.", "## 5."].map((h) => md.indexOf(h));
    assert.ok(order.every((i, k) => i >= 0 && (k === 0 || i > order[k - 1])), `節の順: ${order}`);
  });

  it("アプリの 9 枚の組は 3b. に、5 プロジェクトは 3. に集める", async () => {
    const proc = (kind, footprint) => ({ pid: 1, kind, footprint, rss: footprint });
    const appRun = (scenario, extra = {}) => ({
      run: 1,
      scenario,
      projects: scenario === "tabs5" ? 5 : 9,
      shell_count: scenario === "tabs5" ? 1 : 9,
      startup_ready_ms: 300,
      startup_terminal_ms: 400,
      startup_shells_ms: scenario === "tabs5" ? null : 900,
      processes: [proc("app", 100 * 1_048_576), proc("com.apple.WebKit.WebContent", 200 * 1_048_576)],
      frontend_errors: [],
      frontend_errors_dropped: 0,
      webkit_exited_with_app: true,
      shells_left_after_exit: [],
      free_percent_before: 70,
      flood: null,
      ...extra,
    });
    const flood = { bytes_per_terminal: FLOOD_BYTES, done_count: 9, all_done_ms: 5000, terminal_ms: [1000, 2000, 3000], peak_app_rss: 150 * 1_048_576, peak_webkit_rss: 400 * 1_048_576 };
    const { result } = await runPerf({
      measures: {
        ...FAKE,
        app: () => ({
          status: "ok",
          runs: [appRun("tabs5"), appRun("tiles9-dom"), appRun("tiles9-dom-flood", { flood, frontend_errors: ["[arin:frontend-error] x"] })],
        }),
      },
      environment: ENV,
      outDir: path.join(root, "app-tiles"),
      now: NOW,
      log: quiet,
    });
    assert.equal(result.summary.app[0].n, 1, "3. は tabs5 だけ");
    const m = result.matrices.app_tiles;
    assert.deepEqual(
      m.columns.map((c) => c.n),
      [1, 1],
    );
    const cell = (label) => m.rows.find((r) => r.label === label).cells;
    assert.equal(cell("起動→シェルが 9 個揃う").dom, 900);
    assert.equal(cell("本体 + WebKit(footprint)").dom, 300);
    assert.equal(cell("1 枚が流し終えるまで(9 枚の中央値)")["dom-flood"], 2000);
    assert.equal(cell("流しているあいだの WebKit の RSS の最大")["dom-flood"], 400);
    assert.equal(cell("画面の失敗の行")["dom-flood"], 1);
    assert.equal(cell("起動→9 枚が流し終える").dom, null);
  });
});

describe("計測の動かし方(T49b)", () => {
  const fresh = async (name, recordedAt, runs = E2E_RUNS.map(({ kind: _k, ...r }) => r)) => {
    const file = path.join(root, name);
    await writeFile(file, JSON.stringify({ recordedAt, browser: "webkit 26", runs }));
    return file;
  };

  it("計測が動かせないときだけ終了コードが_0_でない", async () => {
    const at = Date.parse("2026-11-15T12:00:00Z");
    const now = () => at;
    const calls = [];
    const ok = (cmd, args, opts) => {
      calls.push({ cmd, args, env: opts.env[PERF_E2E_ENV] });
      return { status: 0 };
    };
    // spec が通り、今回の結果があれば ok。
    const good = await runTilesE2e({ log: quiet, run: ok, latestFile: await fresh("e2e-ok.json", "2026-11-15T12:00:05Z"), now });
    assert.equal(good.status, "ok");
    assert.equal(good.runs.length, E2E_RUNS.length);
    assert.ok(good.runs.every((r) => r.kind === "tiles_e2e"));
    assert.deepEqual(calls[0], { cmd: "npx", args: ["playwright", "test", TILES_E2E_SPEC, "--workers=1", "--reporter=list"], env: "1" });

    // spec が失敗した・起動できない・結果が無い・古い・回が無い: error。
    const failed = await runTilesE2e({ log: quiet, run: () => ({ status: 1 }), latestFile: await fresh("e2e-f.json", "2026-11-15T12:00:05Z"), now });
    assert.equal(failed.status, "error");
    const spawnError = await runTilesE2e({ log: quiet, run: () => ({ error: new Error("ENOENT") }), now });
    assert.match(spawnError.error, /起動できなかった/);
    const missing = await runTilesE2e({ log: quiet, run: ok, latestFile: path.join(root, "no-such.json"), now });
    assert.match(missing.error, /読めなかった/);
    const stale = await runTilesE2e({ log: quiet, run: ok, latestFile: await fresh("e2e-old.json", "2026-11-15T11:00:00Z"), now });
    assert.match(stale.error, /古い/);
    const empty = await runTilesE2e({ log: quiet, run: ok, latestFile: await fresh("e2e-empty.json", "2026-11-15T12:00:05Z", []), now });
    assert.equal(empty.status, "error");

    // runPerf の終了コード: 動かせなかったら 1、数値がどれだけ悪くても 0。
    const broken = await runPerf({
      measures: { tiles_e2e: () => failed },
      environment: ENV,
      outDir: path.join(root, "e2e-broken"),
      now: NOW,
      log: quiet,
    });
    assert.equal(broken.exitCode, 1);
    const slow = await runPerf({
      measures: { tiles_e2e: () => ({ status: "ok", runs: [e2eRun(1, 1e9)] }) },
      environment: ENV,
      outDir: path.join(root, "e2e-slow"),
      now: NOW,
      log: quiet,
    });
    assert.equal(slow.exitCode, 0);
  });

  it("性能の計測の spec は npm run e2e では動かさず ARIN_PERF_E2E=1 のときだけ拾う", async () => {
    const config = await readFile(path.join(import.meta.dirname, "..", "playwright.config.ts"), "utf8");
    assert.match(config, /process\.env\.ARIN_PERF_E2E === "1"/);
    assert.match(config, /testIgnore: PERF_E2E \? \[\] : \["\*\*\/perf-\*\.spec\.ts"\]/);
    assert.equal(PERF_E2E_ENV, "ARIN_PERF_E2E");
    assert.equal(TILES_E2E_SPEC, "e2e/perf-tiles.spec.ts");
  });
});

describe("9 プロジェクトのタイル表示の起動(計画書 M4 の【仮定】18)", () => {
  it("displayMode を渡すと ui.displayMode に書き、渡さなければ書かない", async () => {
    const work = path.join(root, "state-9");
    await mkdir(work);
    const { projectDirs, fixture } = await preparePerfState(work, TILE_PROJECT_COUNT, { displayMode: "tiles" });
    assert.equal(projectDirs.length, 9);
    const state = JSON.parse(fixture);
    assert.equal(state.ui.displayMode, "tiles");
    assert.equal(state.projects.length, 9);
    assert.equal(new Set(state.projects.map((p) => p.id)).size, 9);
    assert.equal("displayMode" in JSON.parse(perfStateFixture(["/tmp/a"])).ui, false);
  });

  it("起動の組は 5 プロジェクトと、9 枚の落ち着いた状態と流す状態で、描画方式の指定を渡さない", () => {
    assert.deepEqual(
      APP_SCENARIOS.map((s) => [s.name, s.projects, s.displayMode, s.env, s.flood]),
      [
        ["tabs5", 5, null, {}, false],
        ["tiles9-dom", 9, "tiles", {}, false],
        ["tiles9-dom-flood", 9, "tiles", {}, true],
      ],
    );
    assert.ok(APP_SCENARIOS.every((s) => s.runs >= 1 && s.runs <= 3));
  });

  it("流す組の .zshrc は量に上限があり、流し終えたら印を書く", async () => {
    const rc = floodZshrc(1234);
    assert.match(rc, /\| head -c 1234\n/);
    assert.ok(rc.includes(`$HOME/${FLOOD_DONE_PREFIX}$$`));
    assert.throws(() => floodZshrc(0));
    assert.throws(() => floodZshrc(1.5));

    const work = path.join(root, "flood");
    await mkdir(work);
    const home = await prepareFloodHome(work, 10);
    assert.deepEqual(await readdir(home), [".zshrc"]);
    await writeFile(path.join(home, `${FLOOD_DONE_PREFIX}101`), "100.5 101.75\n");
    await writeFile(path.join(home, `${FLOOD_DONE_PREFIX}102.tmp`), "1 2\n");
    assert.deepEqual(await readFloodDone(home), [{ pid: 101, ms: 1250 }]);
  });

  it("メモリの空きが下限を下回ればアプリを起動せずに止める", async () => {
    const app = path.join(root, "Fake.app");
    await mkdir(path.join(app, "Contents", "MacOS"), { recursive: true });
    await writeFile(
      path.join(app, "Contents", "Info.plist"),
      "<plist><dict><key>CFBundleExecutable</key><string>fake</string></dict></plist>",
    );
    const marker = path.join(root, "fake-started");
    const exe = path.join(app, "Contents", "MacOS", "fake");
    await writeFile(exe, `#!/bin/sh\ntouch '${marker}'\n`);
    await chmod(exe, 0o755);
    const realStateDir = path.join(root, "real-state-mem");
    const r = await measureAppMemory({ appPath: app, realStateDir, freePercent: () => 30, log: quiet });
    assert.equal(r.status, "error");
    assert.match(r.error, /メモリの空きが 30%/);
    assert.deepEqual(r.runs, []);
    await assert.rejects(readFile(marker), "アプリを起動していない");
  });
});
