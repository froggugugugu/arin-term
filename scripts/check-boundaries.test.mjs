// scripts/check-boundaries.mjs の単体テスト。`node --test scripts/` で動かす。
// 見本(scripts/fixtures/<名前>/)はそれぞれ小さなリポジトリのルートとして扱う。

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import path from "node:path";
import { describe, it } from "node:test";
import { fileURLToPath } from "node:url";

import {
  CHECKS,
  checkCoreDependencies,
  checkCoreNoIo,
  checkCspConnectSrc,
  checkCspScriptStyle,
  checkNoHtmlInjection,
  checkNoHttpClientCrates,
  checkNoNetworkApi,
  checkNetworkOnlyInRelease,
  checkCodeMirrorImports,
  checkCodeMirrorNoIpc,
  checkTauriApiImports,
  checkUiImports,
  checkXtermImports,
  checkXtermNoIpc,
  formatViolation,
  runAllChecks,
} from "./check-boundaries.mjs";

const here = path.dirname(fileURLToPath(import.meta.url));
const fixture = (name) => path.join(here, "fixtures", name);
const script = path.join(here, "check-boundaries.mjs");

/** 違反を「ファイル:行」の一覧にして、比べやすくする。 */
const places = (violations) => violations.map((v) => `${v.file}:${v.line}`).sort();

describe("違反の無い見本", () => {
  it("全項目が通る", () => {
    const violations = runAllChecks(fixture("clean"));
    assert.deepEqual(violations.map(formatViolation), []);
  });

  it("14 項目すべてを走らせる", () => {
    assert.equal(CHECKS.length, 14);
  });

  it("CLI として実行すると終了コード 0 を返す", () => {
    const result = spawnSync(process.execPath, [script, fixture("clean")], { encoding: "utf8" });
    assert.equal(result.status, 0, result.stdout + result.stderr);
  });
});

describe("1. arin-core の依存", () => {
  it("許可リスト外の依存(表の形、別名、target 別、build-dependencies)を見つける", () => {
    const violations = checkCoreDependencies(fixture("core-dep"));
    const fromToml = violations.filter((v) => v.file === "crates/arin-core/Cargo.toml");
    assert.deepEqual(places(fromToml), [
      "crates/arin-core/Cargo.toml:11",
      "crates/arin-core/Cargo.toml:15",
      "crates/arin-core/Cargo.toml:18",
      "crates/arin-core/Cargo.toml:8",
      "crates/arin-core/Cargo.toml:9",
    ]);
    assert.ok(fromToml.some((v) => v.message.includes("simd-json")), "別名の実体の名前で報告する");
    assert.ok(!violations.some((v) => v.snippet.includes("tempfile")), "dev-dependencies は対象外");
    assert.ok(violations.every((v) => v.rule === "core-deps"));
  });

  it("許可した依存を経由して禁止クレートが入ると Cargo.lock から見つける", () => {
    const violations = checkCoreDependencies(fixture("core-dep"));
    const fromLock = violations.filter((v) => v.file === "Cargo.lock");
    assert.equal(fromLock.length, 1);
    assert.match(fromLock[0].message, /libc/);
    assert.match(fromLock[0].message, /serde/);
  });

  it("許可された依存(serde、serde_json、thiserror、ts-rs)と dev-dependencies は通す", () => {
    assert.deepEqual(checkCoreDependencies(fixture("clean")), []);
  });

  it("Cargo.toml が無ければ「未作成」として失敗する", () => {
    const violations = checkCoreDependencies(fixture("csp-missing"));
    assert.equal(violations.length, 1);
    assert.match(violations[0].message, /未作成/);
  });
});

describe("2. arin-core の入出力の禁止", () => {
  it("std::fs・std::process・std::net(まとめた use、::std、std::os::unix::net を含む)を見つける", () => {
    const violations = checkCoreNoIo(fixture("core-io"));
    assert.deepEqual(places(violations), [
      "crates/arin-core/src/lib.rs:5",
      "crates/arin-core/src/lib.rs:8",
      "crates/arin-core/src/sub/mod.rs:1",
      "crates/arin-core/src/sub/mod.rs:2",
    ]);
    assert.ok(violations.every((v) => v.rule === "core-io"));
  });

  it("コメント・文字列・生文字列の中の出現は数えない", () => {
    assert.deepEqual(checkCoreNoIo(fixture("clean")), []);
  });
});

describe("3. @xterm/* の隔離", () => {
  it("src/terminal/xterm 以外からの import を見つける", () => {
    const violations = checkXtermImports(fixture("xterm-outside"));
    assert.deepEqual(places(violations), ["src/ui/Pane.tsx:1"]);
    assert.match(violations[0].snippet, /@xterm\/xterm/);
  });

  it("src/terminal/xterm からの import とコメント中の import は通す", () => {
    assert.deepEqual(checkXtermImports(fixture("clean")), []);
  });
});

