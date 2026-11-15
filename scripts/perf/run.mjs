#!/usr/bin/env node
// 性能の計測(T24。設計書 §2.2「流量制御」「9 枚の簡易確認」、§7.3、§10.1)。`npm run perf` で実行する。
//
// 記録するだけで合否にしない(PRD の NFR-001〜005・009 は目安。C-14)。目安の値は結果の表に並べて書くだけ。
// 終了コードが 0 以外になるのは、計測そのものが実行できなかったとき(テストの異常終了、スクリプトの誤りなど)だけ。
//
// 測るもの:
//   1. PTY の読み取りの速さ(crates/arin-platform/tests/perf_pty.rs。cargo test --release の --ignored)
//   2. PTY から送り先まで(src-tauri/tests/perf_sessions.rs。処理済みをすぐ返す送り先と、返さない送り先)
//   3. 起動中のアプリのメモリと、起動からターミナルの印までの時間(scripts/perf/app-memory.mjs)。.app が無ければ未計測
//   4. ツリーのフォルダの読み込み(crates/arin-platform/tests/perf_tree.rs。T44a、NFR-005。10 万ファイルの見本)
//   M4(T49b。NFR-009、ADR-001)で足したもの:
//   2c〜2e. 1 枚と 9 枚で同時に `yes` を流したとき(ベル入りの組を含む)、9 枚の流量制御、アテンションの走査器だけの速さ
//           (perf_sessions.rs の続き)
//   3b. 9 プロジェクトのタイル表示で起動したときのメモリと、9 枚に流させたとき(app-memory.mjs の APP_SCENARIOS)
//   5. 画面側: WebKit で 9 枚のタイルに偽の Rust 側から同時に流しているあいだの取り込み・フレームの間隔・操作の反応
//      (e2e/perf-tiles.spec.ts。DOM と WebGL、unicode11 と graphemes の組。`npm run e2e` には入らない)
// 画面の反応(描画の滑らかさ、入力の遅れ)は自動では測れないので、結果に「人間が見る項目」として書く。
//
// 出力: testreport/perf/perf-<日時>.json と .md、latest.json と latest.md。
// Node の標準モジュールだけで書く。ソケットやポートは使わない(PRD N2)。
//
// 使い方: node scripts/perf/run.mjs [--app=<Arin.app のパス>] [--tree-files=<見本のファイルの数>]
//   --tree-files は 4. の見本のファイルの数(既定 100000。100 以上の整数)。perf_tree.rs へ ARIN_PERF_TREE_FILES で渡す

import { spawnSync } from "node:child_process";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { measureAppMemory } from "./app-memory.mjs";

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "..");
export const OUT_DIR = path.join(REPO_ROOT, "testreport", "perf");
/** Rust の計測テストが結果を書く行の頭。 */
export const PERF_LINE_PREFIX = "ARIN_PERF ";

const MIB = 1024 * 1024;
/** ツリーの見本のファイルの数を perf_tree.rs へ渡す環境変数。 */
export const TREE_FILES_ENV = "ARIN_PERF_TREE_FILES";
/** ツリーの見本のファイルの数の既定(NFR-005 の 10 万ファイル)。 */
export const DEFAULT_TREE_FILES = 100_000;
/** 画面側の 9 枚の計測(e2e/perf-tiles.spec.ts)を Playwright に拾わせる環境変数(playwright.config.ts の testIgnore)。 */
export const PERF_E2E_ENV = "ARIN_PERF_E2E";
/** 画面側の 9 枚の計測の spec。 */
export const TILES_E2E_SPEC = "e2e/perf-tiles.spec.ts";
/** 画面側の 9 枚の計測が書く最新の結果(spec が testreport/perf/ に書く)。 */
export const TILES_E2E_LATEST = "e2e-tiles-latest.json";

// ---------------------------------------------------------------------------
// 集計
// ---------------------------------------------------------------------------

/** 出力から `ARIN_PERF {json}` の行を取り出す。 */
export function parsePerfLines(text) {
  return text
    .split("\n")
    .map((line) => {
      const i = line.indexOf(PERF_LINE_PREFIX);
      return i < 0 ? null : line.slice(i + PERF_LINE_PREFIX.length).trim();
    })
    .filter((s) => s !== null && s.startsWith("{"))
    .map((s) => JSON.parse(s));
}

/** 中央値と最小・最大。数でない値は除く。 */
export function stats(values) {
  const v = values.filter((x) => typeof x === "number" && Number.isFinite(x)).sort((a, b) => a - b);
  if (v.length === 0) return null;
  const mid = Math.floor(v.length / 2);
  const median = v.length % 2 === 1 ? v[mid] : (v[mid - 1] + v[mid]) / 2;
  return { median, min: v[0], max: v[v.length - 1], n: v.length };
}

const sum = (xs) => xs.reduce((a, b) => a + (b ?? 0), 0);

/** アプリの 1 回分から、表に出す値を足す。 */
function deriveApp(run) {
  const of = (pred) => run.processes.filter(pred).map((p) => p.footprint);
  const app = of((p) => p.kind === "app")[0] ?? null;
  const webkit = of((p) => p.kind.startsWith("com.apple.WebKit."));
  return {
    ...run,
    app_footprint: app,
    webkit_footprint: sum(webkit),
    webkit_count: webkit.length,
    total_footprint: app === null ? null : app + sum(webkit),
    shell_footprint: sum(of((p) => p.kind === "shell")),
    app_rss: run.processes.find((p) => p.kind === "app")?.rss ?? null,
  };
}

/**
 * 表の行の定義。kind は計測の種類、get は 1 回分から値を取り出す関数。
 * guide は PRD・設計書の目安(合否に使わない)。
 */
/** ツリーの読み込みの 1 回分のうち、見本が `fixture` のものだけから値を取り出す。 */
const treeOf = (fixture, get) => (r) => (r.fixture === fixture ? get(r) : null);

