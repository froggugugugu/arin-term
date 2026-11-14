#!/usr/bin/env node
// カバレッジ付きでテストを走らせ、基準(行のカバレッジ 80%)を対象ごとに判定する。
//
// 基準をかける対象(ADR-006、設計書 §15.1 #8。合算では判定しない):
//   - TypeScript: src/logic/**(テストファイルを除く)
//   - Rust: arin-core(crates/arin-core/src/**)
// 計測して記録するだけの対象: それ以外の src/**、arin-platform、src-tauri(クレート arin)。
//
// 指標は「行」。Rust の分岐カバレッジは nightly でしか取れず(安定版 1.98.1 では不可)、
// TypeScript と Rust で同じ指標を揃えられるのが行だけのため。関数と(Rust は)領域・
// (TypeScript は)分岐も記録するが、判定には使わない。
//
// 計測対象のファイルが 0 件、または行が 0 行のときは「満たした」とせず失敗にする
// (基準をかけない対象では警告だけを出す)。
//
// 出力:
//   testreport/coverage/ts/    Vitest(HTML、json-summary、lcov)
//   testreport/coverage/rust/  cargo-llvm-cov(HTML、JSON の要約、lcov)
//   testreport/coverage/summary.json と summary.md  対象ごとの数値と合否

import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const OUT_DIR = path.join(ROOT, "testreport", "coverage");
const TS_DIR = path.join(OUT_DIR, "ts");
const RUST_DIR = path.join(OUT_DIR, "rust");

/** 基準の値(%)。判定は「行」にかける。 */
const THRESHOLD_LINES = 80;

/**
 * 対象の定義。`match` はリポジトリ直下からの相対パス(区切りは `/`)を受け取る。
 * `gated: true` のものだけが合否に効く。
 */
const TS_TARGETS = [
  { name: "src/logic", gated: true, match: (p) => p.startsWith("src/logic/") },
  { name: "src(その他)", gated: false, match: (p) => p.startsWith("src/") && !p.startsWith("src/logic/") },
];
const RUST_TARGETS = [
  { name: "arin-core", gated: true, match: (p) => p.startsWith("crates/arin-core/src/") },
  { name: "arin-platform", gated: false, match: (p) => p.startsWith("crates/arin-platform/src/") },
  { name: "arin(src-tauri)", gated: false, match: (p) => p.startsWith("src-tauri/src/") },
];

function run(label, command, args) {
  console.log(`\n=== ${label}: ${command} ${args.join(" ")}`);
  const result = spawnSync(command, args, { cwd: ROOT, stdio: "inherit" });
  if (result.error) {
    console.error(`${label} を起動できなかった: ${result.error.message}`);
    return false;
  }
  if (result.status !== 0) {
    console.error(`${label} が失敗した(終了コード ${result.status ?? `シグナル ${result.signal}`})`);
    return false;
  }
  return true;
}

function toRelative(file) {
  return path.relative(ROOT, file).split(path.sep).join("/");
}

function emptyCounts() {
  return { files: 0, lines: { covered: 0, total: 0 }, functions: { covered: 0, total: 0 }, branches: null, regions: null };
}

function addCount(acc, key, covered, total) {
  if (acc[key] === null) acc[key] = { covered: 0, total: 0 };
  acc[key].covered += covered;
  acc[key].total += total;
}

function percent(c) {
  return c && c.total > 0 ? (c.covered / c.total) * 100 : null;
}

/** 集計から合否を決める。 */
function judge(target, counts, problems) {
  const linePct = percent(counts.lines);
  let status;
  let note = "";
  if (counts.files === 0 || counts.lines.total === 0) {
    status = target.gated ? "失敗" : "警告";
    note = counts.files === 0 ? "計測対象のファイルが 0 件" : "計測対象の行が 0 行";
  } else if (!target.gated) {
    status = "記録のみ";
  } else {
    status = linePct >= THRESHOLD_LINES ? "合格" : "失敗";
    if (status === "失敗") note = `行 ${linePct.toFixed(2)}% < ${THRESHOLD_LINES}%`;
  }
  if (status === "失敗") problems.push(`${target.name}: ${note}`);
  return { ...counts, linePercent: linePct, status, note };
}

/** 計測そのものができなかったときの行。理由は呼び出し側が problems に積む。 */
function unmeasured(targets, note) {
  return targets.map((t) => ({ target: t.name, gated: t.gated, ...emptyCounts(), linePercent: null, status: t.gated ? "失敗" : "警告", note }));
}

// ---------- TypeScript ----------

function runTypeScript(problems) {
  const vitest = path.join(ROOT, "node_modules", "vitest", "vitest.mjs");
  const ok = run("TypeScript", process.execPath, [vitest, "run", "--coverage.enabled=true", "--coverage.clean=true"]);
  if (!ok) problems.push("TypeScript: Vitest が失敗した");

  const summaryPath = path.join(TS_DIR, "coverage-summary.json");
  if (!existsSync(summaryPath)) {
    problems.push(`TypeScript: 計測結果が無い(${toRelative(summaryPath)})`);
    return unmeasured(TS_TARGETS, "計測結果が無い");
  }
  const summary = JSON.parse(readFileSync(summaryPath, "utf8"));
  return TS_TARGETS.map((t) => {
    const counts = emptyCounts();
    for (const [file, s] of Object.entries(summary)) {
      if (file === "total") continue;
      if (!t.match(toRelative(file))) continue;
      counts.files += 1;
      counts.lines.covered += s.lines.covered;
      counts.lines.total += s.lines.total;
      counts.functions.covered += s.functions.covered;
      counts.functions.total += s.functions.total;
      addCount(counts, "branches", s.branches.covered, s.branches.total);
    }
    return { target: t.name, gated: t.gated, ...judge(t, counts, problems) };
  });
}

