import { describe, expect, it } from "vitest";

import { createIpc } from "../ipc/client";
import { createFakeIpc, sampleSnapshot } from "../test/fakeIpc";
import { createAppStore } from "./appStore";
import { describeError, startApp } from "./startApp";

/** 起動の手順を、偽の通信と記録用の描画・記録用のログで動かす。 */
function setup(handlers: Parameters<typeof createFakeIpc>[0] = {}) {
  const fake = createFakeIpc(handlers);
  const store = createAppStore();
  const events: string[] = [];
  const logged: unknown[][] = [];
  const run = () =>
    startApp({
      ipc: createIpc(fake.invoke),
      store,
      mount: () => {
        // 描いた時点で、ストアに写しが入っているかどうかも記録する。
        events.push(`mount(snapshot=${store.getState().snapshot === null ? "null" : "set"})`);
      },
      logError: (...args) => logged.push(args),
    });
  return { fake, store, events, logged, run };
}

describe("startApp(画面側の起動の手順)", () => {
  it("描画し、写しを取得してストアに入れてから、準備完了を 1 回通知する", async () => {
    const snapshot = sampleSnapshot({ appVersion: "1.2.3" });
    const { fake, store, events, logged, run } = setup({ get_app_snapshot: () => snapshot });

    const result = await run();

    expect(result).toEqual({ ok: true });
    expect(store.getState().snapshot).toBe(snapshot);
    expect(store.getState().startupError).toBeNull();
    expect(fake.calls.map((c) => c.command)).toEqual(["get_app_snapshot", "notify_app_ready"]);
    // 描画は写しの到着を待たない(届いたらストアの購読で描き直す)。描画は 1 回だけ。
    expect(events).toEqual(["mount(snapshot=null)"]);
    expect(logged).toEqual([]);
  });

  it("写しの取得に失敗したら、準備完了を通知せず、原因をストアとログに残して描画する", async () => {
    const { fake, store, events, logged, run } = setup({
      get_app_snapshot: () => {
        throw new Error("Rust が応答しない");
      },
    });

    const result = await run();

    expect(result).toEqual({ ok: false, error: "状態の写しを取得できなかった: Rust が応答しない" });
    expect(store.getState().snapshot).toBeNull();
    expect(store.getState().startupError).toBe("状態の写しを取得できなかった: Rust が応答しない");
    expect(fake.calls.map((c) => c.command)).toEqual(["get_app_snapshot"]);
    expect(events).toEqual(["mount(snapshot=null)"]);
    expect(logged).toHaveLength(1);
    expect(logged[0]?.[0]).toBe("状態の写しを取得できなかった: Rust が応答しない");
  });

  it("準備完了の通知に失敗したら、原因をストアとログに残す(写しはそのまま)", async () => {
    const snapshot = sampleSnapshot();
    const { store, logged, run } = setup({
      get_app_snapshot: () => snapshot,
      notify_app_ready: () => {
        // Tauri のコマンドが Err(String) を返すと、invoke は文字列で失敗する。
        throw "起動スモークの印を書けなかった";
      },
    });

    const result = await run();

    expect(result).toEqual({
      ok: false,
      error: "準備完了を通知できなかった: 起動スモークの印を書けなかった",
    });
    expect(store.getState().snapshot).toBe(snapshot);
    expect(store.getState().startupError).toBe(
      "準備完了を通知できなかった: 起動スモークの印を書けなかった",
    );
    expect(logged).toHaveLength(1);
  });

  it("描画が例外を投げたら、準備完了を通知せずにその例外を投げ直す", async () => {
    const fake = createFakeIpc();
    const store = createAppStore();

    await expect(
      startApp({
        ipc: createIpc(fake.invoke),
        store,
        mount: () => {
          throw new Error("#root が無い");
        },
        logError: () => {},
      }),
    ).rejects.toThrow("#root が無い");

    expect(fake.calls.map((c) => c.command)).toEqual(["get_app_snapshot"]);
  });
});

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
