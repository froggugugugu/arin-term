import { describe, expect, it } from "vitest";

import { DEFAULT_SHORTCUT_TABLE } from "./defaultShortcuts";
import { type KeyChord, terminalKeyOwner as ownerFromTable } from "./keyOwner";
import type { ShortcutCommandId } from "./shortcutTable";

/** M2 のメニューにあった行(M2 の直書きの集合と同じ範囲)。M2 の振る舞いと同じかを、この表で確かめる。 */
const M2_COMMANDS: readonly ShortcutCommandId[] = [
  "terminal.new-tab",
  "terminal.close-tab",
  "terminal.previous-tab",
  "terminal.next-tab",
  "edit.select-all",
  "edit.copy",
  "edit.paste",
];
const M2_TABLE = DEFAULT_SHORTCUT_TABLE.filter((binding) => M2_COMMANDS.includes(binding.command));

/** M2 のテストをそのまま使うため、M2 の表を渡した受け手。 */
const terminalKeyOwner = (keys: KeyChord) => ownerFromTable(M2_TABLE, keys);

const chord = (key: string, mods: Partial<Omit<KeyChord, "key">> = {}): KeyChord => ({
  key,
  metaKey: false,
  ctrlKey: false,
  altKey: false,
  shiftKey: false,
  ...mods,
});

describe("terminalKeyOwner(ターミナルにフォーカスがあるときのキーの受け手。表から作ったターミナルのキーの受け手は_m2_と同じ)", () => {
  it("⌘C と ⌘V はアプリが受ける(メニューから画面のコピー・ペーストのイベントにする)", () => {
    expect(terminalKeyOwner(chord("c", { metaKey: true }))).toBe("app");
    expect(terminalKeyOwner(chord("v", { metaKey: true }))).toBe("app");
  });

  it("Caps Lock などで大文字になっても ⌘C・⌘V はアプリが受ける", () => {
    expect(terminalKeyOwner(chord("C", { metaKey: true }))).toBe("app");
    expect(terminalKeyOwner(chord("V", { metaKey: true }))).toBe("app");
  });

  it("ターミナルにフォーカスがあっても ⌘T ⌘W ⌘⇧[ ⌘⇧] はアプリが受ける", () => {
    expect(terminalKeyOwner(chord("t", { metaKey: true }))).toBe("app");
    expect(terminalKeyOwner(chord("w", { metaKey: true }))).toBe("app");
    expect(terminalKeyOwner(chord("T", { metaKey: true }))).toBe("app");
    expect(terminalKeyOwner(chord("W", { metaKey: true }))).toBe("app");
    // ⇧ を押すと key は配列によって「[」「]」のままのことも「{」「}」になることもある
    expect(terminalKeyOwner(chord("[", { metaKey: true, shiftKey: true }))).toBe("app");
    expect(terminalKeyOwner(chord("]", { metaKey: true, shiftKey: true }))).toBe("app");
    expect(terminalKeyOwner(chord("{", { metaKey: true, shiftKey: true }))).toBe("app");
    expect(terminalKeyOwner(chord("}", { metaKey: true, shiftKey: true }))).toBe("app");
  });

  it("⌘A はアプリが受ける(メニューの「すべてを選択」にする)", () => {
    expect(terminalKeyOwner(chord("a", { metaKey: true }))).toBe("app");
    expect(terminalKeyOwner(chord("A", { metaKey: true }))).toBe("app");
  });

  it("⇧ の有無が違う組み合わせはターミナルに任せる(割り当てない)", () => {
    expect(terminalKeyOwner(chord("C", { metaKey: true, shiftKey: true }))).toBe("terminal");
    expect(terminalKeyOwner(chord("V", { metaKey: true, shiftKey: true }))).toBe("terminal");
    expect(terminalKeyOwner(chord("T", { metaKey: true, shiftKey: true }))).toBe("terminal");
    expect(terminalKeyOwner(chord("A", { metaKey: true, shiftKey: true }))).toBe("terminal");
    expect(terminalKeyOwner(chord("[", { metaKey: true }))).toBe("terminal");
    expect(terminalKeyOwner(chord("]", { metaKey: true }))).toBe("terminal");
  });

  it("Ctrl と Option の組み合わせは今までどおりターミナルが受ける", () => {
    expect(terminalKeyOwner(chord("c", { ctrlKey: true }))).toBe("terminal");
    expect(terminalKeyOwner(chord("v", { ctrlKey: true }))).toBe("terminal");
    expect(terminalKeyOwner(chord("t", { ctrlKey: true }))).toBe("terminal");
    expect(terminalKeyOwner(chord("w", { ctrlKey: true }))).toBe("terminal");
    expect(terminalKeyOwner(chord("a", { ctrlKey: true }))).toBe("terminal");
    expect(terminalKeyOwner(chord("[", { ctrlKey: true }))).toBe("terminal");
    expect(terminalKeyOwner(chord("†", { altKey: true }))).toBe("terminal");
    expect(terminalKeyOwner(chord("c", { metaKey: true, ctrlKey: true }))).toBe("terminal");
    expect(terminalKeyOwner(chord("t", { metaKey: true, altKey: true }))).toBe("terminal");
    expect(terminalKeyOwner(chord("[", { metaKey: true, shiftKey: true, altKey: true }))).toBe(
      "terminal",
    );
    expect(terminalKeyOwner(chord("]", { metaKey: true, shiftKey: true, ctrlKey: true }))).toBe(
      "terminal",
    );
  });

  it("修飾キーなし・ほかの ⌘ のキーはターミナルに任せる", () => {
    expect(terminalKeyOwner(chord("t"))).toBe("terminal");
    expect(terminalKeyOwner(chord("["))).toBe("terminal");
    expect(terminalKeyOwner(chord("{", { shiftKey: true }))).toBe("terminal");
    expect(terminalKeyOwner(chord("x", { metaKey: true }))).toBe("terminal");
    expect(terminalKeyOwner(chord("1", { metaKey: true }))).toBe("terminal");
    expect(terminalKeyOwner(chord("Meta", { metaKey: true }))).toBe("terminal");
  });
});