const ROWS = {
  pty_read: [
    { label: "所要時間(最初のバイト→最後のバイト)", unit: "ms", get: (r) => r.first_to_last_ms, guide: "NFR-004(UI が固まらない。切替 100ms 以内)の参考" },
    { label: "スループット", unit: "MiB/s", get: (r) => r.mib_per_s },
    { label: "行/秒", unit: "行/s", get: (r) => r.lines_per_s, digits: 0 },
    { label: "受け取った量", unit: "B", get: (r) => r.bytes, digits: 0, guide: "ファイルの大きさ + 改行の CR(1 行 1 バイト)" },
    { label: "読み取りの回数", unit: "回", get: (r) => r.reads, digits: 0 },
    { label: "1 回の読み取りの平均", unit: "B", get: (r) => r.bytes / r.reads, digits: 0, guide: "バッファは 16 KiB" },
  ],
  session_ack: [
    { label: "所要時間(最初の送信→最後の送信)", unit: "ms", get: (r) => r.first_to_last_ms, guide: "NFR-004 の参考" },
    { label: "スループット", unit: "MiB/s", get: (r) => r.mib_per_s },
    { label: "送った回数", unit: "回", get: (r) => r.sends, digits: 0 },
    { label: "1 回あたりの平均", unit: "B", get: (r) => r.avg_send_bytes, digits: 0, guide: "まとめの目安 64 KiB(65536)" },
    { label: "1 回あたりの最大", unit: "B", get: (r) => r.max_send_bytes, digits: 0 },
    { label: "未処理の量の最大値", unit: "B", get: (r) => r.max_unacked, digits: 0, guide: "上限 512 KiB(524288)" },
    { label: "テストのプロセスの RSS の増分", unit: "KiB", get: (r) => r.rss_after_kib - r.rss_before_kib, digits: 0 },
  ],
  session_stall: [
    { label: "頭打ちのときの未処理の量", unit: "B", get: (r) => r.stalled_unacked, digits: 0, guide: "上限 512 KiB(524288)+ 1 回の読み取り" },
    { label: "止まるまでに受け取った量", unit: "B", get: (r) => r.stalled_received, digits: 0, guide: "ファイルは約 7.7 MB" },
    { label: "cat の開始から読み取りが止まるまで", unit: "ms", get: (r) => r.paused_after_ms },
    { label: "止めたまま最後まで届いた回数", unit: "回", get: (r) => (r.ended_while_stalled ? 1 : 0), digits: 0, guide: "0 なら止まっている" },
    { label: "処理済みを返してから最後まで", unit: "ms", get: (r) => r.resumed_to_end_ms, guide: "再開できること" },
    { label: "テストのプロセスの RSS の増分(止まっている間)", unit: "KiB", get: (r) => r.rss_stalled_kib - r.rss_before_kib, digits: 0 },
  ],
  app: [
    { label: "起動→準備完了の印", unit: "ms", get: (r) => r.startup_ready_ms, digits: 0, guide: "NFR-002(コールドスタートから操作可能まで 1 秒以内)の参考。画面が準備完了を通知するまで" },
    { label: "起動→ターミナルの印", unit: "ms", get: (r) => r.startup_terminal_ms, digits: 0, guide: "NFR-002 の参考。シェルの出力が画面に取り込まれ、処理済みが Rust に戻るまで" },
    { label: "アプリ本体(footprint)", unit: "MiB", get: (r) => r.app_footprint / MIB, guide: "NFR-001: 本体 200MB 以下(5 プロジェクト・各 1 タブ)" },
    { label: "WebKit の補助プロセスの合計(footprint)", unit: "MiB", get: (r) => r.webkit_footprint / MIB },
    { label: "本体 + WebKit(footprint)", unit: "MiB", get: (r) => r.total_footprint / MIB, guide: "設計書 §7.3 の仮定(§15.2 #11)の記録方法" },
    { label: "シェルの数(アプリの子プロセス)", unit: "個", get: (r) => r.shell_count, digits: 0, guide: "5 プロジェクト・各 1 タブ。画面が接続したタブだけシェルを起動する" },
    { label: "シェル(子プロセス。footprint)", unit: "MiB", get: (r) => r.shell_footprint / MIB, guide: "NFR-001 では除く" },
    { label: "アプリ本体(ps の RSS)", unit: "MiB", get: (r) => r.app_rss / MIB },
    { label: "WebKit の補助プロセスの数", unit: "個", get: (r) => r.webkit_count, digits: 0, guide: "WebContent・GPU・Networking" },
    { label: "WebKit がアプリの終了と一緒に消えた", unit: "1=はい", get: (r) => (r.webkit_exited_with_app ? 1 : 0), digits: 0, guide: "1 なら別のアプリの WebKit と取り違えていない" },
  ],
  multi_stall: [
    { label: "頭打ちのときの未処理の量(9 枚の最大)", unit: "B", get: (r) => r.stalled_unacked_max, digits: 0, guide: "上限 512 KiB(524288)+ 1 回の読み取り。1 枚の 2b と同じなら、枚数でしきい値がずれていない" },
    { label: "頭打ちのときの未処理の量(9 枚の最小)", unit: "B", get: (r) => r.stalled_unacked_min, digits: 0 },
    { label: "頭打ちのときの未処理の量(9 枚の合計)", unit: "B", get: (r) => r.stalled_unacked_sum, digits: 0, guide: "画面が抱えうる量の上限の目安(9 × 512 KiB 余り)" },
    { label: "流し始めから 9 枚すべての読み取りが止まるまで", unit: "ms", get: (r) => r.paused_after_ms },
    { label: "止めたまま最後まで届いた枚数", unit: "枚", get: (r) => r.ended_while_stalled, digits: 0, guide: "0 なら全部止まっている" },
    { label: "処理済みを返してから 9 枚が最後まで", unit: "ms", get: (r) => r.resumed_to_end_ms, guide: "1 枚あたり 8 MiB の残り" },
    { label: "テストのプロセスの RSS の増分(止まっている間)", unit: "KiB", get: (r) => r.rss_stalled_kib - r.rss_before_kib, digits: 0, guide: "流量制御が効いていれば数 MiB に収まる" },
  ],
  attention_scan: [
    { label: "走査の時間(10 万行・約 7.7 MB、16 KiB ずつ)", unit: "ms", get: (r) => r.ms, digits: 2, guide: "2a の所要時間(約 40ms)に対する上乗せの上限の目安" },
    { label: "走査の速さ", unit: "MiB/s", get: (r) => r.mib_per_s, digits: 0 },
    { label: "契機があった塊の数", unit: "個", get: (r) => r.hits, digits: 0, guide: "ベルを含まないので 0" },
  ],
  tree_list: [
    { label: "1 フォルダに全ファイル: 読み込み+除外+並べ替え", unit: "ms", get: treeOf("flat", (r) => r.total_ms), guide: "NFR-005(展開 100ms 以内)の参考。画面側の時間は含まない" },
    { label: "1 フォルダに全ファイル: 読み込み(範囲の検査を含む)", unit: "ms", get: treeOf("flat", (r) => r.read_ms) },
    { label: "1 フォルダに全ファイル: 除外", unit: "ms", get: treeOf("flat", (r) => r.filter_ms), digits: 3 },
    { label: "1 フォルダに全ファイル: 並べ替え", unit: "ms", get: treeOf("flat", (r) => r.sort_ms) },
    { label: "1 フォルダに全ファイル: 一覧の項目の数", unit: "個", get: treeOf("flat", (r) => r.listed), digits: 0, guide: "既定で隠す 3 つを除いた数" },
    { label: "フォルダ × 100 ファイル: ルート(フォルダの一覧)", unit: "ms", get: treeOf("nested_root", (r) => r.total_ms), digits: 3, guide: "NFR-005 の参考" },
    { label: "フォルダ × 100 ファイル: ルートの項目の数", unit: "個", get: treeOf("nested_root", (r) => r.listed), digits: 0 },
    { label: "フォルダ × 100 ファイル: 中の 1 フォルダ", unit: "ms", get: treeOf("nested_child", (r) => r.total_ms), digits: 3, guide: "NFR-005 の参考" },
    { label: "見本の全ファイルの数", unit: "個", get: treeOf("flat", (r) => r.fixture_files), digits: 0 },
    { label: "見本(1 フォルダ)を作る時間", unit: "s", get: treeOf("flat", (r) => r.create_s), guide: "計測の対象ではない(参考)" },
  ],
};

