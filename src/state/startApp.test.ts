import { describe, expect, it } from "vitest";

import { createIpc } from "../ipc/client";
import type { ShortcutTable } from "../ipc/generated/ShortcutTable";
import type { TreeChange } from "../ipc/generated/TreeChange";
import { DEFAULT_SHORTCUT_TABLE } from "../logic/keys/defaultShortcuts";
import { createFakeIpc, sampleSnapshot } from "../test/fakeIpc";
import { createAppStore } from "./appStore";
import { type StartAppDeps, startApp } from "./startApp";

/** 起動の手順を、偽の通信と記録用の描画・記録用のログで動かす。 */
function setup(handlers: Parameters<typeof createFakeIpc>[0] = {}) {
  const fake = createFakeIpc(handlers);
  const store = createAppStore();
  const events: string[] = [];
  const logged: unknown[][] = [];
  const run = (listeners: NonNullable<StartAppDeps["events"]> = {}) =>
    startApp({
      ipc: createIpc(fake.transport),
      store,
      events: listeners,
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

  it("メニューの操作と終了の確認の要求を、準備完了の通知より先に購読して受け手に渡す", async () => {
    let listenersAtReady = -1;
    let fakeRef: ReturnType<typeof setup>["fake"] | undefined;
    const ctx = setup({
      notify_app_ready: () => {
        listenersAtReady =
          (fakeRef?.listenerCount("menu-command") ?? 0) + (fakeRef?.listenerCount("quit-requested") ?? 0);
        return null;
      },
    });
    fakeRef = ctx.fake;
    const seen: string[] = [];

    await ctx.run({
      onMenuCommand: (command) => seen.push(`menu ${command}`),
      onQuitRequested: (request) => seen.push(`quit ${request.running.map((r) => r.terminalId).join(",")}`),
    });
    ctx.fake.emit("menu-command", "select-all");
    ctx.fake.emit("quit-requested", {
      running: [{ terminalId: "t1", projectId: null, projectName: null, label: "ターミナル 1" }],
    });

    expect(listenersAtReady).toBe(2);
    expect(seen).toEqual(["menu select-all", "quit t1"]);
  });

  it("受け手を渡さなければ、メニューの操作と終了の確認の要求を購読しない", async () => {
    const { fake, run } = setup();

    await run();

    expect(fake.listenerCount("menu-command")).toBe(0);
    expect(fake.listenerCount("quit-requested")).toBe(0);
  });

  it("終了の確認の要求を購読できなかったら、原因を残し、準備完了を通知しない", async () => {
    const { fake, store, run } = setup();
    fake.failListenOf = { event: "quit-requested", cause: "not allowed" };

    const result = await run({ onQuitRequested: () => {} });

    expect(result).toEqual({ ok: false, error: "終了の確認の要求を購読できなかった: not allowed" });
    expect(store.getState().startupError).toBe("終了の確認の要求を購読できなかった: not allowed");
    expect(fake.calls).toEqual([]);
  });

  it("メニューの操作を購読できなかったら、原因を残し、準備完了を通知しない", async () => {
    const { fake, run } = setup();
    fake.failListenOf = { event: "menu-command", cause: "not allowed" };

    const result = await run({ onMenuCommand: () => {} });

    expect(result).toEqual({ ok: false, error: "メニューの操作を購読できなかった: not allowed" });
    expect(fake.calls).toEqual([]);
  });

  it("起動時にショートカットの表を受け取る", async () => {
    const table: ShortcutTable = DEFAULT_SHORTCUT_TABLE.map((b) =>
      b.command === "tree.new-file" ? { ...b, accelerator: "Cmd+Alt+N" } : { ...b },
    );
    const { fake, run } = setup({ get_shortcuts: () => table });
    const received: ShortcutTable[] = [];

    const result = await startApp({
      ipc: createIpc(fake.transport),
      store: createAppStore(),
      mount: () => {},
      logError: () => {},
      onShortcuts: (t) => received.push(t),
    });

    expect(result).toEqual({ ok: true });
    expect(received).toEqual([table]);
    // 写しのあと、準備完了の通知の前に受け取る。
    expect(fake.calls.map((c) => c.command)).toEqual(["get_app_snapshot", "get_shortcuts", "notify_app_ready"]);
    // 受け手を渡さなければ取得しない(既存の起動の手順のまま)。
    fake.calls.length = 0;
    await run();
    expect(fake.calls.map((c) => c.command)).toEqual(["get_app_snapshot", "notify_app_ready"]);
  });

  it("ショートカットの表を取得できなくても起動し_報告だけする", async () => {
    const fake = createFakeIpc({
      get_shortcuts: () => {
        throw "command get_shortcuts not found";
      },
    });
    const reported: string[] = [];
    const received: unknown[] = [];

    const result = await startApp({
      ipc: createIpc(fake.transport),
      store: createAppStore(),
      mount: () => {},
      logError: () => {},
      onShortcuts: (t) => received.push(t),
      reportError: (m) => reported.push(m),
    });

    expect(result).toEqual({ ok: true });
    expect(received).toEqual([]);
    expect(reported).toEqual([
      "ショートカットの表を取得できなかったので、既定の表で動かす: command get_shortcuts not found",
    ]);
    expect(fake.calls.map((c) => c.command)).toContain("notify_app_ready");
  });

  it("ツリーの変更を準備完了の通知より先に購読して受け手に渡す", async () => {
    let listenersAtReady = -1;
    let fakeRef: ReturnType<typeof setup>["fake"] | undefined;
    const ctx = setup({
      notify_app_ready: () => {
        listenersAtReady = fakeRef?.listenerCount("tree-changed") ?? 0;
        return null;
      },
    });
    fakeRef = ctx.fake;
    const seen: TreeChange[] = [];

    await ctx.run({ onTreeChanged: (change) => seen.push(change) });
    const change: TreeChange = { projectId: "p1", dirs: ["src"], fullReload: false };
    ctx.fake.emit("tree-changed", change);

    expect(listenersAtReady).toBe(1);
    expect(seen).toEqual([change]);
  });

  it("ツリーの変更を購読できなかったら、原因を残し、準備完了を通知しない", async () => {
    const { fake, run } = setup();
    fake.failListenOf = { event: "tree-changed", cause: "not allowed" };

    const result = await run({ onTreeChanged: () => {} });

    expect(result).toEqual({ ok: false, error: "ツリーの変更を購読できなかった: not allowed" });
    expect(fake.calls).toEqual([]);
  });
});
