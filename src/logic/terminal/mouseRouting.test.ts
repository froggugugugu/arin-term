import { describe, expect, it } from "vitest";

import { arinTakesMouse, middleClickAction } from "./mouseRouting";

describe("arinTakesMouse", () => {
  it("アプリがマウスを使っていなければ Arin が受ける", () => {
    expect(arinTakesMouse({ appOwnsMouse: false, altKey: false })).toBe(true);
  });
  it("アプリがマウスを使っていればアプリに渡し、Option を押していれば Arin が受ける", () => {
    expect(arinTakesMouse({ appOwnsMouse: true, altKey: false })).toBe(false);
    expect(arinTakesMouse({ appOwnsMouse: true, altKey: true })).toBe(true);
  });
});

describe("middleClickAction", () => {
  it("設定が ON で Arin が受けるときだけ貼る", () => {
    expect(middleClickAction({ enabled: true, appOwnsMouse: false, altKey: false })).toBe("paste");
    expect(middleClickAction({ enabled: true, appOwnsMouse: true, altKey: true })).toBe("paste");
  });
  it("設定が OFF・アプリがマウスを使っているときは任せる", () => {
    expect(middleClickAction({ enabled: false, appOwnsMouse: false, altKey: false })).toBe("pass");
    expect(middleClickAction({ enabled: true, appOwnsMouse: true, altKey: false })).toBe("pass");
  });
});