describe("terminalKeyOwner(M3 の表)", () => {
  const owner = (keys: KeyChord) => ownerFromTable(DEFAULT_SHORTCUT_TABLE, keys);

  it("受け口がメニューの項目と編集の項目のキーはアプリが受ける(⌘X の切り取りと ⌘Q も)", () => {
    expect(owner(chord("x", { metaKey: true }))).toBe("app");
    expect(owner(chord("q", { metaKey: true }))).toBe("app");
    expect(owner(chord("c", { metaKey: true }))).toBe("app");
    expect(owner(chord("{", { metaKey: true, shiftKey: true }))).toBe("app");
  });

  it("受け口が画面のキー(ツリーのキー)はターミナルに任せる", () => {
    expect(owner(chord("n", { metaKey: true }))).toBe("terminal");
    expect(owner(chord("N", { metaKey: true, shiftKey: true }))).toBe("terminal");
    expect(owner(chord("o", { metaKey: true }))).toBe("terminal");
    expect(owner(chord("Enter"))).toBe("terminal");
    expect(owner(chord("Enter", { metaKey: true }))).toBe("terminal");
    expect(owner(chord("Backspace", { metaKey: true }))).toBe("terminal");
    expect(owner(chord("®", { metaKey: true, altKey: true, code: "KeyR" }))).toBe("terminal");
  });

  it("表示の操作のキーはターミナルに渡さない", () => {
    // ⇧⌘G(⇧ で key が大文字になる)・⌥⌘←・⌥⌘→・⇧⌘↩。表の受け口がメニューの項目なので、直書きせずに拾う
    expect(owner(chord("G", { metaKey: true, shiftKey: true }))).toBe("app");
    expect(owner(chord("g", { metaKey: true, shiftKey: true }))).toBe("app");
    expect(owner(chord("ArrowLeft", { metaKey: true, altKey: true, code: "ArrowLeft" }))).toBe("app");
    expect(owner(chord("ArrowRight", { metaKey: true, altKey: true, code: "ArrowRight" }))).toBe("app");
    expect(owner(chord("Enter", { metaKey: true, shiftKey: true }))).toBe("app");
    // 修飾キーが 1 つ違うものはターミナルに任せる(⌘↩ はツリーのパスの挿入で、ターミナルでは使わない)
    expect(owner(chord("g", { metaKey: true }))).toBe("terminal");
    expect(owner(chord("ArrowLeft", { metaKey: true, code: "ArrowLeft" }))).toBe("terminal");
    expect(owner(chord("ArrowLeft", { altKey: true, code: "ArrowLeft" }))).toBe("terminal");
    expect(owner(chord("ArrowRight", { metaKey: true, code: "ArrowRight" }))).toBe("terminal");
    expect(owner(chord("Enter", { shiftKey: true }))).toBe("terminal");
    // 表の行を外すとターミナルに任せる(キーは表から拾っている)
    const withoutView = DEFAULT_SHORTCUT_TABLE.filter((binding) => !binding.command.startsWith("view."));
    expect(ownerFromTable(withoutView, chord("Enter", { metaKey: true, shiftKey: true }))).toBe("terminal");
    expect(
      ownerFromTable(withoutView, chord("ArrowLeft", { metaKey: true, altKey: true, code: "ArrowLeft" })),
    ).toBe("terminal");
  });

  it("設定ファイルを開くキーはターミナルに渡さない", () => {
    // ⌘,(計画 T64c。受け口がメニューの項目なので表から拾う)
    expect(owner(chord(",", { metaKey: true }))).toBe("app");
    expect(owner(chord(",", { metaKey: true, code: "Comma" }))).toBe("app");
    // 修飾キーが違うものはターミナルに任せる
    expect(owner(chord(","))).toBe("terminal");
    expect(owner(chord(",", { ctrlKey: true }))).toBe("terminal");
    expect(owner(chord(",", { metaKey: true, altKey: true, code: "Comma" }))).toBe("terminal");
    // 表の行を外すとターミナルに任せる
    const withoutSettings = DEFAULT_SHORTCUT_TABLE.filter((binding) => binding.command !== "app.open-settings");
    expect(ownerFromTable(withoutSettings, chord(",", { metaKey: true }))).toBe("terminal");
  });

  it("表を替えると新しいメニューのキーはターミナルに渡さず古いキーは渡す", () => {
    // 設定ファイルで新規タブを ⇧⌘Y に替えた表(AC-K06。設定が変わると uiStore の表が置き換わる)
    const changed = DEFAULT_SHORTCUT_TABLE.map((binding) =>
      binding.command === "terminal.new-tab" ? { ...binding, accelerator: "Cmd+Shift+Y" } : binding,
    );
    expect(ownerFromTable(changed, chord("y", { metaKey: true, shiftKey: true }))).toBe("app");
    expect(ownerFromTable(changed, chord("Y", { metaKey: true, shiftKey: true }))).toBe("app");
    // 替える前のキー(⌘T)はターミナルに渡す
    expect(ownerFromTable(changed, chord("t", { metaKey: true }))).toBe("terminal");
    // 替える前の表では逆
    expect(owner(chord("t", { metaKey: true }))).toBe("app");
    expect(owner(chord("y", { metaKey: true, shiftKey: true }))).toBe("terminal");
    // 画面で受けるキー(ツリー)を替えても、ターミナルでは新旧どちらもターミナルに渡す
    const tree = DEFAULT_SHORTCUT_TABLE.map((binding) =>
      binding.command === "tree.new-file" ? { ...binding, accelerator: "Cmd+Alt+J" } : binding,
    );
    expect(ownerFromTable(tree, chord("j", { metaKey: true, altKey: true, code: "KeyJ" }))).toBe("terminal");
    expect(ownerFromTable(tree, chord("n", { metaKey: true }))).toBe("terminal");
  });

  it("表が空ならすべてターミナルに任せる", () => {
    expect(ownerFromTable([], chord("c", { metaKey: true }))).toBe("terminal");
  });
});