const SECTION_TITLES = {
  pty_read: "1. PTY の読み取り(arin-platform、実 PTY、10 万行の cat)",
  session_ack: "2a. PTY から送り先まで: 処理済みをすぐ返す送り先(SessionRegistry)",
  session_stall: "2b. PTY から送り先まで: 処理済みを返さない送り先(流量制御)",
  app: "3. 起動中のアプリ(.app、スモークと同じ起動、5 プロジェクト・各 1 タブ)",
  multi_stall: "2d. 9 枚で処理済みを返さない送り先(流量制御。各 8 MiB の `yes`)",
  attention_scan: "2e. アテンションの走査器だけ(メモリ上。T48c で読み取りスレッドが常に行うようになった処理)",
  tree_list: "4. ツリーのフォルダの読み込み(arin-platform、実ファイル、NFR-005)",
};

// ---------------------------------------------------------------------------
// 比べる表(列=組、行=項目。M4 の 9 枚の計測)
// ---------------------------------------------------------------------------

const multiCol = (terminals, bel) => (r) => r.terminals === terminals && r.bel === bel;
const appCol = (scenario) => (r) => r.scenario === scenario;
const e2eCol = (renderer, unicode) => (r) => r.renderer === renderer && r.unicode === unicode;
const median = (xs) => stats(xs)?.median ?? null;
const pick = (key) => (r) => r[key];

/**
 * 比べる表の定義。source は runsByKind の種類、columns は組(filter が真の 1 回分を集める)、rows は項目。
 * 各セルは、その組の各回の値の中央値と回数。
 */