// ---------- Rust ----------

function runRust(problems) {
  mkdirSync(RUST_DIR, { recursive: true });
  const jsonPath = path.join(RUST_DIR, "coverage-summary.json");
  // 前回の計測の残りで判定しないよう、計測の生データを消してからテストを走らせる。
  const steps = [
    ["Rust(前回の計測を消す)", ["llvm-cov", "clean", "--workspace"]],
    ["Rust(テスト)", ["llvm-cov", "--workspace", "--locked", "--no-report"]],
    ["Rust(HTML)", ["llvm-cov", "report", "--html", "--output-dir", RUST_DIR]],
    ["Rust(lcov)", ["llvm-cov", "report", "--lcov", "--output-path", path.join(RUST_DIR, "lcov.info")]],
    ["Rust(JSON)", ["llvm-cov", "report", "--json", "--summary-only", "--output-path", jsonPath]],
  ];
  for (const [label, args] of steps) {
    if (!run(label, "cargo", args)) {
      problems.push(`${label} が失敗した`);
      return unmeasured(RUST_TARGETS, "計測できなかった");
    }
  }
  const report = JSON.parse(readFileSync(jsonPath, "utf8"));
  const files = report.data?.[0]?.files ?? [];
  return RUST_TARGETS.map((t) => {
    const counts = emptyCounts();
    for (const f of files) {
      if (!t.match(toRelative(f.filename))) continue;
      const s = f.summary;
      counts.files += 1;
      counts.lines.covered += s.lines.covered;
      counts.lines.total += s.lines.count;
      counts.functions.covered += s.functions.covered;
      counts.functions.total += s.functions.count;
      addCount(counts, "regions", s.regions.covered, s.regions.count);
    }
    return { target: t.name, gated: t.gated, ...judge(t, counts, problems) };
  });
}

// ---------- まとめ ----------

function fmt(c) {
  const p = percent(c);
  return p === null ? "-" : `${p.toFixed(2)}% (${c.covered}/${c.total})`;
}

function renderTable(rows) {
  const header = ["言語", "対象", "基準", "ファイル数", "行", "関数", "分岐/領域", "判定", "備考"];
  const lines = [`| ${header.join(" | ")} |`, `| ${header.map(() => "---").join(" | ")} |`];
  for (const r of rows) {
    lines.push(
      `| ${[
        r.language,
        r.target,
        r.gated ? `行 ${THRESHOLD_LINES}%` : "なし",
        String(r.files),
        fmt(r.lines),
        fmt(r.functions),
        r.branches ? `分岐 ${fmt(r.branches)}` : r.regions ? `領域 ${fmt(r.regions)}` : "-",
        r.status,
        r.note || "",
      ].join(" | ")} |`,
    );
  }
  return lines.join("\n");
}

function main() {
  mkdirSync(OUT_DIR, { recursive: true });
  const problems = [];
  const rows = [
    ...runTypeScript(problems).map((r) => ({ language: "TypeScript", ...r })),
    ...runRust(problems).map((r) => ({ language: "Rust", ...r })),
  ];
  const passed = problems.length === 0;
  const table = renderTable(rows);
  const warnings = rows.filter((r) => r.status === "警告").map((r) => `${r.target}: ${r.note}`);

  const md = [
    "# カバレッジの結果",
    "",
    `- 日時: ${new Date().toISOString()}`,
    `- 基準: 行のカバレッジ ${THRESHOLD_LINES}% 以上を src/logic と arin-core のそれぞれにかける(合算しない)`,
    `- 総合: ${passed ? "合格" : "不合格"}`,
    "",
    table,
    "",
    ...(problems.length ? ["## 不合格の理由", "", ...problems.map((p) => `- ${p}`), ""] : []),
    ...(warnings.length ? ["## 警告", "", ...warnings.map((w) => `- ${w}`), ""] : []),
    "詳細: `testreport/coverage/ts/index.html`、`testreport/coverage/rust/html/index.html`",
    "",
  ].join("\n");
  writeFileSync(path.join(OUT_DIR, "summary.md"), md);
  writeFileSync(
    path.join(OUT_DIR, "summary.json"),
    `${JSON.stringify({ generatedAt: new Date().toISOString(), metric: "lines", threshold: THRESHOLD_LINES, passed, problems, warnings, targets: rows }, null, 2)}\n`,
  );

  console.log(`\n=== カバレッジのまとめ(行 ${THRESHOLD_LINES}% を src/logic と arin-core にかける)\n`);
  console.log(table);
  for (const w of warnings) console.log(`警告: ${w}`);
  for (const p of problems) console.error(`不合格: ${p}`);
  console.log(`\n総合: ${passed ? "合格" : "不合格"}(${toRelative(path.join(OUT_DIR, "summary.md"))})`);
  process.exitCode = passed ? 0 : 1;
}

main();
