// scripts/gen-third-party-licenses.mjs の純粋な部分(ファイル名の判定・依存のたどり方・MIT の代わりの全文・まとめ方)を確かめる。
// cargo と node_modules を読む本体(main)は、npm run licenses:check が実物で確かめる。

import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { collectShippedCrates, isLicenseFileName, mitFallbackText, renderLicenses } from "./gen-third-party-licenses.mjs";

const normal = [{ kind: null, target: null }];
const dev = [{ kind: "dev", target: null }];
const build = [{ kind: "build", target: null }];

// アプリ本体 arin → arin-platform(自前)→ tauri → serde。arin は dev で tempfile、build で cc を使う。
function sampleMetadata() {
  const pkg = (id, name) => ({ id, name, version: "1.0.0" });
  return {
    workspace_members: ["arin-id", "core-id", "platform-id"],
    packages: [
      pkg("arin-id", "arin"),
      pkg("core-id", "arin-core"),
      pkg("platform-id", "arin-platform"),
      pkg("tauri-id", "tauri"),
      pkg("serde-id", "serde"),
      pkg("tempfile-id", "tempfile"),
      pkg("cc-id", "cc"),
    ],
    resolve: {
      root: null,
      nodes: [
        {
          id: "arin-id",
          deps: [
            { pkg: "platform-id", dep_kinds: normal },
            { pkg: "tempfile-id", dep_kinds: dev },
            { pkg: "cc-id", dep_kinds: build },
          ],
        },
        { id: "platform-id", deps: [{ pkg: "core-id", dep_kinds: normal }, { pkg: "tauri-id", dep_kinds: normal }] },
        { id: "core-id", deps: [{ pkg: "serde-id", dep_kinds: normal }] },
        { id: "tauri-id", deps: [{ pkg: "serde-id", dep_kinds: normal }] },
        { id: "serde-id", deps: [] },
        { id: "tempfile-id", deps: [] },
        { id: "cc-id", deps: [] },
      ],
    },
  };
}

describe("ライセンスのファイル名", () => {
  it("LICENSE・COPYING・NOTICE とその派生を拾う", () => {
    for (const name of ["LICENSE", "LICENSE-MIT", "LICENSE-APACHE", "license.txt", "COPYING", "NOTICE", "UNLICENSE", "LICENCE.md"]) {
      assert.equal(isLicenseFileName(name), true, name);
    }
  });
  it("ソースや設定のファイルは拾わない", () => {
    for (const name of ["license.rs", "license.json", "README.md", "Cargo.toml", "licenses.toml"]) {
      assert.equal(isLicenseFileName(name), false, name);
    }
  });
});

describe("配布物に入るクレート", () => {
  it("アプリ本体から通常の依存だけをたどり、自前のクレートと dev・build の依存を除く", () => {
    const names = collectShippedCrates(sampleMetadata()).map((p) => p.name);
    assert.deepEqual(names, ["serde", "tauri"]);
  });
  it("アプリ本体のクレートが無ければ失敗する", () => {
    assert.throws(() => collectShippedCrates(sampleMetadata(), "no-such-crate"));
  });
});

describe("MIT の代わりの全文", () => {
  it("作者のメールアドレスを落として著作権者にする", () => {
    const text = mitFallbackText(["Alice <alice@example.com>", "Bob"]);
    assert.match(text, /^MIT License\n\nCopyright \(c\) Alice, Bob\n/);
    assert.doesNotMatch(text, /@/);
  });
  it("作者が無ければ the package authors にする", () => {
    assert.match(mitFallbackText([]), /Copyright \(c\) the package authors/);
  });
});

describe("一覧のまとめ方", () => {
  it("全文が同じパッケージは 1 つの節にまとめ、名前の順に並べる", () => {
    const mit = [{ file: "LICENSE", text: "same\r\n" }];
    const out = renderLicenses([
      { name: "b", version: "1.0.0", license: "MIT", texts: mit },
      { name: "a", version: "2.0.0", license: "MIT", texts: mit },
      { name: "c", version: "1.0.0", license: "MPL-2.0", texts: [{ file: "LICENSE", text: "other" }] },
    ]);
    assert.match(out, /## a 2\.0\.0\(MIT\) \/ b 1\.0\.0\(MIT\)\n/);
    assert.match(out, /## c 1\.0\.0\(MPL-2\.0\)\n/);
    assert.ok(out.indexOf("## a ") < out.indexOf("## c "));
    assert.doesNotMatch(out, /\r/);
  });
});
