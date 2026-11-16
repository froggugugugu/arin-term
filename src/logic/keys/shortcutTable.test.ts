import { describe, expect, it } from "vitest";

import { DEFAULT_SHORTCUT_TABLE } from "./defaultShortcuts";
import type { KeyChord } from "./keyOwner";
import {
  acceleratorOf,
  ariaKeyShortcuts,
  formatAccelerator,
  formatAriaKeyShortcut,
  matchesAccelerator,
  parseAccelerator,
  type ShortcutCommandId,
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

  it("既定の表に表示の_4_行とタイルの番号キーの_9_行がある", () => {
    const view = DEFAULT_SHORTCUT_TABLE.filter(
      (binding) => binding.command.startsWith("view.") && !binding.command.startsWith("view.focus-tile-"),
    );
    expect(view).toEqual([
      { command: "view.toggle-display-mode", accelerator: "Cmd+Shift+G", receiver: "menu-item" },
      { command: "view.previous-page", accelerator: "Cmd+Alt+ArrowLeft", receiver: "menu-item" },
      { command: "view.next-page", accelerator: "Cmd+Alt+ArrowRight", receiver: "menu-item" },
      { command: "view.toggle-maximize", accelerator: "Cmd+Shift+Enter", receiver: "menu-item" },
    ]);
    // 表の最後の 13 行は表示の 4 行とタイルの番号キーの 9 行(Rust の表と同じ順。ADR-014 の 4)
    expect(DEFAULT_SHORTCUT_TABLE.slice(-13, -9)).toEqual(view);
    expect(DEFAULT_SHORTCUT_TABLE.slice(-9).map((binding) => binding.command)).toEqual(
      Array.from({ length: 9 }, (_, i) => `view.focus-tile-${i + 1}`),
    );
    // ツールチップに出すキーの表示(T47f が表から引く)
    expect(shortcutLabel(DEFAULT_SHORTCUT_TABLE, "view.toggle-display-mode")).toBe("⇧⌘G");
    expect(shortcutLabel(DEFAULT_SHORTCUT_TABLE, "view.previous-page")).toBe("⌥⌘←");
    expect(shortcutLabel(DEFAULT_SHORTCUT_TABLE, "view.next-page")).toBe("⌥⌘→");
    expect(shortcutLabel(DEFAULT_SHORTCUT_TABLE, "view.toggle-maximize")).toBe("⇧⌘↩");
    // ⌥ を含むページ送りは物理的な位置で一致する
    const previous = parseAccelerator("Cmd+Alt+ArrowLeft");
    if (previous === null) throw new Error("読めない");
    expect(previous).toEqual({ meta: true, shift: false, alt: true, ctrl: false, key: "ArrowLeft" });
    expect(
      matchesAccelerator(previous, chord("ArrowLeft", { metaKey: true, altKey: true, code: "ArrowLeft" })),
    ).toBe(true);
  });

  it("既定の表に_32_行がある", () => {
    // M4.7 でタイルの番号キー 9 行を足した(ADR-014 の 4)
    expect(DEFAULT_SHORTCUT_TABLE).toHaveLength(32);
    // コマンドの id は 1 行に 1 つ
    expect(new Set(DEFAULT_SHORTCUT_TABLE.map((binding) => binding.command)).size).toBe(32);
    // 設定は設定ファイルを開く(⌘,。計画 T64c)。表の順では終了のすぐ後ろ
    const index = DEFAULT_SHORTCUT_TABLE.findIndex((binding) => binding.command === "app.open-settings");
    expect(DEFAULT_SHORTCUT_TABLE[index]).toEqual({
      command: "app.open-settings",
      accelerator: "Cmd+,",
      receiver: "menu-item",
    });
    expect(DEFAULT_SHORTCUT_TABLE[index - 1]?.command).toBe("app.quit");
    expect(shortcutLabel(DEFAULT_SHORTCUT_TABLE, "app.open-settings")).toBe("⌘,");
    expect(parseAccelerator("Cmd+,")).toEqual({ meta: true, shift: false, alt: false, ctrl: false, key: "," });
  });

  it("タイルの番号キーは_⌘1〜⌘9_でメニューの項目が受ける", () => {
    for (let n = 1; n <= 9; n += 1) {
      const command = `view.focus-tile-${n}` as ShortcutCommandId;
      expect(DEFAULT_SHORTCUT_TABLE.find((binding) => binding.command === command)).toEqual({
        command,
        accelerator: `Cmd+${n}`,
        receiver: "menu-item",
      });
      expect(shortcutLabel(DEFAULT_SHORTCUT_TABLE, command)).toBe(`⌘${n}`);
    }
  });

  it("見本の表はすべての行が読める", () => {
    for (const binding of DEFAULT_SHORTCUT_TABLE) {
      expect(parseAccelerator(binding.accelerator), binding.command).not.toBeNull();
    }
  });
});

describe("ariaKeyShortcuts(読み上げ用のキーの表示。aria-keyshortcuts の値)", () => {
  it.each([
    ["Cmd+T", "Meta+T"],
    ["Cmd+Shift+T", "Meta+Shift+T"],
    ["shift+cmd+t", "Meta+Shift+T"],
    ["Cmd+Alt+N", "Meta+Alt+N"],
    ["Option+Cmd+C", "Meta+Alt+C"],
    ["Ctrl+Shift+Tab", "Control+Shift+Tab"],
    ["Cmd+Ctrl+Alt+Shift+K", "Meta+Control+Alt+Shift+K"],
    ["Cmd+Backspace", "Meta+Backspace"],
    ["Cmd+Alt+ArrowLeft", "Meta+Alt+ArrowLeft"],
    ["Cmd+Shift+Enter", "Meta+Shift+Enter"],
    ["Cmd+Space", "Meta+Space"],
    ["Cmd+,", "Meta+,"],
    ["Cmd+Shift+[", "Meta+Shift+["],
    ["Cmd+1", "Meta+1"],
  ])("読み上げ用のキーの表示は_meta_の形(%s → %s)", (accelerator, expected) => {
    expect(formatAriaKeyShortcut(accelerator)).toBe(expected);
  });

  it("読めない文字列は表示しない(null)", () => {
    expect(formatAriaKeyShortcut("")).toBeNull();
    expect(formatAriaKeyShortcut("Cmd+")).toBeNull();
    expect(formatAriaKeyShortcut("Hyper+T")).toBeNull();
  });

  it("表からコマンドの_id_で引き_無い行は_null", () => {
    expect(ariaKeyShortcuts(DEFAULT_SHORTCUT_TABLE, "terminal.new-tab")).toBe("Meta+T");
    expect(ariaKeyShortcuts(DEFAULT_SHORTCUT_TABLE, "tree.copy-absolute-path")).toBe("Meta+Alt+C");
    const changed = DEFAULT_SHORTCUT_TABLE.map((b) =>
      b.command === "terminal.new-tab" ? { ...b, accelerator: "Cmd+Shift+Y" } : b,
    );
    expect(ariaKeyShortcuts(changed, "terminal.new-tab")).toBe("Meta+Shift+Y");
    expect(ariaKeyShortcuts([], "terminal.new-tab")).toBeNull();
  });
});
