#!/usr/bin/env node
// 境界検査(設計書 ARCH_multi-terminal.md §3.3、NFR-008、§12)。
//
// 使い方: node scripts/check-boundaries.mjs [リポジトリのルート]
//   ルートを省略すると、このスクリプトの一つ上のディレクトリを調べる。
//   違反があれば 1 件につき 1 行を表示して終了コード 1、無ければ 0 を返す。
//
// 検査の対象は src/、crates/arin-core/、src-tauri/tauri.conf.json、Cargo.lock だけ。
// scripts/fixtures/(違反を仕込んだ見本)は対象に入らない。
//
// 外部のパッケージを使わず、Node の標準モジュールだけで書く。TOML と TypeScript は
// 本格的に構文解析せず、コメントを取り除いたうえで必要な形だけを読み取る。

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

// ---------------------------------------------------------------------------
// 規則の定数
// ---------------------------------------------------------------------------

/** arin-core が依存してよいクレート(設計書 §3.1)。 */
export const CORE_ALLOWED_DEPENDENCIES = ["serde", "serde_json", "thiserror", "ts-rs"];

/**
 * arin-core が推移的にも引き込んではならないクレート(設計書 §3.2 の禁止依存)。
 * 名前が一致するか、`<名前>-` で始まるものを禁止する。
 */
export const CORE_FORBIDDEN_TRANSITIVE = ["tauri", "objc2", "libc", "notify", "portable-pty", "pty-process"];

/**
 * 組み込んではならない通信のクレート(NFR-008、設計書 §12「HTTP クライアントと自動アップデートの部品を入れない」)。
 * 設計書が例に挙げた reqwest・hyper・ureq・isahc・surf・curl に、同じ役割のクレートを足している。
 */
export const HTTP_CLIENT_CRATES = [
  "reqwest",
  "hyper",
  "hyper-util",
  "hyper-tls",
  "hyper-rustls",
  "ureq",
  "isahc",
  "surf",
  "curl",
  "curl-sys",
  "attohttpc",
  "minreq",
  "http-req",
  "awc",
  "ehttp",
  "tungstenite",
  "tokio-tungstenite",
  "async-tungstenite",
  "tauri-plugin-http",
  "tauri-plugin-updater",
  "tauri-plugin-websocket",
  "tauri-plugin-upload",
];

/** CSP の connect-src に書いてよい値(Tauri の IPC と自アプリのアセットだけ)。 */
export const CSP_ALLOWED_CONNECT_SOURCES = [
  "'self'",
  "'none'",
  "ipc:",
  "http://ipc.localhost",
  "asset:",
  "http://asset.localhost",
];

/** CSP でスクリプト(script-src とその細目。無ければ default-src)に書いてよい値。インラインと eval は許さない。 */
export const CSP_ALLOWED_SCRIPT_SOURCES = ["'self'", "'none'"];

/**
 * CSP でスタイル(style-src・style-src-elem・style-src-attr)に書いてよい値。
 * xterm.js は実行時に <style> の要素を作り、truecolor の文字には style 属性を書くので 'unsafe-inline' が要る
 * (T18。node_modules/@xterm/xterm/lib/xterm.mjs の createElement("style") と setAttribute("style")、
 * 起動スモークで style-src-elem の違反を確認)。外部の読み込み元は許さない。
 */
export const CSP_ALLOWED_STYLE_SOURCES = ["'self'", "'none'", "'unsafe-inline'"];

const SRC_EXTENSIONS = new Set([".ts", ".tsx", ".mts", ".cts", ".js", ".jsx", ".mjs", ".cjs"]);
const TEST_FILE_PATTERN = /\.(test|spec)\.[cm]?[jt]sx?$/;

// ---------------------------------------------------------------------------
// 共通の補助
// ---------------------------------------------------------------------------

/**
 * 違反 1 件。
 * @typedef {{ rule: string, file: string, line: number, snippet: string, message: string }} Violation
 */

/** @returns {Violation} */
function violation(rule, file, line, snippet, message) {
  return { rule, file, line, snippet: snippet.trim().slice(0, 160), message };
}

const toPosix = (p) => p.split(path.sep).join("/");

function readText(file) {
  try {
    return fs.readFileSync(file, "utf8");
  } catch {
    return null;
  }
}

/** ディレクトリ配下のファイルを再帰で集める(node_modules と隠しディレクトリは見ない)。 */
function walk(dir) {
  /** @type {string[]} */
  const files = [];
  let entries;
  try {
    entries = fs.readdirSync(dir, { withFileTypes: true });
  } catch {
    return files;
  }
  for (const entry of entries) {
    if (entry.name === "node_modules" || entry.name.startsWith(".")) continue;
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) files.push(...walk(full));
    else if (entry.isFile()) files.push(full);
  }
  return files.sort();
}

/** 文字の位置から行番号(1 始まり)を引く関数を作る。 */
function lineIndex(text) {
  const starts = [0];
  for (let i = 0; i < text.length; i++) if (text[i] === "\n") starts.push(i + 1);
  return (pos) => {
    let lo = 0;
    let hi = starts.length - 1;
    while (lo < hi) {
      const mid = (lo + hi + 1) >> 1;
      if (starts[mid] <= pos) lo = mid;
      else hi = mid - 1;
    }
    return lo + 1;
  };
}

const lineText = (text, line) => text.split("\n")[line - 1] ?? "";

/** `rel` が `dir` そのものか、その配下か。 */
const isUnder = (rel, dir) => rel === dir || rel.startsWith(`${dir}/`);

// ---------------------------------------------------------------------------
// TypeScript / JavaScript のコメントの除去
// ---------------------------------------------------------------------------

const REGEX_PRECEDING_WORDS = new Set([
  "return", "typeof", "case", "do", "else", "in", "of", "new", "delete", "void", "throw", "yield", "await", "instanceof",
]);