const MATRICES = {
  multi_ack: {
    title: "2c. 1 枚と 9 枚で同時に `yes` を流す(処理済みをすぐ返す送り先。1 枚あたり 4 MiB)",
    note: "ベル入りは、行ごとにベル(BEL)を入れ、読み取りの塊ごとにアテンションの契機が起きる組(Workspace のロックを毎回取る)。",
    columns: [
      { key: "1", label: "1 枚", filter: multiCol(1, false) },
      { key: "9", label: "9 枚", filter: multiCol(9, false) },
      { key: "1-bel", label: "1 枚(ベル入り)", filter: multiCol(1, true) },
      { key: "9-bel", label: "9 枚(ベル入り)", filter: multiCol(9, true) },
    ],
    rows: [
      { label: "最初の流し始め→最後の流し終わり", unit: "ms", get: pick("wall_ms"), guide: "NFR-009 の参考" },
      { label: "全体のスループット", unit: "MiB/s", get: pick("mib_per_s_total") },
      { label: "いちばん遅い 1 枚", unit: "ms", get: pick("slowest_ms") },
      { label: "いちばん速い 1 枚", unit: "ms", get: pick("fastest_ms") },
      { label: "未処理の量の最大値(どの 1 枚でも)", unit: "B", get: pick("max_unacked"), digits: 0, guide: "上限 512 KiB(524288)" },
      { label: "1 回の送信の平均", unit: "B", get: pick("avg_send_bytes"), digits: 0, guide: "まとめの目安 64 KiB" },
      { label: "テストのプロセスの RSS の増分", unit: "KiB", get: (r) => r.rss_after_kib - r.rss_before_kib, digits: 0 },
    ],
  },
  app_tiles: {
    title: "3b. 9 プロジェクトのタイル表示で起動(.app。各 1 タブ、`ui.displayMode: \"tiles\"`)",
    note: "「流す」は各シェルが起動直後に `yes | head -c 4 MiB` を流す組。印は 1 枚目の出力で立つので、9 枚が揃ったことはシェルの数で見る。",
    columns: [
      { key: "dom", label: "DOM", filter: appCol("tiles9-dom") },
      { key: "webgl", label: "WebGL", filter: appCol("tiles9-webgl") },
      { key: "dom-flood", label: "DOM・流す", filter: appCol("tiles9-dom-flood") },
      { key: "webgl-flood", label: "WebGL・流す", filter: appCol("tiles9-webgl-flood") },
    ],
    rows: [
      { label: "起動→準備完了の印", unit: "ms", get: pick("startup_ready_ms"), digits: 0, guide: "NFR-002 の参考" },
      { label: "起動→ターミナルの印(最初の 1 枚)", unit: "ms", get: pick("startup_terminal_ms"), digits: 0 },
      { label: "起動→シェルが 9 個揃う", unit: "ms", get: pick("startup_shells_ms"), digits: 0, guide: "画面が 9 枚とも接続した" },
      { label: "アプリ本体(footprint)", unit: "MiB", get: (r) => r.app_footprint / MIB, guide: "NFR-001 は 5 プロジェクトで 200MB 以下(参考)" },
      { label: "WebKit の補助プロセスの合計(footprint)", unit: "MiB", get: (r) => r.webkit_footprint / MIB },
      { label: "本体 + WebKit(footprint)", unit: "MiB", get: (r) => r.total_footprint / MIB, guide: "3. の 5 プロジェクトと比べる" },
      { label: "シェルの数", unit: "個", get: pick("shell_count"), digits: 0 },
      { label: "起動→9 枚が流し終える", unit: "ms", get: (r) => r.flood?.all_done_ms ?? null, digits: 0, guide: "画面が取り込んで処理済みを返す速さで決まる(流量制御)" },
      { label: "1 枚が流し終えるまで(9 枚の中央値)", unit: "ms", get: (r) => median(r.flood?.terminal_ms ?? []), digits: 0 },
      { label: "1 枚が流し終えるまで(9 枚の最大)", unit: "ms", get: (r) => stats(r.flood?.terminal_ms ?? [])?.max ?? null, digits: 0 },
      { label: "流しているあいだの WebKit の RSS の最大", unit: "MiB", get: (r) => (r.flood == null ? null : r.flood.peak_webkit_rss / MIB), guide: "増え続けないか(流し終えたあとの footprint と比べる)" },
      { label: "流しているあいだの本体の RSS の最大", unit: "MiB", get: (r) => (r.flood == null ? null : r.flood.peak_app_rss / MIB) },
      { label: "画面の失敗の行(DOM に戻した報告など)", unit: "行", get: (r) => (r.frontend_errors?.length ?? 0) + (r.frontend_errors_dropped ?? 0), digits: 0, guide: "0 であること(WebGL のコンテキストが足りないと出る)" },
      { label: "起動の前のメモリの空き", unit: "%", get: pick("free_percent_before"), digits: 0 },
      { label: "WebKit がアプリの終了と一緒に消えた", unit: "1=はい", get: (r) => (r.webkit_exited_with_app ? 1 : 0), digits: 0 },
    ],
  },
  tiles_e2e: {
    title: "5. 画面側: 9 枚のタイルに同時に流す(Playwright の WebKit、偽の Rust 側。e2e/perf-tiles.spec.ts)",
    note: "偽の Rust 側が、製品と同じ流量制御(未処理 512 KiB まで、64 KiB ずつ)で 9 枚に同時に流す。WKWebView の実機とは別のビルドなので、組どうしの比較に使う。",
    columns: [
      { key: "dom-u11", label: "DOM・unicode11", filter: e2eCol("dom", "unicode11") },
      { key: "webgl-u11", label: "WebGL・unicode11", filter: e2eCol("webgl", "unicode11") },
      { key: "dom-gr", label: "DOM・graphemes", filter: e2eCol("dom", "graphemes") },
      { key: "webgl-gr", label: "WebGL・graphemes", filter: e2eCol("webgl", "graphemes") },
    ],
    rows: [
      { label: "9 枚が取り込み終えるまで", unit: "ms", get: pick("ingestMs"), digits: 0, guide: "流し始めから、全量の処理済みが返るまで" },
      { label: "取り込みの速さ(9 枚の合計)", unit: "MiB/s", get: pick("mibPerS") },
      { label: "フレームの間隔の中央値", unit: "ms", get: (r) => r.frames?.medianMs ?? null, guide: "60Hz なら 16.7" },
      { label: "フレームの間隔の 95 パーセンタイル", unit: "ms", get: (r) => r.frames?.p95Ms ?? null },
      { label: "フレームの間隔の最大", unit: "ms", get: (r) => r.frames?.maxMs ?? null },
      { label: "50ms を超えたフレームの割合", unit: "%", get: (r) => r.frames?.over50Percent ?? null },
      { label: "タイルのクリック→フォーカスのコマンド", unit: "ms", get: pick("clickToCommandMs"), guide: "NFR-004(100ms 以内)の参考。入力のイベントが届くまでの待ち(主スレッドが塞がっている時間)は含まない" },
      { label: "キー入力→書き込みのコマンド", unit: "ms", get: pick("keyToCommandMs") },
      { label: "次のページのボタン→コマンド", unit: "ms", get: pick("nextPageToCommandMs") },
      { label: "2 ページ目の写し→描き終わり", unit: "ms", get: pick("pageAwayMs"), guide: "9 枚を隠し 1 枚を出す" },
      { label: "1 ページ目の写し→描き終わり", unit: "ms", get: pick("pageBackMs"), guide: "9 枚を出し直す(WebGL はコンテキストを作り直す)" },
      { label: "WebGL の canvas の数(1 ページ目)", unit: "個", get: pick("webglCanvases"), digits: 0, guide: "DOM なら 0。WebGL は 1 枚に canvas が 2 つ(addon-webgl 0.19.0)なので 9 枚で 18" },
      { label: "WebGL の canvas の数(戻ったあと)", unit: "個", get: pick("webglCanvasesAfter"), digits: 0 },
      { label: "画面の失敗の報告", unit: "件", get: pick("frontendErrors"), digits: 0, guide: "0 であること(DOM に戻した報告など)" },
    ],
  },
};

