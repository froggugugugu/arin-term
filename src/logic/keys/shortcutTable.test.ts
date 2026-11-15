import { describe, expect, it } from "vitest";

import { DEFAULT_SHORTCUT_TABLE } from "./defaultShortcuts";
import type { KeyChord } from "./keyOwner";
import {
  acceleratorOf,
  formatAccelerator,
  matchesAccelerator,
  parseAccelerator,
  shortcutLabel,
} from "./shortcutTable";

const chord = (key: string, mods: Partial<Omit<KeyChord, "key">> = {}): KeyChord => ({
  key,
  metaKey: false,
  ctrlKey: false,
  altKey: false,
  shiftKey: false,
  ...mods,
});

describe("parseAccelerator(キーの文字列を修飾キーとキーに分けて読む)", () => {
  it("キーの文字列を修飾キーとキーに分けて読める", () => {
    expect(parseAccelerator("Cmd+Shift+N")).toEqual({
      meta: true,
      shift: true,
      alt: false,
      ctrl: false,
      key: "N",
    });
    expect(parseAccelerator("Enter")).toEqual({
      meta: false,
      shift: false,
      alt: false,
      ctrl: false,
      key: "Enter",
    });
    expect(parseAccelerator("Cmd+Shift+[")?.key).toBe("[");
  });

  it("修飾キーの別名と順の違いを同じに読む", () => {
    expect(parseAccelerator("Shift+Option+Command+c")).toEqual(parseAccelerator("Cmd+Shift+Alt+C"));
    expect(parseAccelerator("Control+Super+t")).toEqual(parseAccelerator("Ctrl+Cmd+T"));
  });

  it("読めない文字列は null", () => {
    expect(parseAccelerator("")).toBeNull();
    expect(parseAccelerator("Cmd+")).toBeNull();
    expect(parseAccelerator("Cmd+Shift")).toBeNull();
    expect(parseAccelerator("Hyper+A")).toBeNull();
    expect(parseAccelerator("Cmd+Cmd+A")).toBeNull();
    expect(parseAccelerator("Cmd+A+B")).toBeNull();
    expect(parseAccelerator("Cmd+Foo")).toBeNull();
  });

  it("Rust_の_KeyChord_と同じく_Return_は読まない", () => {
    // muda は Return を読めない(Tauri が黙って捨てる)ので、Rust の KeyChord::parse も拒否する。名前は Enter だけ
    expect(parseAccelerator("Return")).toBeNull();
    expect(parseAccelerator("Cmd+Return")).toBeNull();
    expect(parseAccelerator("Cmd+Enter")?.key).toBe("Enter");
  });

  it("Rust_の_KeyChord_と同じくキーの後ろに修飾キーを書いた形は読まない", () => {
    expect(parseAccelerator("A+Cmd")).toBeNull();
    expect(parseAccelerator("Cmd+N+Shift")).toBeNull();
    expect(parseAccelerator("Enter+Cmd")).toBeNull();
  });
});

