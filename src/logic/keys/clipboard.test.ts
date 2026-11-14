import { describe, expect, it } from "vitest";

import { type KeyChord, routeClipboard, terminalKeyOwner } from "./clipboard";

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

  it("⌘⇧C・⌘⇧V はターミナルに任せる(M1 では割り当てない)", () => {
    expect(terminalKeyOwner(chord("C", { metaKey: true, shiftKey: true }))).toBe("terminal");
    expect(terminalKeyOwner(chord("V", { metaKey: true, shiftKey: true }))).toBe("terminal");
  });

  it("Ctrl+C・Ctrl+V はターミナルに任せる(xterm.js が \\x03・\\x16 にして PTY へ送る)", () => {
    expect(terminalKeyOwner(chord("c", { ctrlKey: true }))).toBe("terminal");
    expect(terminalKeyOwner(chord("v", { ctrlKey: true }))).toBe("terminal");
  });

  it("⌘ に Ctrl・Option を重ねた組み合わせはターミナルに任せる", () => {
    expect(terminalKeyOwner(chord("c", { metaKey: true, ctrlKey: true }))).toBe("terminal");
    expect(terminalKeyOwner(chord("c", { metaKey: true, altKey: true }))).toBe("terminal");
    expect(terminalKeyOwner(chord("v", { metaKey: true, altKey: true }))).toBe("terminal");
  });

  it("修飾キーなし・Option だけ・ほかの ⌘ のキーはターミナルに任せる", () => {
    expect(terminalKeyOwner(chord("c"))).toBe("terminal");
    expect(terminalKeyOwner(chord("ç", { altKey: true }))).toBe("terminal");
    expect(terminalKeyOwner(chord("a", { metaKey: true }))).toBe("terminal");
    expect(terminalKeyOwner(chord("x", { metaKey: true }))).toBe("terminal");
    expect(terminalKeyOwner(chord("Meta", { metaKey: true }))).toBe("terminal");
  });
});

describe("routeClipboard(画面のコピー・ペーストのイベントの振り分け)", () => {
  it("フォーカスがターミナルで選択があれば、コピーは選択のコピーになる", () => {
    expect(routeClipboard("copy", { kind: "terminal", hasSelection: true })).toBe("copy-selection");
  });

  it("フォーカスがターミナルで選択が無ければ、コピーは何もしない", () => {
    expect(routeClipboard("copy", { kind: "terminal", hasSelection: false })).toBe("none");
  });

  it("フォーカスがターミナルなら、ペーストはターミナルへの貼り付けになる(選択の有無によらない)", () => {
    expect(routeClipboard("paste", { kind: "terminal", hasSelection: false })).toBe("paste");
    expect(routeClipboard("paste", { kind: "terminal", hasSelection: true })).toBe("paste");
  });

  it("フォーカスがターミナルでなければ、どちらも振り分けない(画面の既定の動作に任せる)", () => {
    expect(routeClipboard("copy", { kind: "other" })).toBe("none");
    expect(routeClipboard("paste", { kind: "other" })).toBe("none");
  });
});