/**
 * コメントを空白に置き換えた文字列を返す(長さと改行の位置は元のまま)。
 * 文字列・テンプレート・正規表現リテラルの中身は残す(`el["innerHTML"]` のような書き方も検査できるように)。
 * それらの中にある `//` や `/*` はコメントの始まりとして扱わない。
 */
export function stripJsComments(src) {
  const out = src.split("");
  const n = src.length;
  const blank = (a, b) => {
    for (let k = a; k < b; k++) if (out[k] !== "\n") out[k] = " ";
  };
  /** テンプレートの `${` を開いた時点の波括弧の深さ。 */
  const templateStack = [];
  let depth = 0;

  const regexAllowed = (pos) => {
    let k = pos - 1;
    while (k >= 0 && /\s/.test(out[k])) k--;
    if (k < 0) return true;
    const c = out[k];
    if (/[\w$]/.test(c)) {
      let s = k;
      while (s > 0 && /[\w$]/.test(out[s - 1])) s--;
      return REGEX_PRECEDING_WORDS.has(out.slice(s, k + 1).join(""));
    }
    return !/[)\]"'`]/.test(c);
  };

  const skipQuoted = (i, quote) => {
    let j = i + 1;
    while (j < n && src[j] !== quote && src[j] !== "\n") j += src[j] === "\\" ? 2 : 1;
    return Math.min(j + 1, n);
  };

  /** テンプレートの文字の部分を読み進める。終わり(`)か `${` の直後の位置を返す。 */
  const scanTemplate = (i) => {
    let j = i;
    while (j < n) {
      if (src[j] === "\\") j += 2;
      else if (src[j] === "`") return j + 1;
      else if (src[j] === "$" && src[j + 1] === "{") {
        templateStack.push(depth);
        return j + 2;
      } else j++;
    }
    return n;
  };

  const skipRegex = (i) => {
    let j = i + 1;
    let inClass = false;
    while (j < n && src[j] !== "\n") {
      const c = src[j];
      if (c === "\\") {
        j += 2;
        continue;
      }
      if (inClass) {
        if (c === "]") inClass = false;
      } else if (c === "[") inClass = true;
      else if (c === "/") {
        j++;
        while (j < n && /[a-z]/i.test(src[j])) j++;
        return j;
      }
      j++;
    }
    return j;
  };

  let i = 0;
  while (i < n) {
    const c = src[i];
    const d = src[i + 1];
    if (c === "/" && d === "/") {
      let j = src.indexOf("\n", i);
      if (j < 0) j = n;
      blank(i, j);
      i = j;
    } else if (c === "/" && d === "*") {
      let j = src.indexOf("*/", i + 2);
      j = j < 0 ? n : j + 2;
      blank(i, j);
      i = j;
    } else if (c === '"' || c === "'") {
      i = skipQuoted(i, c);
    } else if (c === "`") {
      i = scanTemplate(i + 1);
    } else if (c === "/" && regexAllowed(i)) {
      i = skipRegex(i);
    } else if (c === "{") {
      depth++;
      i++;
    } else if (c === "}") {
      if (templateStack.length > 0 && templateStack[templateStack.length - 1] === depth) {
        templateStack.pop();
        i = scanTemplate(i + 1);
      } else {
        depth--;
        i++;
      }
    } else {
      i++;
    }
  }
  return out.join("");
}

// ---------------------------------------------------------------------------
// Rust のコメントと文字列の除去
// ---------------------------------------------------------------------------

/**
 * Rust のコメント(入れ子のブロックコメントを含む)を空白に置き換え、文字列リテラル
 * (通常・バイト・生文字列)と文字リテラルの中身も空白に置き換える。長さと改行の位置は元のまま。
 */
export function stripRust(src) {
  const out = src.split("");
  const n = src.length;
  const blank = (a, b) => {
    for (let k = a; k < b; k++) if (out[k] !== "\n") out[k] = " ";
  };
  const isIdent = (c) => c !== undefined && /[\w]/.test(c);
  let i = 0;
  while (i < n) {
    const c = src[i];
    const d = src[i + 1];
    if (c === "/" && d === "/") {
      let j = src.indexOf("\n", i);
      if (j < 0) j = n;
      blank(i, j);
      i = j;
      continue;
    }
    if (c === "/" && d === "*") {
      let level = 0;
      let j = i;
      while (j < n) {
        if (src[j] === "/" && src[j + 1] === "*") {
          level++;
          j += 2;
        } else if (src[j] === "*" && src[j + 1] === "/") {
          level--;
          j += 2;
          if (level === 0) break;
        } else j++;
      }
      blank(i, j);
      i = j;
      continue;
    }
    // 生文字列: r"..."、r#"..."#、br#"..."#
    if (!isIdent(src[i - 1]) && (c === "r" || (c === "b" && d === "r"))) {
      const start = c === "b" ? i + 2 : i + 1;
      const m = /^(#*)"/.exec(src.slice(start, start + 260));
      if (m) {
        const close = `"${m[1]}`;
        const bodyStart = start + m[0].length;
        let j = src.indexOf(close, bodyStart);
        j = j < 0 ? n : j;
        blank(bodyStart, j);
        i = j + close.length;
        continue;
      }
    }
    if (c === '"') {
      let j = i + 1;
      while (j < n && src[j] !== '"') j += src[j] === "\\" ? 2 : 1;
      blank(i + 1, j);
      i = j + 1;
      continue;
    }
    if (c === "'") {
      // 文字リテラルか、ライフタイム('a)か
      const m = /^'(?:\\(?:x[0-9a-fA-F]{2}|u\{[0-9a-fA-F]{1,6}\}|.)|[^\\'\n])'/u.exec(src.slice(i, i + 16));
      if (m) {
        blank(i + 1, i + m[0].length - 1);
        i += m[0].length;
        continue;
      }
    }
    if (isIdent(c)) {
      // 識別子の途中の r を生文字列の始まりと見なさないよう、識別子をまとめて読み飛ばす。
      // b"..." と b'x' は、b を読み飛ばしたあと次の周回で文字列・文字として扱う
      let j = i;
      while (j < n && isIdent(src[j])) j++;
      i = j;
      continue;
    }
    i++;
  }
  return out.join("");
}

