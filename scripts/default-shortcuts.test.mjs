// 画面の見本の表(src/logic/keys/defaultShortcuts.ts)が、Rust から生成した既定の表
// (src/ipc/generated/defaultShortcuts.json)と同じ中身・同じ順であることを確かめる(計画 T42e)。
//
// src/logic は src/ipc を import できず(tsconfig の rootDir)、src/ と e2e/ の tsconfig は JSON を対象に含めないので、
// node のテストで両方を読む(Node 24 は .ts を型を取り除いて読み込める)。偽の通信(src/test/fakeIpc.ts、
// e2e/fixtures/tauriMock.ts)は get_shortcuts の既定の応答にこの見本を使うので、このテストで生成物と同じことを保証する。
// 生成物が Rust の既定の表と同じことは crates/arin-core/tests/generated_types.rs と src-tauri/tests/app_menu.rs が確かめる。

import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { describe, it } from "node:test";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const generatedPath = path.join(root, "src", "ipc", "generated", "defaultShortcuts.json");

describe("既定のショートカットの表", () => {
  it("見本の表は生成した既定の表と同じ中身で同じ順", async () => {
    const { DEFAULT_SHORTCUT_TABLE } = await import("../src/logic/keys/defaultShortcuts.ts");
    const generated = JSON.parse(fs.readFileSync(generatedPath, "utf8"));
    assert.deepEqual(DEFAULT_SHORTCUT_TABLE, generated);
  });

  it("生成した既定の表のキーはすべて画面で読める", async () => {
    const { parseAccelerator } = await import("../src/logic/keys/shortcutTable.ts");
    const generated = JSON.parse(fs.readFileSync(generatedPath, "utf8"));
    assert.equal(generated.length, 23);
    for (const binding of generated) {
      assert.notEqual(parseAccelerator(binding.accelerator), null, binding.command);
    }
  });
});