/** 比べる表の値。種類 → { title, note, columns: [{key,label,n}], rows: [{label, unit, guide, digits, cells: {key: median}}] }。 */
export function summarizeMatrices(byKind) {
  const out = {};
  for (const [kind, m] of Object.entries(MATRICES)) {
    const runs = byKind[kind] ?? [];
    const groups = m.columns.map((c) => ({ ...c, runs: runs.filter(c.filter) }));
    out[kind] = {
      title: m.title,
      note: m.note,
      columns: groups.map((g) => ({ key: g.key, label: g.label, n: g.runs.length })),
      rows: m.rows.map((row) => ({
        label: row.label,
        unit: row.unit,
        guide: row.guide ?? "",
        digits: row.digits ?? 1,
        cells: Object.fromEntries(
          groups.map((g) => [
            g.key,
            median(
              g.runs.map((r) => {
                try {
                  return row.get(r);
                } catch {
                  return null;
                }
              }),
            ),
          ]),
        ),
      })),
    };
  }
  return out;
}

/** 計測の結果を、種類ごとの 1 回分の一覧にする。 */
function runsByKind(measures) {
  const out = {
    pty_read: [],
    session_ack: [],
    session_stall: [],
    multi_ack: [],
    multi_stall: [],
    attention_scan: [],
    app: [],
    app_tiles: [],
    tiles_e2e: [],
    tree_list: [],
  };
  for (const r of measures.pty_read?.runs ?? []) if (r.kind === "pty_read") out.pty_read.push(r);
  for (const r of measures.sessions?.runs ?? []) if (out[r.kind] && r.kind !== "app" && r.kind !== "app_tiles") out[r.kind].push(r);
  // 5 プロジェクト(tabs5。M4 より前の結果には scenario が無い)は 3.、それ以外の組は 3b. に集める。
  for (const r of measures.app?.runs ?? []) {
    const derived = deriveApp(r);
    if (r.scenario === undefined || r.scenario === "tabs5") out.app.push(derived);
    else out.app_tiles.push(derived);
  }
  for (const r of measures.tiles_e2e?.runs ?? []) if (r.kind === "tiles_e2e") out.tiles_e2e.push(r);
  for (const r of measures.tree?.runs ?? []) if (r.kind === "tree_list") out.tree_list.push(r);
  return out;
}

/** 表の値(種類 → 行の一覧)。 */
export function summarize(measures) {
  const byKind = runsByKind(measures);
  const summary = {};
  for (const [kind, rows] of Object.entries(ROWS)) {
    summary[kind] = rows.map((row) => ({
      label: row.label,
      unit: row.unit,
      guide: row.guide ?? "",
      ...(stats(byKind[kind].map((r) => {
        try {
          return row.get(r);
        } catch {
          return null;
        }
      })) ?? { median: null, min: null, max: null, n: 0 }),
      digits: row.digits ?? 1,
    }));
  }
  return summary;
}

// ---------------------------------------------------------------------------
// 人が読む結果
// ---------------------------------------------------------------------------

export const HUMAN_ITEMS = [
  "描画の滑らかさ: `cat` で 10 万行を流しているあいだのスクロールと、切替の操作が 100ms 以内に反応するか(NFR-004。DOM と WebGL を比べる。設計書 §2.2「描画方式」)",
  "入力の遅れ: キーを押してから文字が出るまでの遅れ、日本語入力の候補の出方",
  "9 枚同時表示の体感: 9 枚のタイルで Claude Code などを動かし、いくつかで `yes` を流しながら、ページ送り・タイルのクリック・入力が我慢できる速さか(NFR-009。M4 の人間ゲート T50a。DOM と WebGL、unicode11 と graphemes を比べる)",
  "画面側の流量制御: 大量出力を長く流したとき WebKit の補助プロセス(WebContent)のメモリが増え続けないか(アクティビティモニタ。設計書 §2.2「流量制御」)。計測は 1 枚 4 MiB までしか流さない",
  "アイドル時の CPU: 全ターミナルが無出力のときの CPU(NFR-003。`top -pid` の 60 秒平均)。このスクリプトでは測らない",
];

const STATUS_LABEL = { ok: "計測した", skipped: "未計測", error: "計測できなかった" };

function fmt(v, digits) {
  if (v === null || v === undefined) return "—";
  return v.toLocaleString("en-US", { minimumFractionDigits: digits, maximumFractionDigits: digits });
}

/** Markdown に書く節の順。`matrix:` で始まるものは比べる表。 */
const SECTION_ORDER = [
  "pty_read",
  "session_ack",
  "session_stall",
  "matrix:multi_ack",
  "multi_stall",
  "attention_scan",
  "app",
  "matrix:app_tiles",
  "tree_list",
  "matrix:tiles_e2e",
];