// ---------------------------------------------------------------------------
// Cargo.toml と Cargo.lock の読み取り
// ---------------------------------------------------------------------------

/** TOML の 1 行から、文字列の外にある `#` 以降を取り除く。 */
function stripTomlComment(line) {
  let quote = null;
  for (let i = 0; i < line.length; i++) {
    const c = line[i];
    if (quote) {
      if (c === "\\" && quote === '"') i++;
      else if (c === quote) quote = null;
    } else if (c === '"' || c === "'") quote = c;
    else if (c === "#") return line.slice(0, i);
  }
  return line;
}

/** `target.'cfg(unix)'.dependencies` のような表の名前を、引用符を外した部分の列にする。 */
function splitTomlKey(key) {
  const parts = [];
  const re = /\s*(?:"((?:[^"\\]|\\.)*)"|'([^']*)'|([A-Za-z0-9_-]+))\s*(?:\.|$)/gy;
  let m;
  while (re.lastIndex < key.length && (m = re.exec(key))) parts.push(m[1] ?? m[2] ?? m[3]);
  return parts;
}

const DEPENDENCY_KINDS = new Set(["dependencies", "build-dependencies", "dev-dependencies"]);

/**
 * Cargo.toml の依存を読み取る。
 * @returns {{ name: string, key: string, kind: string, line: number, text: string }[]}
 *   name は実体のクレート名(`package = "..."` で別名にしていればその値)、key は書かれた名前。
 */
export function parseCargoDependencies(text) {
  const deps = [];
  const lines = text.split("\n");
  /** 現在の表: 依存の一覧の表なら { kind }、依存 1 件の表なら { kind, dep }。 */
  let table = null;
  for (let idx = 0; idx < lines.length; idx++) {
    const raw = lines[idx];
    const line = stripTomlComment(raw).trim();
    if (line === "") continue;
    const header = /^\[([^[\]]+)\]$/.exec(line);
    if (header) {
      table = null;
      const parts = splitTomlKey(header[1]);
      let rest = parts;
      if (parts[0] === "target" && parts.length >= 3) rest = parts.slice(2);
      if (DEPENDENCY_KINDS.has(rest[0])) {
        if (rest.length === 1) table = { kind: rest[0] };
        else if (rest.length === 2) {
          const dep = { name: rest[1], key: rest[1], kind: rest[0], line: idx + 1, text: raw };
          deps.push(dep);
          table = { kind: rest[0], dep };
        }
      }
      continue;
    }
    if (line.startsWith("[[")) {
      table = null;
      continue;
    }
    if (!table) continue;
    const kv = /^(?:"([^"]+)"|'([^']+)'|([A-Za-z0-9_-]+))\s*(\.|=)(.*)$/.exec(line);
    if (!kv) continue;
    const key = kv[1] ?? kv[2] ?? kv[3];
    const value = kv[5];
    if (table.dep) {
      // 依存 1 件の表の中: package = "..." だけを見る
      const pkg = key === "package" ? /^\s*"([^"]+)"/.exec(value) : null;
      if (pkg) table.dep.name = pkg[1];
      continue;
    }
    const pkg = /\bpackage\s*=\s*"([^"]+)"/.exec(value);
    deps.push({ name: pkg ? pkg[1] : key, key, kind: table.kind, line: idx + 1, text: raw });
  }
  return deps;
}

/**
 * Cargo.lock の [[package]] を読み取る。
 * @returns {{ name: string, version: string, source: string | null, line: number, dependencies: string[] }[]}
 */
export function parseCargoLock(text) {
  const packages = [];
  const lines = text.split("\n");
  let current = null;
  let inDeps = false;
  for (let idx = 0; idx < lines.length; idx++) {
    const line = stripTomlComment(lines[idx]).trim();
    if (inDeps) {
      if (line.startsWith("]")) {
        inDeps = false;
        continue;
      }
      const m = /^"([^"]+)"/.exec(line);
      if (m) current.dependencies.push(m[1].split(" ")[0]);
      continue;
    }
    if (line === "[[package]]") {
      current = { name: "", version: "", source: null, line: idx + 1, dependencies: [] };
      packages.push(current);
      continue;
    }
    if (line.startsWith("[")) {
      // [metadata] や [[patch.unused]] など、package 以外の表
      current = null;
      continue;
    }
    if (!current) continue;
    const kv = /^([A-Za-z0-9_-]+)\s*=\s*(.*)$/.exec(line);
    if (!kv) continue;
    const [, key, value] = kv;
    const str = /^"([^"]*)"/.exec(value);
    if (key === "name" && str) {
      current.name = str[1];
      current.line = idx + 1;
    } else if (key === "version" && str) current.version = str[1];
    else if (key === "source" && str) current.source = str[1];
    else if (key === "dependencies") {
      const inline = /^\[(.*)\]\s*$/.exec(value);
      if (inline) {
        for (const m of inline[1].matchAll(/"([^"]+)"/g)) current.dependencies.push(m[1].split(" ")[0]);
      } else inDeps = true;
    }
  }
  return packages;
}

/** 名前ごとにまとめる(同じ名前で版が違うものは依存を合わせる。安全側に倒すため)。 */
function lockByName(packages) {
  const map = new Map();
  for (const pkg of packages) {
    const entry = map.get(pkg.name) ?? { name: pkg.name, line: pkg.line, dependencies: new Set() };
    for (const dep of pkg.dependencies) entry.dependencies.add(dep);
    map.set(pkg.name, entry);
  }
  return map;
}

const matchesCrate = (name, list) => list.some((item) => name === item || name.startsWith(`${item}-`));

// ---------------------------------------------------------------------------
// 1. arin-core の依存
// ---------------------------------------------------------------------------

