import { describe, expect, it } from "vitest";

import { isShiftEnter, NEWLINE_INPUT, newlineInput, type NewlineKeyEvent } from "./newline";

function key(overrides: Partial<NewlineKeyEvent> = {}): NewlineKeyEvent {
  return {
    type: "keydown",
    key: "Enter",
    shiftKey: true,
    metaKey: false,
    ctrlKey: false,
    altKey: false,
    isComposing: false,
    keyCode: 13,
    ...overrides,
  };
}

describe("Shift+Enter の改行(ADR-014 の 5)", () => {
  it("送る文字は_ESC_と_CR", () => {
    expect(NEWLINE_INPUT).toBe("\x1b\r");
  });

  it("Shift_だけと_Enter_の_keydown_で_ESC_CR_を返す", () => {
    expect(newlineInput(key())).toBe("\x1b\r");
    expect(isShiftEnter(key())).toBe(true);
  });

  it("Shift_なしの_Enter_は対象外(CR_のまま_xterm_js_に任せる)", () => {
    expect(newlineInput(key({ shiftKey: false }))).toBeNull();
    expect(isShiftEnter(key({ shiftKey: false }))).toBe(false);
  });

  it("ほかの修飾キーつき(⌘⇧Enter_の拡大など)は対象外", () => {
    for (const mod of ["metaKey", "ctrlKey", "altKey"] as const) {
      expect(newlineInput(key({ [mod]: true })), mod).toBeNull();
      expect(isShiftEnter(key({ [mod]: true })), mod).toBe(false);
    }
  });

  it("IME_の変換中(isComposing_か_keyCode_229)は対象外", () => {
    expect(newlineInput(key({ isComposing: true }))).toBeNull();
    expect(newlineInput(key({ keyCode: 229 }))).toBeNull();
    expect(isShiftEnter(key({ isComposing: true }))).toBe(false);
  });

  it("Enter_以外のキーは対象外", () => {
    expect(newlineInput(key({ key: "A" }))).toBeNull();
    expect(newlineInput(key({ key: "Tab" }))).toBeNull();
  });

  it("keypress・keyup_では送らないが_同じ組み合わせとして見分ける(xterm_js_に既定の処理をさせないため)", () => {
    expect(newlineInput(key({ type: "keypress" }))).toBeNull();
    expect(newlineInput(key({ type: "keyup" }))).toBeNull();
    expect(isShiftEnter(key({ type: "keypress" }))).toBe(true);
    expect(isShiftEnter(key({ type: "keyup" }))).toBe(true);
  });
});
