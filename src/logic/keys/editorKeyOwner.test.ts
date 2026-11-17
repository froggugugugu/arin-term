import { describe, expect, it } from "vitest";

import { DEFAULT_SHORTCUT_TABLE } from "./defaultShortcuts";
import { editorKeyOwner } from "./editorKeyOwner";
import type { KeyChord } from "./keyOwner";
import { parseAccelerator, type ShortcutTable } from "./shortcutTable";

const chord = (key: string, mods: Partial<Omit<KeyChord, "key">> = {}): KeyChord => ({
  key,
  metaKey: false,
  ctrlKey: false,
  altKey: false,
  shiftKey: false,
  ...mods,
});

/** 表のキーの文字列を、そのキーを押したときの keydown にする(⌥ を含むときは物理的な位置も付ける)。 */
function chordOf(accelerator: string): KeyChord {
  const parsed = parseAccelerator(accelerator);
  if (parsed === null) throw new Error(`読めない: ${accelerator}`);
  const code = /^[a-z]$/i.test(parsed.key) ? `Key${parsed.key.toUpperCase()}` : /^[0-9]$/.test(parsed.key) ? `Digit${parsed.key}` : parsed.key;
  return { key: parsed.key, code, metaKey: parsed.meta, ctrlKey: parsed.ctrl, altKey: parsed.alt, shiftKey: parsed.shift };
}

describe("editorKeyOwner(エディタにフォーカスがあるときのキーの受け手。ADR-015 の 6)", () => {
  it("menu_item_と_edit_item_のキーは_app", () => {
    const rows = DEFAULT_SHORTCUT_TABLE.filter((b) => b.receiver === "menu-item" || b.receiver === "edit-item");
    expect(rows.length).toBeGreaterThan(20);
    for (const binding of rows) {
      expect(editorKeyOwner(DEFAULT_SHORTCUT_TABLE, chordOf(binding.accelerator)), binding.command).toBe("app");
    }
    // ⌘W・⌘T・⌘Q・⌘,・⇧⌘G・⌘1・⌘A・⌘C
    for (const [key, mods] of [
      ["w", { metaKey: true }],
      ["t", { metaKey: true }],
      ["q", { metaKey: true }],
      [",", { metaKey: true }],
      ["G", { metaKey: true, shiftKey: true }],
      ["1", { metaKey: true }],
      ["a", { metaKey: true }],
      ["c", { metaKey: true }],
    ] as const) {
      expect(editorKeyOwner(DEFAULT_SHORTCUT_TABLE, chord(key, mods)), key).toBe("app");
    }
  });

  it("editor_save_のキーは保存", () => {
    expect(editorKeyOwner(DEFAULT_SHORTCUT_TABLE, chord("s", { metaKey: true }))).toBe("save");
    // Caps Lock で大文字でも同じ
    expect(editorKeyOwner(DEFAULT_SHORTCUT_TABLE, chord("S", { metaKey: true }))).toBe("save");
  });

  it("それ以外と_screen_の木のキーはエディタ", () => {
    const tree = DEFAULT_SHORTCUT_TABLE.filter((b) => b.receiver === "screen" && b.command !== "editor.save");
    expect(tree.map((b) => b.command)).toContain("tree.rename");
    for (const binding of tree) {
      expect(editorKeyOwner(DEFAULT_SHORTCUT_TABLE, chordOf(binding.accelerator)), binding.command).toBe("editor");
    }
    // ⌘Z・⇧⌘Z(表に無い。CodeMirror の取り消し)、Esc(Vim)、ふつうの文字、⌥ の文字
    for (const [key, mods] of [
      ["z", { metaKey: true }],
      ["z", { metaKey: true, shiftKey: true }],
      ["Escape", {}],
      ["a", {}],
      ["Enter", {}],
      ["s", { ctrlKey: true }],
      ["ß", { altKey: true, code: "KeyS" }],
    ] as const) {
      expect(editorKeyOwner(DEFAULT_SHORTCUT_TABLE, chord(key, mods)), key).toBe("editor");
    }
  });

  it("表でキーを替えると追従する", () => {
    const table: ShortcutTable = DEFAULT_SHORTCUT_TABLE.map((b) =>
      b.command === "editor.save"
        ? { ...b, accelerator: "Cmd+Alt+S" }
        : b.command === "terminal.close-tab"
          ? { ...b, accelerator: "Cmd+Shift+W" }
          : b,
    );
    expect(editorKeyOwner(table, chord("s", { metaKey: true }))).toBe("editor");
    expect(editorKeyOwner(table, chord("ß", { metaKey: true, altKey: true, code: "KeyS" }))).toBe("save");
    expect(editorKeyOwner(table, chord("w", { metaKey: true }))).toBe("editor");
    expect(editorKeyOwner(table, chord("W", { metaKey: true, shiftKey: true }))).toBe("app");
  });

  it("保存の行が無い表では保存しない", () => {
    const table = DEFAULT_SHORTCUT_TABLE.filter((b) => b.command !== "editor.save");
    expect(editorKeyOwner(table, chord("s", { metaKey: true }))).toBe("editor");
  });

  it("保存のキーをメニューの項目と同じキーにしたら_app_を先にする", () => {
    const table: ShortcutTable = DEFAULT_SHORTCUT_TABLE.map((b) =>
      b.command === "editor.save" ? { ...b, accelerator: "Cmd+W" } : b,
    );
    expect(editorKeyOwner(table, chord("w", { metaKey: true }))).toBe("app");
  });
});