/** 比べる表 1 つ分の Markdown の行。どの列も 0 回なら「未計測。」。 */
function renderMatrix(m) {
  const lines = ["", `## ${m.title}`, ""];
  if (m.columns.every((c) => c.n === 0)) {
    lines.push("未計測。");
    return lines;
  }
  if (m.note) lines.push(m.note, "");
  lines.push(
    `| 項目 | ${m.columns.map((c) => `${c.label}(${c.n} 回)`).join(" | ")} | 単位 | 目安・備考 |`,
    `| ---- | ${m.columns.map(() => "---:").join(" | ")} | ---- | ---------- |`,
  );
  for (const r of m.rows) {
    lines.push(`| ${r.label} | ${m.columns.map((c) => fmt(r.cells[c.key], r.digits)).join(" | ")} | ${r.unit} | ${r.guide} |`);
  }
  lines.push("", "値は各回の中央値。");
  return lines;
}

/** 結果(JSON と同じもの)から Markdown を作る。 */
export function renderMarkdown(result) {
  const e = result.environment;
  const lines = [
    `# 性能の計測(${result.started_at})`,
    "",
    "記録するだけで合否にしない(PRD の NFR-001〜005・009 は目安。C-14)。「目安・備考」の列は並べて書くだけ。",
    "",
    "## 環境",
    "",
    "| 項目 | 値 |",
    "| ---- | -- |",
    `| 日時 | ${result.started_at} |`,
    `| コミット | ${e.commit} |`,
    `| 機種 | ${e.model}(${e.chip}) |`,
    `| macOS | ${e.macos} |`,
    `| Rust | ${e.rustc} |`,
    `| Node | ${e.node} |`,
    `| ビルドの種類 | ${e.build} |`,
    `| 所要時間(全体) | ${fmt(result.duration_s, 1)} 秒 |`,
    "",
    "## 計測の状態",
    "",
    "| 計測 | 状態 | 回数 | 所要時間 | 備考 |",
    "| ---- | ---- | ---- | -------- | ---- |",
  ];
  for (const [name, m] of Object.entries(result.measures)) {
    const note = [m.note, m.error].filter(Boolean).join(" / ").replaceAll("|", "\\|").replaceAll("\n", " ");
    lines.push(`| ${name} | ${STATUS_LABEL[m.status] ?? m.status} | ${m.runs.length} | ${fmt(m.duration_s, 1)} 秒 | ${note} |`);
  }
  for (const section of SECTION_ORDER) {
    if (section.startsWith("matrix:")) {
      const m = result.matrices?.[section.slice("matrix:".length)];
      if (m !== undefined) lines.push(...renderMatrix(m));
      continue;
    }
    const rows = result.summary[section];
    if (rows === undefined) continue;
    lines.push("", `## ${SECTION_TITLES[section]}`, "");
    if (rows.every((r) => r.n === 0)) {
      lines.push("未計測。");
      continue;
    }
    lines.push("| 項目 | 中央値 | 最小 | 最大 | 単位 | 回数 | 目安・備考 |", "| ---- | -----: | ---: | ---: | ---- | ---: | ---------- |");
    for (const r of rows) {
      lines.push(`| ${r.label} | ${fmt(r.median, r.digits)} | ${fmt(r.min, r.digits)} | ${fmt(r.max, r.digits)} | ${r.unit} | ${r.n} | ${r.guide} |`);
    }
  }
  lines.push("", "## 条件", "");
  for (const c of result.conditions) lines.push(`- ${c}`);
  lines.push("", "## 人間が見る項目(自動では測らない)", "");
  for (const h of HUMAN_ITEMS) lines.push(`- [ ] ${h}`);
  lines.push("");
  return lines.join("\n");
}

// ---------------------------------------------------------------------------
// 環境と計測の本体
// ---------------------------------------------------------------------------

function capture(cmd, args) {
  const r = spawnSync(cmd, args, { cwd: REPO_ROOT, encoding: "utf8" });
  return r.status === 0 ? r.stdout.trim() : "不明";
}

/** 記録する環境。 */
export function collectEnvironment() {
  const head = capture("git", ["rev-parse", "--short=12", "HEAD"]);
  const dirty = capture("git", ["status", "--porcelain"]);
  return {
    commit: `${head}${dirty !== "" && dirty !== "不明" ? "(作業ツリーに未コミットの変更あり)" : ""}`,
    model: capture("sysctl", ["-n", "hw.model"]),
    chip: capture("sysctl", ["-n", "machdep.cpu.brand_string"]),
    macos: `${capture("sw_vers", ["-productVersion"])}(${capture("sw_vers", ["-buildVersion"])})`,
    rustc: capture("rustc", ["--version"]),
    node: process.version,
    build: "Rust の計測は cargo test --release(最適化あり)。アプリは npm run app が作った .app(tauri build の release)",
  };
}

