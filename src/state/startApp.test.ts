import { describe, expect, it } from "vitest";

import { createIpc } from "../ipc/client";
import type { ShortcutTable } from "../ipc/generated/ShortcutTable";
import type { TreeChange } from "../ipc/generated/TreeChange";
import { DEFAULT_SHORTCUT_TABLE } from "../logic/keys/defaultShortcuts";
import { createFakeIpc, deferred, sampleSnapshot } from "../test/fakeIpc";
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
      unsaved: [],
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

  it("エディタを閉じる確認の要求を準備完了の通知より先に購読して受け手に渡す", async () => {
    let listenersAtReady = -1;
    let fakeRef: ReturnType<typeof setup>["fake"] | undefined;
    const ctx = setup({
      notify_app_ready: () => {
        listenersAtReady = fakeRef?.listenerCount("editor-close-requested") ?? 0;
        return null;
      },
    });
    fakeRef = ctx.fake;
    const seen: string[] = [];

    await ctx.run({ onEditorCloseRequested: (request) => seen.push(request.editorId) });
    ctx.fake.emit("editor-close-requested", { editorId: "editor-1" });

    expect(listenersAtReady).toBe(1);
    expect(seen).toEqual(["editor-1"]);
  });
});

describe("startApp(設定の表と知らせへの追従。T65e)", () => {
  /** 応答を待つあいだに積まれた処理を流す。 */
  const flush = () => new Promise((resolve) => setTimeout(resolve, 0));
  const withKey = (command: string, accelerator: string): ShortcutTable =>
    DEFAULT_SHORTCUT_TABLE.map((b) => (b.command === command ? { ...b, accelerator } : { ...b }));

  /** 応答を差し替えられる偽の通信で起動する。 */
  async function boot(initial: { table: ShortcutTable; notice: string | null }) {
    const current = { ...initial, failTable: false, failNotice: false };
    let listenersAtReady = -1;
    let fakeRef: ReturnType<typeof createFakeIpc> | undefined;
    const fake = createFakeIpc({
      get_shortcuts: () => {
        if (current.failTable) throw "command get_shortcuts failed";
        return current.table;
      },
      get_settings_notice: () => {
        if (current.failNotice) throw "command get_settings_notice failed";
        return current.notice;
      },
      notify_app_ready: () => {
        listenersAtReady = fakeRef?.listenerCount("settings-changed") ?? -1;
        return null;
      },
    });
    fakeRef = fake;
    const tables: ShortcutTable[] = [];
    const notices: (string | null)[] = [];
    const reported: string[] = [];
    const result = await startApp({
      ipc: createIpc(fake.transport),
      store: createAppStore(),
      mount: () => {},
      logError: () => {},
      onShortcuts: (t) => tables.push(t),
      onSettingsNotice: (n) => notices.push(n),
      reportError: (m) => reported.push(m),
    });
    return { fake, current, tables, notices, reported, result, listenersAtReady: () => listenersAtReady };
  }

  it("起動時に表と設定の知らせを受け取り_設定の変更を準備完了の通知より先に購読する", async () => {
    const table = withKey("terminal.new-tab", "Cmd+Shift+Y");
    const ctx = await boot({ table, notice: "ショートカットの設定の 1 行を使わず、既定のキーのままにしました: x" });

    expect(ctx.result).toEqual({ ok: true });
    expect(ctx.tables).toEqual([table]);
    expect(ctx.notices).toEqual(["ショートカットの設定の 1 行を使わず、既定のキーのままにしました: x"]);
    expect(ctx.fake.calls.map((c) => c.command)).toEqual([
      "get_app_snapshot",
      "get_shortcuts",
      "get_settings_notice",
      "notify_app_ready",
    ]);
    expect(ctx.listenersAtReady()).toBe(1);
  });

  it("設定が変わったと届くと表と知らせを取り直して置き換える", async () => {
    const ctx = await boot({ table: withKey("terminal.new-tab", "Cmd+T"), notice: null });
    ctx.fake.calls.length = 0;

    const changed = withKey("terminal.new-tab", "Cmd+Shift+Y");
    ctx.current.table = changed;
    ctx.current.notice = "設定ファイルの 3 行 5 列目を読めないため、直前の設定のままです";
    ctx.fake.emit("settings-changed", null);
    await flush();

    expect(ctx.fake.calls.map((c) => c.command)).toEqual(["get_shortcuts", "get_settings_notice"]);
    expect(ctx.tables.at(-1)).toEqual(changed);
    expect(ctx.notices.at(-1)).toBe("設定ファイルの 3 行 5 列目を読めないため、直前の設定のままです");

    // 知らせだけが消えたときも届く(表は同じまま置き換える)
    ctx.current.notice = null;
    ctx.fake.emit("settings-changed", null);
    await flush();
    expect(ctx.notices.at(-1)).toBeNull();
    expect(ctx.tables).toHaveLength(3);
    expect(ctx.reported).toEqual([]);
  });

  it("設定の変更が続けて届くと最後の取り直しの結果だけを当てる", async () => {
    const first = deferred<ShortcutTable>();
    const tables: ShortcutTable[] = [];
    let call = 0;
    const latest = withKey("terminal.new-tab", "Cmd+Shift+U");
    const fake = createFakeIpc({
      get_shortcuts: () => {
        call += 1;
        if (call === 1) return DEFAULT_SHORTCUT_TABLE.map((b) => ({ ...b }));
        // 1 回目の取り直しは遅れて届く(古い応答)
        if (call === 2) return first.promise;
        return latest;
      },
    });
    await startApp({
      ipc: createIpc(fake.transport),
      store: createAppStore(),
      mount: () => {},
      logError: () => {},
      onShortcuts: (t) => tables.push(t),
    });

    fake.emit("settings-changed", null);
    fake.emit("settings-changed", null);
    await flush();
    first.resolve(withKey("terminal.new-tab", "Cmd+Shift+O"));
    await flush();

    expect(tables).toHaveLength(2);
    expect(tables.at(-1)).toEqual(latest);
  });

  it("取り直せないときは今の表のまま", async () => {
    const ctx = await boot({ table: withKey("terminal.new-tab", "Cmd+T"), notice: "前の知らせ" });

    ctx.current.table = withKey("terminal.new-tab", "Cmd+Shift+Y");
    ctx.current.failTable = true;
    ctx.fake.emit("settings-changed", null);
    await flush();

    // 表も知らせも置き換えない(片方だけ当てない)
    expect(ctx.tables).toHaveLength(1);
    expect(ctx.notices).toEqual(["前の知らせ"]);
    expect(ctx.reported).toEqual([
      "設定の変更を取り直せなかったので、今の表のまま動かす: command get_shortcuts failed",
    ]);

    // 知らせだけを取れなくても、表を置き換えない
    ctx.current.failTable = false;
    ctx.current.failNotice = true;
    ctx.fake.emit("settings-changed", null);
    await flush();
    expect(ctx.tables).toHaveLength(1);
    expect(ctx.reported.at(-1)).toBe(
      "設定の変更を取り直せなかったので、今の表のまま動かす: command get_settings_notice failed",
    );

    // 直れば次の知らせで置き換わる
    ctx.current.failNotice = false;
    ctx.fake.emit("settings-changed", null);
    await flush();
    expect(ctx.tables.at(-1)).toEqual(withKey("terminal.new-tab", "Cmd+Shift+Y"));
  });

  it("起動時に設定の知らせを取得できなくても起動し_表は受け取って報告だけする", async () => {
    const fake = createFakeIpc({
      get_settings_notice: () => {
        throw "command get_settings_notice not found";
      },
    });
    const reported: string[] = [];
    const tables: ShortcutTable[] = [];
    const notices: (string | null)[] = [];
    const result = await startApp({
      ipc: createIpc(fake.transport),
      store: createAppStore(),
      mount: () => {},
      logError: () => {},
      onShortcuts: (t) => tables.push(t),
      onSettingsNotice: (n) => notices.push(n),
      reportError: (m) => reported.push(m),
    });

    expect(result).toEqual({ ok: true });
    expect(tables).toHaveLength(1);
    expect(notices).toEqual([]);
    expect(reported).toEqual(["設定の知らせを取得できなかった: command get_settings_notice not found"]);
  });

  it("設定の変更を購読できなくても起動し_報告だけする", async () => {
    const fake = createFakeIpc();
    fake.failListenOf = { event: "settings-changed", cause: "not allowed" };
    const reported: string[] = [];
    const tables: ShortcutTable[] = [];

    const result = await startApp({
      ipc: createIpc(fake.transport),
      store: createAppStore(),
      mount: () => {},
      logError: () => {},
      onShortcuts: (t) => tables.push(t),
      reportError: (m) => reported.push(m),
    });

    expect(result).toEqual({ ok: true });
    expect(tables).toHaveLength(1);
    expect(reported).toEqual(["設定の変更を購読できなかったので、起動時の表のまま動かす: not allowed"]);
  });
});

