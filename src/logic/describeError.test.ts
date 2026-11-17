import { describe, expect, it } from "vitest";

import { describeError } from "./describeError";

describe("describeError", () => {
  it("Error はメッセージにする", () => {
    expect(describeError(new Error("壊れた"))).toBe("壊れた");
  });

  it("文字列はそのまま使う", () => {
    expect(describeError("Rust からの失敗")).toBe("Rust からの失敗");
  });

  it("それ以外は JSON にする", () => {
    expect(describeError({ code: 1 })).toBe('{"code":1}');
  });

  it("JSON にできないものは String で文字にする", () => {
    expect(describeError(undefined)).toBe("undefined");
    expect(describeError(10n)).toBe("10");
  });
});