export const CONDITIONS = [
  "計測用のファイルは 10 万行・約 7.7 MB(1 行 77 バイト)。一時フォルダに作り、計測の後で消す",
  "シェルは /bin/zsh に固定し、HOME を一時フォルダに向ける(利用者の設定ファイルを読まない)。PTY は 24 行 × 80 桁",
  "Rust の計測は各 3 回。PTY の読み取りは 1 本のシェルで 3 回続けて流す。所要時間は出力の目印(cat の前後に printf)を読み取った時刻の差",
  "送り先までの計測は製品の既定値(上限 512 KiB・下限 128 KiB、まとめ 4ms/64 KiB、読み取り 16 KiB)のまま。偽の送り先は受け取った中身を貯めない。送った回数と量は目印を含む送信の単位で数えるので、最大 1 回分ずれる",
  "未処理の量の最大値は、送り先が受け取った時点(処理済みを返す直前)の値の最大",
  "処理済みを返さない送り先では、読み取りが止まり未処理の量と受け取った量が 1 秒変わらなくなった時点を頭打ちとする",
  "アプリはスモークと同じ起動(一時の状態フォルダ、ARIN_SMOKE=1)で 3 回。状態は 5 プロジェクト・各 1 タブ(一時フォルダに作ったプロジェクトのフォルダ 5 つと、bookmark が null の state.json を置く)。印が揃ってから 3 秒置いて測る。起動の時間はファイルの印を 10ms ごとに見た値で、ディスクのキャッシュが効いた起動(コールドスタートではない)",
  "WebKit の補助プロセスは launchd の子なので、起動の前後で増えたものをこのアプリのものとみなす(同時に別のアプリが WebKit を起動すると混ざる。JSON の webkit_exited_with_app が true なら、アプリの終了と一緒に消えた)",
  "メモリは footprint の phys_footprint(アクティビティモニタの「メモリ」と同じ)。MiB は 1024×1024 バイト",
  "ツリーの読み込みは見本を一時フォルダに 2 つ作って各 3 回(1 つのフォルダに全ファイル、全ファイルを 100 ファイルずつのフォルダに分けたもの。どちらも .git・node_modules・.DS_Store を足す)。見本は計測の後で消す。ファイルの数は --tree-files(既定 10 万)",
  "2c・2d は Workspace::for_verify_grid の 9 枚(持ち主の無いタブ)で、シェルは /bin/zsh、HOME は一時フォルダ。各 3 回。`yes '<78 文字>' | head -c 4194304`(2d は 8 MiB)を 9 枚に順に書き込んで同時に流す。ベル入りは行の終わりに BEL を足したもの",
  "2a・2b の 1 枚の値の、アテンションの走査(T48c)を入れる前の記録: testreport/perf/perf-20261115-205753(2a の所要時間の中央値 39.8 ms、185.2 MiB/s、2b の頭打ち 525,184 B)",
  "3b は 3. と同じ起動で、9 プロジェクト・各 1 タブ・`ui.displayMode: \"tiles\"` の state.json。描画方式は ARIN_RENDERER で渡し、全角幅の方式は既定(unicode11)のまま。各 2 回。印が揃い、シェルが 9 個になってから 3 秒置いて測る",
  "3b の「流す」組は、一時フォルダの HOME(.zshrc だけ)をアプリに渡し、各シェルが起動直後に `yes | head -c 4194304` を流す。流し終えたシェルが HOME に書く印を 250ms ごとに見ながら、本体と WebKit の RSS(ps)の最大を取る。上限 60 秒",
  "起動の前ごとに memory_pressure のメモリの空きを記録し、50% を下回ったら残りのアプリの起動を止める",
  "5. は Vite の画面を Playwright の WebKit(ヘッドレス、1280×720)で開き、10 プロジェクト・各 1 タブのタイル表示(1 ページ目に 9 枚)の 1 ページ目の 9 枚に、偽の Rust 側が 1 枚あたり 2 MiB を同時に流す(行は ASCII と全角の混ざった 1 行)。組ごとに 2 回。流しているあいだにタイルのクリック・キー入力・次のページのボタンを 1 回ずつ行い、写しで 2 ページ目へ送って戻す",
  "ツリーの読み込みは src-tauri の list_directory と同じ手順(範囲の検査・read_dir・除外・並べ替え)を Rust の中で測った時間。作った直後に測るのでディスクのキャッシュが効いている。コマンドの呼び出し・写しの受け渡し・画面の描画は含まない(T44b)",
];

/** cargo test で計測テスト(--ignored)を動かし、`ARIN_PERF` の行を返す。 */
export function runCargoPerf(pkg, test, { log = console.log, env = {} } = {}) {
  const args = ["test", "--release", "--locked", "-p", pkg, "--test", test, "--", "--ignored", "--nocapture", "--test-threads=1"];
  log(`\n=== cargo ${args.join(" ")}`);
  const r = spawnSync("cargo", args, {
    cwd: REPO_ROOT,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "inherit"],
    maxBuffer: 64 * MIB,
    env: { ...process.env, ...env },
  });
  if (r.error) return { status: "error", runs: [], error: `cargo を起動できなかった: ${r.error.message}` };
  const runs = parsePerfLines(r.stdout ?? "");
  for (const run of runs) log(`${PERF_LINE_PREFIX}${JSON.stringify(run)}`);
  if (r.status !== 0) {
    const tail = (r.stdout ?? "").trim().split("\n").slice(-20).join("\n");
    return { status: "error", runs, error: `cargo test が失敗した(終了コード ${r.status ?? r.signal})\n${tail}` };
  }
  if (runs.length === 0) return { status: "error", runs, error: `${test} が結果の行(${PERF_LINE_PREFIX.trim()})を出さなかった` };
  return { status: "ok", runs };
}

/**
 * 画面側の 9 枚の計測(e2e/perf-tiles.spec.ts)を Playwright で動かし、spec が書いた最新の結果の runs を返す。
 * spec が失敗した・結果が無い・古い(このときより前に書かれた)ときは error。数値の良し悪しでは失敗にしない。
 */
export async function runTilesE2e({
  log = console.log,
  run = (cmd, args, opts) => spawnSync(cmd, args, opts),
  latestFile = path.join(OUT_DIR, TILES_E2E_LATEST),
  now = () => Date.now(),
} = {}) {
  const startedAt = now();
  const args = ["playwright", "test", TILES_E2E_SPEC, "--workers=1", "--reporter=list"];
  log(`\n=== npx ${args.join(" ")}(${PERF_E2E_ENV}=1)`);
  const r = run("npx", args, {
    cwd: REPO_ROOT,
    encoding: "utf8",
    stdio: ["ignore", "inherit", "inherit"],
    env: { ...process.env, [PERF_E2E_ENV]: "1" },
  });
  if (r.error) return { status: "error", runs: [], error: `Playwright を起動できなかった: ${r.error.message}` };
  if (r.status !== 0) return { status: "error", runs: [], error: `${TILES_E2E_SPEC} が失敗した(終了コード ${r.status ?? r.signal})` };
  let latest;
  try {
    latest = JSON.parse(await readFile(latestFile, "utf8"));
  } catch (e) {
    return { status: "error", runs: [], error: `${path.basename(latestFile)} を読めなかった: ${e.message}` };
  }
  const recorded = Date.parse(latest.recordedAt ?? "");
  if (!Number.isFinite(recorded) || recorded < startedAt - 1000) {
    return { status: "error", runs: [], error: `${path.basename(latestFile)} が今回の計測より古い(${latest.recordedAt})` };
  }
  const runs = (latest.runs ?? []).map((x) => ({ kind: "tiles_e2e", ...x }));
  if (runs.length === 0) return { status: "error", runs, error: `${path.basename(latestFile)} に計測の回が無い` };
  return { status: "ok", runs, note: `${latest.browser ?? "WebKit"}。${path.basename(latestFile)}` };
}