describe("startApp(新しい版の確認の知らせ。ADR-018)", () => {
  it("update-changed を購読してから今の知らせを取り_準備完了の前に受け手へ渡す", async () => {
    const fake = createFakeIpc({ get_update_notice: () => ({ kind: "available", version: "0.4.0" }) });
    const notices: unknown[] = [];
    const result = await startApp({
      ipc: createIpc(fake.transport),
      store: createAppStore(),
      mount: () => {},
      logError: () => {},
      onUpdateNotice: (n) => notices.push(n),
    });

    expect(result).toEqual({ ok: true });
    expect(notices).toEqual([{ kind: "available", version: "0.4.0" }]);
    expect(fake.listenerCount("update-changed")).toBe(1);
    expect(fake.calls.map((c) => c.command)).toEqual(["get_app_snapshot", "get_update_notice", "notify_app_ready"]);

    fake.emit("update-changed", { kind: "upToDate", version: "0.4.0" });
    expect(notices.at(-1)).toEqual({ kind: "upToDate", version: "0.4.0" });
  });

  it("知らせを取れなくても起動は止めず_報告だけする", async () => {
    const fake = createFakeIpc({
      get_update_notice: () => Promise.reject(new Error("壊れた")),
    });
    const reported: string[] = [];
    const result = await startApp({
      ipc: createIpc(fake.transport),
      store: createAppStore(),
      mount: () => {},
      logError: () => {},
      onUpdateNotice: () => {},
      reportError: (m) => reported.push(m),
    });

    expect(result).toEqual({ ok: true });
    expect(reported).toHaveLength(1);
    expect(reported[0]).toContain("新しい版の知らせを取得できなかった");
  });
});