describe("matchesAccelerator(押されたキーと表のキーを比べる)", () => {
  it("文字のキーは大文字小文字によらず一致し、修飾キーは過不足なく一致する", () => {
    const cmdN = parseAccelerator("Cmd+N");
    if (cmdN === null) throw new Error("読めない");
    expect(matchesAccelerator(cmdN, chord("n", { metaKey: true }))).toBe(true);
    expect(matchesAccelerator(cmdN, chord("N", { metaKey: true }))).toBe(true);
    expect(matchesAccelerator(cmdN, chord("n", { metaKey: true, shiftKey: true }))).toBe(false);
    expect(matchesAccelerator(cmdN, chord("n"))).toBe(false);
  });

  it("option_を含むキーは押した文字が変わっても一致する", () => {
    // ⌥ を押すと key は別の文字になる(US 配列で ⌥⌘R は「®」、⇧⌥⌘C は「Ç」)。物理的な位置(code)で比べる
    const reveal = parseAccelerator("Cmd+Alt+R");
    const relative = parseAccelerator("Cmd+Shift+Alt+C");
    if (reveal === null || relative === null) throw new Error("読めない");
    expect(matchesAccelerator(reveal, chord("®", { metaKey: true, altKey: true, code: "KeyR" }))).toBe(true);
    expect(
      matchesAccelerator(relative, chord("Ç", { metaKey: true, altKey: true, shiftKey: true, code: "KeyC" })),
    ).toBe(true);
    // code が違えば、key が同じ文字でも一致しない
    expect(matchesAccelerator(reveal, chord("r", { metaKey: true, altKey: true, code: "KeyT" }))).toBe(false);
    // code が無ければ一致しない
    expect(matchesAccelerator(reveal, chord("r", { metaKey: true, altKey: true }))).toBe(false);
  });

  it("名前のキーは KeyboardEvent の key の名前で比べる", () => {
    const enter = parseAccelerator("Enter");
    const trash = parseAccelerator("Cmd+Backspace");
    const insert = parseAccelerator("Cmd+Enter");
    if (enter === null || trash === null || insert === null) throw new Error("読めない");
    expect(matchesAccelerator(enter, chord("Enter"))).toBe(true);
    expect(matchesAccelerator(enter, chord("Enter", { metaKey: true }))).toBe(false);
    expect(matchesAccelerator(trash, chord("Backspace", { metaKey: true }))).toBe(true);
    expect(matchesAccelerator(insert, chord("Enter", { metaKey: true }))).toBe(true);
  });

  it("⇧ を押した記号のキーは、配列によって変わる ⇧ 付きの文字でも一致する", () => {
    const previous = parseAccelerator("Cmd+Shift+[");
    if (previous === null) throw new Error("読めない");
    expect(matchesAccelerator(previous, chord("[", { metaKey: true, shiftKey: true }))).toBe(true);
    expect(matchesAccelerator(previous, chord("{", { metaKey: true, shiftKey: true }))).toBe(true);
    expect(matchesAccelerator(previous, chord("}", { metaKey: true, shiftKey: true }))).toBe(false);
  });

  it("option_と記号や数字の組み合わせも物理的な位置で比べる", () => {
    const bracket = parseAccelerator("Cmd+Alt+[");
    const digit = parseAccelerator("Cmd+Alt+1");
    if (bracket === null || digit === null) throw new Error("読めない");
    expect(matchesAccelerator(bracket, chord("“", { metaKey: true, altKey: true, code: "BracketLeft" }))).toBe(true);
    expect(matchesAccelerator(digit, chord("¡", { metaKey: true, altKey: true, code: "Digit1" }))).toBe(true);
  });
});

describe("formatAccelerator(メニューに出すキーの表示)", () => {
  it("修飾キーは macOS の記号と順(⌃⌥⇧⌘)で、名前のキーは記号で出す", () => {
    expect(formatAccelerator("Cmd+O")).toBe("⌘O");
    expect(formatAccelerator("Cmd+Alt+R")).toBe("⌥⌘R");
    expect(formatAccelerator("Cmd+Shift+Alt+C")).toBe("⌥⇧⌘C");
    expect(formatAccelerator("Cmd+Enter")).toBe("⌘↩");
    expect(formatAccelerator("Enter")).toBe("↩");
    expect(formatAccelerator("Cmd+Backspace")).toBe("⌘⌫");
    expect(formatAccelerator("Cmd+Shift+N")).toBe("⇧⌘N");
    expect(formatAccelerator("Ctrl+Cmd+[")).toBe("⌃⌘[");
  });

  it("読めない文字列は表示しない(null)", () => {
    expect(formatAccelerator("Hyper+A")).toBeNull();
  });
});

describe("表の引き方", () => {
  it("コマンドの id からキーの文字列と表示を引ける", () => {
    expect(acceleratorOf(DEFAULT_SHORTCUT_TABLE, "tree.trash")).toBe("Cmd+Backspace");
    expect(shortcutLabel(DEFAULT_SHORTCUT_TABLE, "tree.copy-absolute-path")).toBe("⌥⌘C");
  });

  it("表に無いコマンドの id は null", () => {
    expect(acceleratorOf([], "tree.trash")).toBeNull();
    expect(shortcutLabel([], "tree.trash")).toBeNull();
  });

  it("見本の表はすべての行が読める", () => {
    for (const binding of DEFAULT_SHORTCUT_TABLE) {
      expect(parseAccelerator(binding.accelerator), binding.command).not.toBeNull();
    }
  });
});