describe("3b. CodeMirror と lezer の隔離(ADR-015 の 1)", () => {
  it("src_ui_から_codemirror_を_import_すると違反", () => {
    const violations = checkCodeMirrorImports(fixture("codemirror-outside"));
    assert.ok(places(violations).includes("src/ui/editor/Panel.tsx:2"));
    assert.ok(places(violations).includes("src/ui/editor/Panel.tsx:3"));
  });

  it("lezer_と_replit_も同じ", () => {
    const violations = checkCodeMirrorImports(fixture("codemirror-outside"));
    assert.deepEqual(places(violations), [
      "src/editor/EditorManager.ts:2",
      "src/editor/EditorManager.ts:4",
      "src/logic/editor/rules.ts:2",
      "src/ui/editor/Panel.tsx:2",
      "src/ui/editor/Panel.tsx:3",
    ]);
    assert.match(violations.find((v) => v.file === "src/logic/editor/rules.ts").snippet, /@lezer\/markdown/);
    assert.match(violations.find((v) => v.file === "src/editor/EditorManager.ts" && v.line === 2).snippet, /@replit/);
  });

  it("src_editor_codemirror_の中は違反でない", () => {
    assert.deepEqual(checkCodeMirrorImports(fixture("clean")), []);
  });

  it("src_ui_から_src_editor_codemirror_を_import_すると違反", () => {
    const violations = checkUiImports(fixture("codemirror-outside"));
    assert.deepEqual(places(violations), ["src/ui/editor/Panel.tsx:5"]);
    assert.match(violations[0].message, /src\/editor\/codemirror/);
  });

  it("src_editor_codemirror_から_src_ipc_を_import_すると違反", () => {
    const violations = checkCodeMirrorNoIpc(fixture("codemirror-imports-ipc"));
    assert.deepEqual(places(violations), ["src/editor/codemirror/CodeMirrorEngine.ts:3"]);
  });

  it("抽象(src/editor)への import は違反でない", () => {
    assert.deepEqual(checkCodeMirrorNoIpc(fixture("clean")), []);
    assert.deepEqual(checkUiImports(fixture("clean")), []);
  });
});

describe("4. @tauri-apps/api の隔離", () => {
  it("src/ipc 以外からの import(複数行の import と動的 import を含む)を見つける", () => {
    const violations = checkTauriApiImports(fixture("tauri-outside"));
    assert.deepEqual(places(violations), ["src/state/store.ts:3", "src/state/store.ts:6"]);
  });

  it("src/ipc からの import とテスト補助(src/test)は通す", () => {
    assert.deepEqual(checkTauriApiImports(fixture("clean")), []);
  });
});

describe("5. src/ui からの直接 import", () => {
  it("相対パスで src/ipc と src/terminal/xterm を越境する import を見つける", () => {
    const violations = checkUiImports(fixture("ui-imports"));
    assert.deepEqual(places(violations), [
      "src/ui/shell/Shell.tsx:2",
      "src/ui/shell/Shell.tsx:3",
      "src/ui/shell/Shell.tsx:4",
    ]);
  });

  it("src/terminal(抽象)と src/state への import は通す", () => {
    assert.deepEqual(checkUiImports(fixture("clean")), []);
  });
});

describe("6. src/terminal/xterm から src/ipc への import", () => {
  it("相対パスの import を見つける", () => {
    const violations = checkXtermNoIpc(fixture("xterm-imports-ipc"));
    assert.deepEqual(places(violations), ["src/terminal/xterm/XtermEngine.ts:2"]);
  });

  it("抽象(src/terminal)への import は通す", () => {
    assert.deepEqual(checkXtermNoIpc(fixture("clean")), []);
  });
});

describe("7. HTTP クライアントのクレート", () => {
  it("ワークスペースから届く通信のクレートを、引き込んだクレートとともに報告する", () => {
    const violations = checkNoHttpClientCrates(fixture("http-client"));
    assert.equal(violations.length, 1);
    assert.match(violations[0].message, /ureq/);
    assert.match(violations[0].message, /arin-platform/);
    assert.equal(violations[0].file, "Cargo.lock");
  });

  it("例外の辺(tauri → reqwest。macOS では組み込まれない)の先は数えない", () => {
    const violations = checkNoHttpClientCrates(fixture("http-client"));
    assert.ok(!violations.some((v) => /reqwest|hyper/.test(v.message)));
  });

  it("例外を外すと、推移的に入った reqwest と hyper も報告する", () => {
    const violations = checkNoHttpClientCrates(fixture("http-client"), { edgeExceptions: [] });
    assert.equal(violations.length, 3);
    const reqwest = violations.find((v) => v.message.includes("reqwest"));
    assert.ok(reqwest);
    assert.match(reqwest.message, /tauri/);
    assert.ok(violations.some((v) => v.message.includes("hyper")));
  });

  it("http(型だけのクレート)と HTTP クライアントでないクレートは通す", () => {
    assert.deepEqual(checkNoHttpClientCrates(fixture("clean")), []);
  });

  it("Cargo.lock が無ければ「未作成」として失敗する", () => {
    const violations = checkNoHttpClientCrates(fixture("csp-missing"));
    assert.equal(violations.length, 1);
    assert.match(violations[0].message, /未作成/);
  });
});

