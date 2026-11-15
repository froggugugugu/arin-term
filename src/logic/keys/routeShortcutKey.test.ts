import { describe, expect, it } from "vitest";

import { DEFAULT_SHORTCUT_TABLE } from "./defaultShortcuts";
import type { KeyChord } from "./keyOwner";
import { routeShortcutKey } from "./routeShortcutKey";
import { parseAccelerator, type ShortcutTable } from "./shortcutTable";

const chord = (key: string, mods: Partial<Omit<KeyChord, "key">> = {}): KeyChord => ({
  key,
  metaKey: false,
  ctrlKey: false,
  altKey: false,
  shiftKey: false,
  ...mods,
});

/** 表のキーの文字列から、そのキーを押したときの KeyChord を作る(⌥ のときは key を別の文字にする)。 */
function chordOf(accelerator: string): KeyChord {
  const parsed = parseAccelerator(accelerator);
  if (parsed === null) throw new Error(`読めない: ${accelerator}`);
  const letter = /^[A-Z]$/.test(parsed.key);
  return {
    key: parsed.alt ? "∂" : letter ? parsed.key.toLowerCase() : parsed.key,
    code: letter ? `Key${parsed.key}` : parsed.key,
    metaKey: parsed.meta,
    ctrlKey: parsed.ctrl,
    altKey: parsed.alt,
    shiftKey: parsed.shift,
  };
}

const screenRows = DEFAULT_SHORTCUT_TABLE.filter((binding) => binding.receiver === "screen");
const otherRows = DEFAULT_SHORTCUT_TABLE.filter((binding) => binding.receiver !== "screen");

describe("routeShortcutKey(画面の keydown をフォーカス位置でコマンドの id にする)", () => {
  it("ツリーにフォーカスがあると表のキーをコマンドの_id_にする", () => {
    // 見本の表で受け口が画面の行は 9 つ(PRD §6.4 の Must のうち、編集の項目で受ける ⌘C・⌘X・⌘V を除いたもの)
    expect(screenRows.map((binding) => binding.command)).toEqual([
      "tree.open",
      "tree.reveal-in-finder",
      "tree.insert-path",
      "tree.copy-relative-path",
      "tree.copy-absolute-path",
      "tree.rename",
      "tree.trash",
      "tree.new-file",
      "tree.new-folder",
    ]);
    for (const binding of screenRows) {
      expect(routeShortcutKey(DEFAULT_SHORTCUT_TABLE, chordOf(binding.accelerator), "tree")).toBe(
        binding.command,
      );
    }
  });

  it("受け口がメニューの項目と編集の項目のキーは keydown では受けない(OS とコピー・ペーストのイベントが受ける)", () => {
    for (const binding of otherRows) {
      expect(routeShortcutKey(DEFAULT_SHORTCUT_TABLE, chordOf(binding.accelerator), "tree")).toBeNull();
    }
  });

  it("ターミナルにフォーカスがあるとツリーのキーは何もしない", () => {
    for (const binding of screenRows) {
      expect(routeShortcutKey(DEFAULT_SHORTCUT_TABLE, chordOf(binding.accelerator), "terminal")).toBeNull();
    }
  });

  it("名前の入力中はツリーのキーを受けない", () => {
    for (const binding of screenRows) {
      expect(routeShortcutKey(DEFAULT_SHORTCUT_TABLE, chordOf(binding.accelerator), "name-input")).toBeNull();
    }
  });

  it("ほかの場所にフォーカスがあっても受けない", () => {
    expect(routeShortcutKey(DEFAULT_SHORTCUT_TABLE, chord("n", { metaKey: true }), "other")).toBeNull();
  });

  it("修飾キーが違う・表に無いキーは受けない", () => {
    expect(routeShortcutKey(DEFAULT_SHORTCUT_TABLE, chord("n", { metaKey: true, ctrlKey: true }), "tree")).toBeNull();
    expect(routeShortcutKey(DEFAULT_SHORTCUT_TABLE, chord("Enter", { shiftKey: true }), "tree")).toBeNull();
    expect(routeShortcutKey(DEFAULT_SHORTCUT_TABLE, chord("z", { metaKey: true }), "tree")).toBeNull();
  });

  it("option_を含むキーは押した文字が変わっても一致する", () => {
    expect(
      routeShortcutKey(DEFAULT_SHORTCUT_TABLE, chord("®", { metaKey: true, altKey: true, code: "KeyR" }), "tree"),
    ).toBe("tree.reveal-in-finder");
    expect(
      routeShortcutKey(
        DEFAULT_SHORTCUT_TABLE,
        chord("Ç", { metaKey: true, altKey: true, shiftKey: true, code: "KeyC" }),
        "tree",
      ),
    ).toBe("tree.copy-relative-path");
    expect(
      routeShortcutKey(DEFAULT_SHORTCUT_TABLE, chord("ç", { metaKey: true, altKey: true, code: "KeyC" }), "tree"),
    ).toBe("tree.copy-absolute-path");
  });

  it("表の 1 行を変えると、その行のキーだけが変わる(設定ファイルでキーを変える土台)", () => {
    const table: ShortcutTable = DEFAULT_SHORTCUT_TABLE.map((binding) =>
      binding.command === "tree.new-file" ? { ...binding, accelerator: "Cmd+Alt+N" } : binding,
    );
    expect(routeShortcutKey(table, chord("n", { metaKey: true }), "tree")).toBeNull();
    expect(routeShortcutKey(table, chord("˜", { metaKey: true, altKey: true, code: "KeyN" }), "tree")).toBe(
      "tree.new-file",
    );
  });

  it("受け口を画面からメニューの項目に替えた行は keydown で受けない(【仮定】21 の切り替え)", () => {
    const table: ShortcutTable = DEFAULT_SHORTCUT_TABLE.map((binding) =>
      binding.command === "tree.trash" ? { ...binding, receiver: "menu-item" } : binding,
    );
    expect(routeShortcutKey(table, chord("Backspace", { metaKey: true }), "tree")).toBeNull();
  });

  it("読めないキーの行は一致しない", () => {
    const table: ShortcutTable = [{ command: "tree.open", accelerator: "Hyper+O", receiver: "screen" }];
    expect(routeShortcutKey(table, chord("o", { metaKey: true }), "tree")).toBeNull();
  });
});