/**
 * `crates/arin-core/Cargo.toml` の依存(dependencies・build-dependencies・target 別)が
 * 許可リストだけであること。dev-dependencies は製品に組み込まれないので対象外。
 * さらに、許可した依存から Cargo.lock をたどり、§3.2 の禁止クレートと通信のクレートが
 * 推移的に入っていないことを確かめる(設計書 §3.3 の `cargo tree -p arin-core` の代わり)。
 * @returns {Violation[]}
 */
export function checkCoreDependencies(root) {
  const rule = "core-deps";
  const tomlRel = "crates/arin-core/Cargo.toml";
  const text = readText(path.join(root, tomlRel));
  if (text === null) return [violation(rule, tomlRel, 1, "", `${tomlRel} が未作成`)];

  const violations = [];
  const seeds = [];
  for (const dep of parseCargoDependencies(text)) {
    if (dep.kind === "dev-dependencies") continue;
    if (CORE_ALLOWED_DEPENDENCIES.includes(dep.name)) {
      seeds.push(dep.name);
      continue;
    }
    const alias = dep.key !== dep.name ? `(別名 ${dep.key})` : "";
    violations.push(
      violation(
        rule,
        tomlRel,
        dep.line,
        dep.text,
        `arin-core の [${dep.kind}] に許可されていない依存 ${dep.name}${alias}。許可: ${CORE_ALLOWED_DEPENDENCIES.join("、")}`,
      ),
    );
  }

  const lockText = readText(path.join(root, "Cargo.lock"));
  if (lockText !== null) {
    const byName = lockByName(parseCargoLock(lockText));
    const forbidden = [...CORE_FORBIDDEN_TRANSITIVE, ...HTTP_CLIENT_CRATES];
    /** 名前 → たどってきた経路 */
    const via = new Map(seeds.map((s) => [s, [s]]));
    const queue = [...seeds];
    while (queue.length > 0) {
      const name = queue.shift();
      const entry = byName.get(name);
      if (!entry) continue;
      for (const dep of entry.dependencies) {
        if (via.has(dep)) continue;
        via.set(dep, [...via.get(name), dep]);
        queue.push(dep);
      }
    }
    for (const [name, route] of via) {
      if (!matchesCrate(name, forbidden)) continue;
      const entry = byName.get(name);
      violations.push(
        violation(
          rule,
          "Cargo.lock",
          entry?.line ?? 1,
          `name = "${name}"`,
          `arin-core が禁止クレート ${name} を推移的に引き込んでいる(経路: arin-core → ${route.join(" → ")})`,
        ),
      );
    }
  }
  return violations;
}

// ---------------------------------------------------------------------------
// 2. arin-core の入出力の禁止
// ---------------------------------------------------------------------------

/**
 * `crates/arin-core/src/**` の Rust のソースに std::fs・std::process・std::net が無いこと。
 * `use std::{fs, io}` のようにまとめた use、`::std::net`、`std::os::unix::net` も見つける。
 * `std::*` の glob と `std` の別名(`use std as s`)は、禁止モジュールを隠せるので違反にする。
 * @returns {Violation[]}
 */
