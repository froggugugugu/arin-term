import { describe, expect, it } from "vitest";

import { createIpc } from "../ipc/client";
import { createFakeIpc, sampleSnapshot } from "../test/fakeIpc";
import { createAppStore } from "./appStore";
import { startApp } from "./startApp";

/** 起動の手順を、偽の通信と記録用の描画・記録用のログで動かす。 */
function setup(handlers: Parameters<typeof createFakeIpc>[0] = {}) {
  const fake = createFakeIpc(handlers);
  const store = createAppStore();
  const events: string[] = [];
  const logged: unknown[][] = [];
  const run = () =>
    startApp({
      ipc: createIpc(fake.transport),
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
        ipc: createIpc(fake.transport),
        store,
        mount: () => {
          throw new Error("#root が無い");
        },
        logError: () => {},
      }),
    ).rejects.toThrow("#root が無い");

    expect(fake.calls.map((c) => c.command)).toEqual(["get_app_snapshot"]);
  });

  it("写しの更新のイベントを取得より先に購読し、届いた写しでストアを置き換える", async () => {
    const { fake, store, run } = setup({ get_app_snapshot: () => sampleSnapshot({ appVersion: "1" }) });

    await run();
    expect(fake.listenerCount("app-snapshot")).toBe(1);
    fake.emit("app-snapshot", sampleSnapshot({ appVersion: "2" }));

    expect(store.getState().snapshot?.appVersion).toBe("2");
  });

  it("取得の応答より先にイベントで写しが届いたら、取得の応答で上書きしない", async () => {
    let fakeRef: ReturnType<typeof setup>["fake"] | undefined;
    const ctx = setup({
      get_app_snapshot: () => {
        // 取得の応答が画面に届く前に、新しい写しがイベントで届いた場合。
        fakeRef?.emit("app-snapshot", sampleSnapshot({ appVersion: "新しい" }));
        return sampleSnapshot({ appVersion: "古い" });
      },
    });
    fakeRef = ctx.fake;

    await ctx.run();

    expect(ctx.store.getState().snapshot?.appVersion).toBe("新しい");
  });

  it("購読に失敗したら、描画して原因を残し、写しの取得も準備完了の通知もしない", async () => {
    const { fake, store, events, run } = setup();
    fake.failListen = "Command plugin:event|listen not allowed by ACL";

    const result = await run();

    expect(result).toEqual({
      ok: false,
      error: "状態の更新を購読できなかった: Command plugin:event|listen not allowed by ACL",
    });
    expect(store.getState().startupError).toBe(
      "状態の更新を購読できなかった: Command plugin:event|listen not allowed by ACL",
    );
    expect(fake.calls).toEqual([]);
    expect(events).toEqual(["mount(snapshot=null)"]);
  });
});
