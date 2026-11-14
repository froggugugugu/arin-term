import { describe, expect, it } from "vitest";

import { sampleSnapshot } from "../test/fakeIpc";
import { createAppStore } from "./appStore";

describe("appStore", () => {
  it("作った直後は写しを持たない", () => {
    const store = createAppStore();

    expect(store.getState().snapshot).toBeNull();
  });

  it("届いた写しで丸ごと置き換わる", () => {
    const store = createAppStore();
    const first = sampleSnapshot({ appVersion: "1.0.0" });
    const second = sampleSnapshot({ schemaVersion: 2, appVersion: "2.0.0" });

    store.getState().replaceSnapshot(first);
    store.getState().replaceSnapshot(second);

    // 中身を混ぜずに、届いたオブジェクトそのものに置き換わる。
    expect(store.getState().snapshot).toBe(second);
    expect(store.getState().snapshot).toEqual({ schemaVersion: 2, appVersion: "2.0.0", terminals: [] });
  });

  it("置き換えると購読者に 1 回だけ知らせる", () => {
    const store = createAppStore();
    const seen: unknown[] = [];
    store.subscribe((state) => seen.push(state.snapshot));
    const snapshot = sampleSnapshot();

    store.getState().replaceSnapshot(snapshot);

    expect(seen).toEqual([snapshot]);
  });

  it("作った直後は起動の失敗を持たず、記録すると原因が残る", () => {
    const store = createAppStore();
    expect(store.getState().startupError).toBeNull();

    store.getState().reportStartupError("状態の写しを取得できなかった: x");

    expect(store.getState().startupError).toBe("状態の写しを取得できなかった: x");
  });

  it("生成関数ごとに別のストアになる", () => {
    const a = createAppStore();
    const b = createAppStore();

    a.getState().replaceSnapshot(sampleSnapshot());

    expect(b.getState().snapshot).toBeNull();
  });
});