describe("8. 画面の通信 API", () => {
  it("fetch(・XMLHttpRequest・WebSocket・EventSource・sendBeacon を見つける", () => {
    const violations = checkNoNetworkApi(fixture("network-api"));
    assert.deepEqual(places(violations), [
      "src/state/sync.ts:2",
      "src/state/sync.ts:3",
      "src/state/sync.ts:4",
      "src/state/sync.ts:5",
      "src/state/sync.ts:6",
    ]);
  });

  it("コメントの中の出現、fetch を含む別の名前、テストのファイルは数えない", () => {
    assert.deepEqual(checkNoNetworkApi(fixture("clean")), []);
  });
});

describe("9. 画面への HTML の差し込み", () => {
  it("innerHTML・outerHTML(角括弧の参照)・insertAdjacentHTML・document.write・dangerouslySetInnerHTML を見つける", () => {
    const violations = checkNoHtmlInjection(fixture("html-injection"));
    assert.deepEqual(places(violations), [
      "src/ui/Name.tsx:2",
      "src/ui/Name.tsx:3",
      "src/ui/Name.tsx:4",
      "src/ui/Name.tsx:5",
      "src/ui/Name.tsx:6",
    ]);
  });

  it("コメントの中の出現とテストのファイル(*.test.tsx)は数えない", () => {
    assert.deepEqual(checkNoHtmlInjection(fixture("clean")), []);
  });
});

describe("10. CSP の connect-src", () => {
  it("tauri.conf.json が無ければ「未作成」として失敗する", () => {
    const violations = checkCspConnectSrc(fixture("csp-missing"));
    assert.equal(violations.length, 1);
    assert.match(violations[0].message, /未作成/);
  });

  it("connect-src が無ければ失敗する", () => {
    const violations = checkCspConnectSrc(fixture("csp-no-connect"));
    assert.equal(violations.length, 1);
    assert.match(violations[0].message, /connect-src/);
  });

  it("connect-src が外部を許していれば(オブジェクト形式の CSP でも)許した値ごとに失敗する", () => {
    const violations = checkCspConnectSrc(fixture("csp-wildcard"));
    assert.equal(violations.length, 2);
    assert.ok(violations.some((v) => v.message.includes("https:")));
    assert.ok(violations.some((v) => v.message.includes("*")));
  });

  it("IPC だけに絞った connect-src は通す", () => {
    assert.deepEqual(checkCspConnectSrc(fixture("clean")), []);
  });
});

describe("11. CSP のスクリプトとスタイル", () => {
  it("スクリプトを緩める値と、スタイルの外部の読み込み元を、csp と devCsp の両方で見つける", () => {
    const violations = checkCspScriptStyle(fixture("csp-loose-script"));
    const messages = violations.map((v) => v.message);

    assert.equal(violations.length, 3, messages.join("\n"));
    assert.ok(messages.some((m) => m.includes("csp") && m.includes("script-src") && m.includes("'unsafe-eval'")));
    assert.ok(messages.some((m) => m.includes("style-src-elem") && m.includes("https://cdn.example.com")));
    // devCsp には script-src が無いので default-src の * がスクリプトにも効く。
    assert.ok(messages.some((m) => m.includes("devCsp") && m.includes("default-src") && m.includes("*")));
  });

  it("スタイルの 'unsafe-inline' と 'self' だけのスクリプトは通す", () => {
    assert.deepEqual(checkCspScriptStyle(fixture("clean")), []);
  });

  it("tauri.conf.json や CSP が無いときは 10 の検査に任せて何も言わない", () => {
    assert.deepEqual(checkCspScriptStyle(fixture("csp-missing")), []);
  });
});

describe("出力と終了コード", () => {
  it("違反 1 件を 1 行(規則名、ファイル、行番号、該当箇所)で表す", () => {
    const [violation] = checkXtermImports(fixture("xterm-outside"));
    const line = formatViolation(violation);
    assert.doesNotMatch(line, /\n/);
    assert.match(line, /xterm-import/);
    assert.match(line, /src\/ui\/Pane\.tsx:1/);
    assert.match(line, /@xterm\/xterm/);
  });

  it("違反があれば終了コード 1 を返し、違反を表示する", () => {
    const result = spawnSync(process.execPath, [script, fixture("html-injection")], {
      encoding: "utf8",
    });
    assert.equal(result.status, 1);
    assert.match(result.stdout + result.stderr, /html-injection/);
    assert.match(result.stdout + result.stderr, /src\/ui\/Name\.tsx:2/);
  });
});

describe("8b. Rust の通信は新しい版の確認だけ(ADR-018)", () => {
  it("release の外の curl の起動と std::net を見つけ_コメントと release の中は数えない", () => {
    const violations = checkNetworkOnlyInRelease(fixture("network-process"));
    assert.deepEqual(places(violations), ["src-tauri/src/leak.rs:3", "src-tauri/src/leak.rs:4"]);
    assert.ok(violations.every((v) => v.rule === "network-process"));
  });

  it("違反の無い見本では何も出さない", () => {
    assert.deepEqual(checkNetworkOnlyInRelease(fixture("clean")), []);
  });
});