export function defaultMeasures({ appPath, treeFiles = DEFAULT_TREE_FILES } = {}) {
  return {
    pty_read: ({ log }) => runCargoPerf("arin-platform", "perf_pty", { log }),
    sessions: ({ log }) => runCargoPerf("arin", "perf_sessions", { log }),
    app: ({ log }) => {
      log("\n=== 起動中のアプリのメモリ");
      return measureAppMemory({ ...(appPath ? { appPath } : {}), log });
    },
    tiles_e2e: ({ log }) => runTilesE2e({ log }),
    tree: ({ log }) => runCargoPerf("arin-platform", "perf_tree", { log, env: { [TREE_FILES_ENV]: String(treeFiles) } }),
  };
}

const pad = (n) => String(n).padStart(2, "0");
/** ファイル名に使う日時(手元の時刻)。 */
export function stamp(d) {
  return `${d.getFullYear()}${pad(d.getMonth() + 1)}${pad(d.getDate())}-${pad(d.getHours())}${pad(d.getMinutes())}${pad(d.getSeconds())}`;
}

/** 手元の時刻の ISO 8601(時差付き)。 */
function localIso(d) {
  const off = -d.getTimezoneOffset();
  const sign = off >= 0 ? "+" : "-";
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}${sign}${pad(Math.floor(Math.abs(off) / 60))}:${pad(Math.abs(off) % 60)}`;
}

/**
 * 計測して結果を書く。measures は名前 → 計測の関数(`{ status, runs, note?, error? }` を返す)。
 * @returns {Promise<{ exitCode: number, files: string[], result: object }>}
 */
export async function runPerf({
  measures = defaultMeasures(),
  environment = collectEnvironment,
  outDir = OUT_DIR,
  now = () => new Date(),
  log = console.log,
} = {}) {
  const started = now();
  const t0 = performance.now();
  const results = {};
  for (const [name, fn] of Object.entries(measures)) {
    const m0 = performance.now();
    let r;
    try {
      r = await fn({ log });
    } catch (e) {
      r = { status: "error", runs: [], error: `計測が例外で止まった: ${e?.stack ?? e}` };
    }
    results[name] = { ...r, runs: r.runs ?? [], duration_s: (performance.now() - m0) / 1000 };
    if (r.status === "error") log(`${name}: 計測できなかった: ${r.error}`);
  }
  const result = {
    started_at: localIso(started),
    duration_s: (performance.now() - t0) / 1000,
    environment: environment(),
    measures: results,
    summary: summarize(results),
    matrices: summarizeMatrices(runsByKind(results)),
    conditions: CONDITIONS,
    human_items: HUMAN_ITEMS,
  };
  await mkdir(outDir, { recursive: true });
  const base = path.join(outDir, `perf-${stamp(started)}`);
  const json = `${JSON.stringify(result, null, 2)}\n`;
  const md = renderMarkdown(result);
  const files = [`${base}.json`, `${base}.md`, path.join(outDir, "latest.json"), path.join(outDir, "latest.md")];
  await writeFile(files[0], json);
  await writeFile(files[1], md);
  await writeFile(files[2], json);
  await writeFile(files[3], md);
  const failed = Object.entries(results).filter(([, r]) => r.status === "error").map(([n]) => n);
  return { exitCode: failed.length > 0 ? 1 : 0, files, result, failed };
}

/** コマンドラインの引数を読む。 */
export function parseArgs(argv) {
  const opts = {};
  for (const arg of argv) {
    const app = /^--app=(.+)$/.exec(arg);
    if (app !== null) {
      opts.appPath = path.resolve(app[1]);
      continue;
    }
    const tree = /^--tree-files=(.+)$/.exec(arg);
    if (tree !== null) {
      const n = /^[0-9]+$/.test(tree[1]) ? Number(tree[1]) : Number.NaN;
      if (!Number.isSafeInteger(n) || n < 100) throw new Error(`--tree-files は 100 以上の整数: ${tree[1]}`);
      opts.treeFiles = n;
      continue;
    }
    throw new Error(`知らない引数: ${arg}`);
  }
  return opts;
}

async function main() {
  let opts;
  try {
    opts = parseArgs(process.argv.slice(2));
  } catch (e) {
    console.error(`性能の計測: ${e.message}`);
    return 2;
  }
  for (const sig of ["SIGINT", "SIGTERM", "SIGHUP"]) {
    process.once(sig, () => {
      console.error(`性能の計測: ${sig} を受けたので中断する`);
      process.exit(130);
    });
  }
  const { exitCode, files, failed } = await runPerf({ measures: defaultMeasures(opts) });
  console.log(`\n結果: ${files.map((f) => path.relative(REPO_ROOT, f)).join("、")}`);
  if (exitCode !== 0) console.error(`計測できなかったもの: ${failed.join("、")}(数値の良し悪しでは失敗にしない)`);
  return exitCode;
}

if (process.argv[1] !== undefined && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().then(
    (code) => {
      process.exitCode = code;
    },
    (e) => {
      console.error(`性能の計測(予期しない例外): ${e?.stack ?? e}`);
      process.exitCode = 1;
    },
  );
}
