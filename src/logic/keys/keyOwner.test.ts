import { describe, expect, it } from "vitest";

import { type KeyChord, terminalKeyOwner } from "./keyOwner";

const chord = (key: string, mods: Partial<Omit<KeyChord, "key">> = {}): KeyChord => ({
  key,
  metaKey: false,
  ctrlKey: false,
  altKey: false,
  shiftKey: false,
  ...mods,
});

describe("terminalKeyOwner(ターミナルにフォーカスがあるときのキーの受け手)", () => {
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