export function checkCoreNoIo(root) {
  const rule = "core-io";
  const dirRel = "crates/arin-core/src";
  const dir = path.join(root, dirRel);
  if (!fs.existsSync(dir)) return [violation(rule, dirRel, 1, "", `${dirRel} が未作成`)];

  const violations = [];
  for (const file of walk(dir).filter((f) => f.endsWith(".rs"))) {
    const rel = toPosix(path.relative(root, file));
    const src = fs.readFileSync(file, "utf8");
    const code = stripRust(src);
    const lineOf = lineIndex(src);
    const seen = new Set();
    const report = (pos, message) => {
      const line = lineOf(pos);
      if (seen.has(line)) return;
      seen.add(line);
      violations.push(violation(rule, rel, line, lineText(src, line), message));
    };

    for (const m of code.matchAll(/(?<![\w])std\s*::\s*((?:[A-Za-z_]\w*\s*::\s*)*?)(fs|process|net)\b/g)) {
      report(m.index, `arin-core で std::${m[1].replace(/\s/g, "")}${m[2]} を使っている(入出力は arin-platform のポート実装に置く)`);
    }
    for (const m of code.matchAll(/(?<![\w])std\s*::\s*\{/g)) {
      // 対応する } までを取り出し、その中に fs・process・net があるか見る
      let level = 0;
      let j = m.index + m[0].length - 1;
      for (; j < code.length; j++) {
        if (code[j] === "{") level++;
        else if (code[j] === "}" && --level === 0) break;
      }
      const group = code.slice(m.index + m[0].length, j);
      const hit = /\b(fs|process|net)\b/.exec(group);
      if (hit) report(m.index + m[0].length + hit.index, `arin-core で std の ${hit[1]} を use している`);
    }
    for (const m of code.matchAll(/(?<![\w])std\s*::\s*\*/g)) {
      report(m.index, "arin-core で std::* を glob で取り込んでいる(std::fs などを隠せるため禁止)");
    }
    for (const m of code.matchAll(/\b(?:extern\s+crate\s+std|use\s+(?:::\s*)?std)\s+as\b/g)) {
      report(m.index, "arin-core で std に別名を付けている(std::fs などを隠せるため禁止)");
    }
  }
  return violations;
}

// ---------------------------------------------------------------------------
// 画面のソースの読み取り(3〜6、8、9)
// ---------------------------------------------------------------------------

/**
 * 画面の製品コードを集める。テストのファイル(`*.test.*`・`*.spec.*`)と
 * テストの補助(`src/test/`)は製品に入らないので除く。
 */
function collectFrontendSources(root) {
  return walk(path.join(root, "src"))
    .filter((file) => SRC_EXTENSIONS.has(path.extname(file)))
    .map((file) => ({ file, rel: toPosix(path.relative(root, file)) }))
    .filter(({ rel }) => !TEST_FILE_PATTERN.test(rel) && !isUnder(rel, "src/test"))
    .map(({ file, rel }) => {
      const src = fs.readFileSync(file, "utf8");
      return { file, rel, src, code: stripJsComments(src), lineOf: lineIndex(src) };
    });
}

/**
 * import の指定子を取り出す(静的な import / export ... from、import()、require())。
 * @returns {{ spec: string, pos: number }[]}
 */
export function extractImports(code) {
  const found = [];
  const patterns = [
    /\b(?:import|export)\b\s*(?:type\b\s*)?(?:[\w$*{}\s,]+?\bfrom\s*)?(["'])([^"'\n]+)\1/g,
    /\bimport\s*\(\s*(["'`])([^"'`\n]+)\1/g,
    /\brequire\s*\(\s*(["'`])([^"'`\n]+)\1/g,
  ];
  for (const re of patterns) {
    for (const m of code.matchAll(re)) {
      found.push({ spec: m[2], pos: m.index + m[0].lastIndexOf(m[1] + m[2]) });
    }
  }
  return found.sort((a, b) => a.pos - b.pos);
}

/**
 * 指定子が指す先を返す。相対パス(`./`・`../`)と `/` 始まり(Vite のルート基準)は
 * ルートからの相対パスに直す。それ以外はパッケージ名として返す。
 */
function resolveImport(root, file, spec) {
  if (spec.startsWith(".")) return { path: toPosix(path.relative(root, path.resolve(path.dirname(file), spec))) };
  if (spec.startsWith("/")) return { path: toPosix(path.relative(root, path.join(root, spec))) };
  return { package: spec };
}

/** import を 1 件ずつ判定する。`judge` が文字列を返したら、それを違反の説明にする。 */
function checkImports(root, rule, judge) {
  const violations = [];
  for (const source of collectFrontendSources(root)) {
    for (const { spec, pos } of extractImports(source.code)) {
      const message = judge(source.rel, spec, resolveImport(root, source.file, spec));
      if (!message) continue;
      const line = source.lineOf(pos);
      violations.push(violation(rule, source.rel, line, lineText(source.src, line), message));
    }
  }
  return violations;
}

// ---------------------------------------------------------------------------
// 3〜6. import の境界
// ---------------------------------------------------------------------------

const isXtermPackage = (name) => name.startsWith("@xterm/") || name === "xterm" || name.startsWith("xterm-addon-");

/** 3. `@xterm/*`(旧名の `xterm`・`xterm-addon-*` を含む)を import してよいのは src/terminal/xterm だけ。 */
export function checkXtermImports(root) {
  return checkImports(root, "xterm-import", (rel, spec, target) =>
    target.package && isXtermPackage(target.package) && !isUnder(rel, "src/terminal/xterm")
      ? `${spec} を import してよいのは src/terminal/xterm/ だけ`
      : null,
  );
}

/**
 * CodeMirror の部品のパッケージか(ADR-015 の 1)。`codemirror`・`@codemirror/*`・`@lezer/*`・`@replit/*`。
 * `@replit/*` は Vim の部品(`@replit/codemirror-vim`)のためにまとめて塞ぐ。
 */
const isCodeMirrorPackage = (name) =>
  name === "codemirror" ||
  name.startsWith("codemirror/") ||
  name.startsWith("@codemirror/") ||
  name.startsWith("@lezer/") ||
  name.startsWith("@replit/");

/** 3b. CodeMirror の部品を import してよいのは src/editor/codemirror だけ(ADR-015 の 1)。 */
export function checkCodeMirrorImports(root) {
  return checkImports(root, "codemirror-import", (rel, spec, target) =>
    target.package && isCodeMirrorPackage(target.package) && !isUnder(rel, "src/editor/codemirror")
      ? `${spec} を import してよいのは src/editor/codemirror/ だけ`
      : null,
  );
}

/** Mermaid のパッケージか(ADR-017)。`mermaid` とそのサブパス、`@mermaid-js/*`。 */
const isMermaidPackage = (name) => name === "mermaid" || name.startsWith("mermaid/") || name.startsWith("@mermaid-js/");

/** 3c. Mermaid を import してよいのは src/editor/mermaid だけ(ADR-017)。 */
export function checkMermaidImports(root) {
  return checkImports(root, "mermaid-import", (rel, spec, target) =>
    target.package && isMermaidPackage(target.package) && !isUnder(rel, "src/editor/mermaid")
      ? `${spec} を import してよいのは src/editor/mermaid/ だけ`
      : null,
  );
}

/** 4. `@tauri-apps/*`(`@tauri-apps/api` とそのサブパス、プラグイン)を import してよいのは src/ipc だけ。 */
export function checkTauriApiImports(root) {
  return checkImports(root, "tauri-api-import", (rel, spec, target) =>
    target.package?.startsWith("@tauri-apps/") && !isUnder(rel, "src/ipc")
      ? `${spec} を import してよいのは src/ipc/ だけ`
      : null,
  );
}

/** 5. src/ui が src/ipc・src/terminal/xterm・src/editor/codemirror・src/editor/mermaid を直接 import しないこと。 */
export function checkUiImports(root) {
  return checkImports(root, "ui-import", (rel, spec, target) => {
    if (!isUnder(rel, "src/ui") || !target.path) return null;
    for (const dir of ["src/ipc", "src/terminal/xterm", "src/editor/codemirror", "src/editor/mermaid"]) {
      if (isUnder(target.path, dir)) return `src/ui から ${dir}/ を直接 import している(${spec})`;
    }
    return null;
  });
}

/** 6. src/terminal/xterm が src/ipc を import しないこと。 */
export function checkXtermNoIpc(root) {
  return checkImports(root, "xterm-ipc", (rel, spec, target) =>
    isUnder(rel, "src/terminal/xterm") && target.path && isUnder(target.path, "src/ipc")
      ? `src/terminal/xterm から src/ipc/ を import している(${spec})`
      : null,
  );
}

// ---------------------------------------------------------------------------
// 7. HTTP クライアントのクレート
// ---------------------------------------------------------------------------

/**
 * 数えない依存の辺。Cargo.lock はすべての target の依存を載せるので、macOS(aarch64-apple-darwin)では
 * 組み込まれないものまで入っている。そうした辺だけを、理由を添えてここに書く。
 * 足すときは `cargo tree --locked -i <クレート> --target aarch64-apple-darwin` が何も出さないことを確かめる。
 */
export const HTTP_CLIENT_EDGE_EXCEPTIONS = [
  {
    from: "tauri",
    to: "reqwest",
    reason:
      "tauri 2.12 は android と iOS(apple で macos でないもの)のときだけ reqwest に依存する。" +
      "aarch64-apple-darwin では組み込まれない(2026-11-14 に tauri 2.12.5 の Cargo.toml と cargo tree で確認)",
  },
];

/**
 * ワークスペースのメンバー(Cargo.lock で source の無い package)から依存をたどり、
 * 通信のクレートに届かないこと。直接の依存だけでなく、Tauri などが推移的に引き込んだものも
 * 違反にする(NFR-008 は「アプリが外部通信をしない」なので、誰が入れたかによらず問題にする)。
 * ただし HTTP_CLIENT_EDGE_EXCEPTIONS の辺はたどらない。違反には引き込んだクレートを添える。
 * @param {string} root
 * @param {{ edgeExceptions?: { from: string, to: string }[] }} [options]
 * @returns {Violation[]}
 */
export function checkNoHttpClientCrates(root, { edgeExceptions = HTTP_CLIENT_EDGE_EXCEPTIONS } = {}) {
  const rule = "http-client-crate";
  const text = readText(path.join(root, "Cargo.lock"));
  if (text === null) return [violation(rule, "Cargo.lock", 1, "", "Cargo.lock が未作成")];

  const packages = parseCargoLock(text);
  const byName = lockByName(packages);
  const skipped = new Set(edgeExceptions.map((e) => `${e.from}\u0000${e.to}`));
  /** 届いたクレート → それを引き込んだ(届いた)クレート */
  const dependentsOf = new Map();
  const reached = new Set(packages.filter((p) => p.source === null).map((p) => p.name));
  const queue = [...reached];
  while (queue.length > 0) {
    const name = queue.shift();
    for (const dep of byName.get(name)?.dependencies ?? []) {
      if (skipped.has(`${name}\u0000${dep}`)) continue;
      if (!dependentsOf.has(dep)) dependentsOf.set(dep, new Set());
      dependentsOf.get(dep).add(name);
      if (!reached.has(dep)) {
        reached.add(dep);
        queue.push(dep);
      }
    }
  }

  const violations = [];
  for (const pkg of packages) {
    if (!HTTP_CLIENT_CRATES.includes(pkg.name) || !reached.has(pkg.name)) continue;
    const dependents = [...(dependentsOf.get(pkg.name) ?? [])].sort();
    const by = dependents.length > 0 ? dependents.join("、") : "なし(ワークスペースのメンバー自身)";
    violations.push(
      violation(
        rule,
        "Cargo.lock",
        pkg.line,
        `name = "${pkg.name}"`,
        `通信のクレート ${pkg.name} ${pkg.version} が組み込まれている(引き込んだクレート: ${by})`,
      ),
    );
  }
  return violations;
}

// ---------------------------------------------------------------------------
// 8、9. 画面のソースの禁止語
// ---------------------------------------------------------------------------

/** コメントを除いたソースに禁止語の正規表現が現れたら違反にする(同じ行・同じ語は 1 件)。 */
function checkForbiddenPatterns(root, rule, patterns) {
  const violations = [];
  for (const source of collectFrontendSources(root)) {
    const seen = new Set();
    for (const [label, re] of patterns) {
      for (const m of source.code.matchAll(re)) {
        const line = source.lineOf(m.index);
        const key = `${line}:${label}`;
        if (seen.has(key)) continue;
        seen.add(key);
        violations.push(violation(rule, source.rel, line, lineText(source.src, line), `画面のソースで ${label} を使っている`));
      }
    }
  }
  return violations.sort((a, b) => a.file.localeCompare(b.file) || a.line - b.line);
}

/** 8. 画面のソースに fetch(・XMLHttpRequest・WebSocket・EventSource・sendBeacon が無いこと(NFR-008)。 */
export function checkNoNetworkApi(root) {
  return checkForbiddenPatterns(root, "network-api", [
    ["fetch(", /\bfetch\s*\(/g],
    ["XMLHttpRequest", /\bXMLHttpRequest\b/g],
    ["WebSocket", /\bWebSocket\b/g],
    ["EventSource", /\bEventSource\b/g],
    ["sendBeacon", /\bsendBeacon\b/g],
  ]);
}

/**
 * 8b. Rust のソースで外へ通信できるのは、新しい版の確認(`crates/arin-platform/src/release/`)だけ(ADR-018。NFR-008 の例外)。
 * `curl`・`wget` の起動、NSURLSession、std::net のソケットを、ほかの場所に書かない。コメントの行は見ない。
 * @returns {Violation[]}
 */
export function checkNetworkOnlyInRelease(root) {
  const rule = "network-process";
  const allowed = "crates/arin-platform/src/release/";
  // コマンドの名前は文字列に書くので、コメントの行だけを除いた元の文で見る
  const commands = [
    ["curl", /\bcurl\b/],
    ["wget", /\bwget\b/],
  ];
  // 型と関数は、コメントと文字列を除いた文で見る
  const apis = [
    ["NSURLSession", /\bNSURLSession\b/],
    ["std::net", /\bstd\s*::\s*net\b|\bTcpStream\b|\bUdpSocket\b/],
  ];
  const violations = [];
  const dirs = ["crates", "src-tauri/src"].map((d) => path.join(root, d)).filter((d) => fs.existsSync(d));
  for (const dir of dirs) {
    for (const file of walk(dir).filter((f) => f.endsWith(".rs"))) {
      const rel = toPosix(path.relative(root, file));
      if (rel.startsWith(allowed) || /\/tests\//.test(rel) || rel.includes("/target/")) continue;
      const src = fs.readFileSync(file, "utf8");
      const lines = src.split("\n");
      const codeLines = stripRust(src).split("\n");
      lines.forEach((text, i) => {
        const hit =
          (/^\s*(\/\/|\*|\/\*)/.test(text) ? undefined : commands.find(([, re]) => re.test(text))) ??
          apis.find(([, re]) => re.test(codeLines[i] ?? ""));
        if (hit !== undefined) {
          violations.push(
            violation(rule, rel, i + 1, text.trim(), `${hit[0]} は ${allowed} だけで使う(新しい版の確認のほかに外へ通信しない。NFR-008・ADR-018)`),
          );
        }
      });
    }
  }
  return violations;
}

/** 9. 画面のソースに innerHTML・outerHTML・insertAdjacentHTML・document.write・dangerouslySetInnerHTML が無いこと(§12)。 */
export function checkNoHtmlInjection(root) {
  return checkForbiddenPatterns(root, "html-injection", [
    ["innerHTML", /\binnerHTML\b/g],
    ["outerHTML", /\bouterHTML\b/g],
    ["insertAdjacentHTML", /\binsertAdjacentHTML\b/g],
    ["document.write", /\bdocument\s*\.\s*write(?:ln)?\b/g],
    ["dangerouslySetInnerHTML", /\bdangerouslySetInnerHTML\b/g],
  ]);
}

// ---------------------------------------------------------------------------
// 10. CSP の connect-src
// ---------------------------------------------------------------------------

/** CSP(文字列か、指示名 → 値の表)を、指示名 → 値の配列にする。 */
function parseCsp(csp) {
  const directives = new Map();
  if (typeof csp === "string") {
    for (const part of csp.split(";")) {
      const [name, ...values] = part.trim().split(/\s+/);
      if (name) directives.set(name.toLowerCase(), values);
    }
  } else if (csp && typeof csp === "object") {
    for (const [name, value] of Object.entries(csp)) {
      const values = Array.isArray(value) ? value.flatMap((v) => String(v).split(/\s+/)) : String(value).split(/\s+/);
      directives.set(name.toLowerCase(), values.filter((v) => v !== ""));
    }
  }
  return directives;
}

/**
 * `src-tauri/tauri.conf.json` の `app.security.csp` に connect-src があり、
 * その値が IPC と自アプリのアセット(CSP_ALLOWED_CONNECT_SOURCES)だけであること。
 * 開発時だけの `devCsp` は対象外(開発サーバーへの接続が要るため)。
 * @returns {Violation[]}
 */
export function checkCspConnectSrc(root) {
  const rule = "csp-connect-src";
  const rel = "src-tauri/tauri.conf.json";
  const text = readText(path.join(root, rel));
  if (text === null) return [violation(rule, rel, 1, "", `${rel} が未作成(CSP を確かめられない)`)];

  let conf;
  try {
    conf = JSON.parse(text);
  } catch (error) {
    return [violation(rule, rel, 1, "", `${rel} を JSON として読めない: ${error.message}`)];
  }
  const lines = text.split("\n");
  const findLine = (needle) => {
    const idx = lines.findIndex((l) => l.includes(needle));
    return idx < 0 ? 1 : idx + 1;
  };
  const csp = conf?.app?.security?.csp;
  if (csp === undefined || csp === null) {
    return [violation(rule, rel, findLine('"security"'), "", "app.security.csp が無い(connect-src で外部通信を塞げない)")];
  }
  const connect = parseCsp(csp).get("connect-src");
  const line = findLine("connect-src") === 1 ? findLine('"csp"') : findLine("connect-src");
  if (!connect) {
    return [violation(rule, rel, line, lineText(text, line), "CSP に connect-src が無い(default-src に任せず明示する)")];
  }
  return connect
    .filter((source) => !CSP_ALLOWED_CONNECT_SOURCES.includes(source))
    .map((source) =>
      violation(
        rule,
        rel,
        line,
        lineText(text, line),
        `connect-src が ${source} を許している(許可: ${CSP_ALLOWED_CONNECT_SOURCES.join(" ")})`,
      ),
    );
}

// ---------------------------------------------------------------------------
// 11. CSP のスクリプトとスタイル
// ---------------------------------------------------------------------------

const SCRIPT_DIRECTIVES = ["script-src", "script-src-elem", "script-src-attr"];
const STYLE_DIRECTIVES = ["style-src", "style-src-elem", "style-src-attr"];

/**
 * `app.security.csp` と(あれば)開発時の `devCsp` で、スクリプトが自アプリのものだけに絞られ
 * (script-src。無ければ default-src が効く)、スタイルが自アプリとインライン(xterm.js が要る)だけであること。
 * ファイルや csp が無いときは 10 の検査が報告するので、ここでは何も言わない。
 * @returns {Violation[]}
 */
export function checkCspScriptStyle(root) {
  const rule = "csp-script-style";
  const rel = "src-tauri/tauri.conf.json";
  const text = readText(path.join(root, rel));
  if (text === null) return [];
  let conf;
  try {
    conf = JSON.parse(text);
  } catch {
    return [];
  }
  const security = conf?.app?.security ?? {};
  const findLine = (needle) => {
    const idx = text.split("\n").findIndex((l) => l.includes(needle));
    return idx < 0 ? 1 : idx + 1;
  };

  const violations = [];
  for (const key of ["csp", "devCsp"]) {
    const csp = security[key];
    if (csp === undefined || csp === null) continue;
    const directives = parseCsp(csp);
    const report = (directive, source, allowed, what) => {
      const line = findLine(`"${key}"`);
      violations.push(
        violation(
          rule,
          rel,
          line,
          lineText(text, line),
          `${key} の ${directive} が ${source} を許している(${what}。許可: ${allowed.join(" ")})`,
        ),
      );
    };

    const scriptDirectives = SCRIPT_DIRECTIVES.filter((d) => directives.has(d));
    if (!directives.has("script-src")) scriptDirectives.push("default-src");
    for (const directive of scriptDirectives) {
      for (const source of directives.get(directive) ?? []) {
        if (!CSP_ALLOWED_SCRIPT_SOURCES.includes(source)) {
          report(directive, source, CSP_ALLOWED_SCRIPT_SOURCES, "スクリプトは自アプリのものだけにする");
        }
      }
    }
    for (const directive of STYLE_DIRECTIVES) {
      for (const source of directives.get(directive) ?? []) {
        if (!CSP_ALLOWED_STYLE_SOURCES.includes(source)) {
          report(directive, source, CSP_ALLOWED_STYLE_SOURCES, "スタイルは自アプリとインラインだけにする");
        }
      }
    }
  }
  return violations;
}

/** 6b. src/editor/codemirror が src/ipc を import しないこと(ADR-015 の 1)。 */
export function checkCodeMirrorNoIpc(root) {
  return checkImports(root, "codemirror-ipc", (rel, spec, target) =>
    isUnder(rel, "src/editor/codemirror") && target.path && isUnder(target.path, "src/ipc")
      ? `src/editor/codemirror から src/ipc/ を import している(${spec})`
      : null,
  );
}

/** 6c. src/editor/mermaid が src/ipc を import しないこと(ADR-017)。 */
export function checkMermaidNoIpc(root) {
  return checkImports(root, "mermaid-ipc", (rel, spec, target) =>
    isUnder(rel, "src/editor/mermaid") && target.path && isUnder(target.path, "src/ipc")
      ? `src/editor/mermaid から src/ipc/ を import している(${spec})`
      : null,
  );
}

// ---------------------------------------------------------------------------
// まとめて実行
// ---------------------------------------------------------------------------

/** 全検査項目(CLI ではこの順に走らせる)。 */
export const CHECKS = [
  { id: "core-deps", title: "arin-core の依存が許可リストだけ", run: checkCoreDependencies },
  { id: "core-io", title: "arin-core に std::fs・std::process・std::net が無い", run: checkCoreNoIo },
  { id: "xterm-import", title: "@xterm/* は src/terminal/xterm だけ", run: checkXtermImports },
  {
    id: "codemirror-import",
    title: "codemirror・@codemirror/*・@lezer/*・@replit/* は src/editor/codemirror だけ",
    run: checkCodeMirrorImports,
  },
  { id: "mermaid-import", title: "mermaid・@mermaid-js/* は src/editor/mermaid だけ", run: checkMermaidImports },
  { id: "tauri-api-import", title: "@tauri-apps/api は src/ipc だけ", run: checkTauriApiImports },
  {
    id: "ui-import",
    title: "src/ui が src/ipc・src/terminal/xterm・src/editor/codemirror・src/editor/mermaid を import しない",
    run: checkUiImports,
  },
  { id: "xterm-ipc", title: "src/terminal/xterm が src/ipc を import しない", run: checkXtermNoIpc },
  { id: "codemirror-ipc", title: "src/editor/codemirror が src/ipc を import しない", run: checkCodeMirrorNoIpc },
  { id: "mermaid-ipc", title: "src/editor/mermaid が src/ipc を import しない", run: checkMermaidNoIpc },
  { id: "http-client-crate", title: "HTTP クライアントのクレートが無い", run: checkNoHttpClientCrates },
  { id: "network-api", title: "画面のソースに通信の API が無い", run: checkNoNetworkApi },
  { id: "network-process", title: "Rust の通信は新しい版の確認だけ", run: checkNetworkOnlyInRelease },
  { id: "html-injection", title: "画面のソースに HTML の差し込みが無い", run: checkNoHtmlInjection },
  { id: "csp-connect-src", title: "CSP の connect-src が制限されている", run: checkCspConnectSrc },
  { id: "csp-script-style", title: "CSP のスクリプトが自アプリだけ、スタイルが自アプリとインラインだけ", run: checkCspScriptStyle },
];

/** @returns {Violation[]} */
export function runAllChecks(root) {
  return CHECKS.flatMap((check) => check.run(root));
}

/** 違反 1 件を 1 行にする: 規則名、ファイル:行、説明、該当箇所。 */
export function formatViolation(v) {
  const snippet = v.snippet ? `  該当: ${v.snippet}` : "";
  return `[${v.rule}] ${v.file}:${v.line}  ${v.message}${snippet}`.replace(/[\r\n]+/g, " ");
}

/** CLI の入口。終了コードを返す。 */
export function main(argv = process.argv.slice(2), log = console.log) {
  const root = path.resolve(argv[0] ?? path.join(path.dirname(fileURLToPath(import.meta.url)), ".."));
  if (!fs.existsSync(root) || !fs.statSync(root).isDirectory()) {
    log(`境界検査: ルート ${root} がディレクトリではない`);
    return 1;
  }
  const violations = [];
  for (const check of CHECKS) {
    const found = check.run(root);
    violations.push(...found);
    log(`${found.length === 0 ? "OK  " : "NG  "} ${check.id}: ${check.title}${found.length > 0 ? `(${found.length} 件)` : ""}`);
    if (check.id === "http-client-crate") {
      for (const e of HTTP_CLIENT_EDGE_EXCEPTIONS) log(`       数えない辺: ${e.from} → ${e.to}(${e.reason})`);
    }
  }
  for (const v of violations) log(formatViolation(v));
  log(violations.length === 0 ? "境界検査: 違反なし" : `境界検査: 違反 ${violations.length} 件`);
  return violations.length === 0 ? 0 : 1;
}

const invokedDirectly = (() => {
  if (!process.argv[1]) return false;
  try {
    return fs.realpathSync(process.argv[1]) === fs.realpathSync(fileURLToPath(import.meta.url));
  } catch {
    return false;
  }
})();

if (invokedDirectly) process.exitCode = main();
