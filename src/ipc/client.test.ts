import { describe, expect, it } from "vitest";

import { createFakeIpc, sampleSnapshot } from "../test/fakeIpc";
import { createIpc, toBytes } from "./client";
import type { DirListing } from "./generated/DirListing";
import type { TreeChange } from "./generated/TreeChange";

describe("src/ipc の包み", () => {
  it("写しの取得は get_app_snapshot を引数なしで呼び、届いた写しを返す", async () => {
    const snapshot = sampleSnapshot({ appVersion: "9.8.7" });
    const fake = createFakeIpc({ get_app_snapshot: () => snapshot });
    const ipc = createIpc(fake.transport);

    const result = await ipc.getAppSnapshot();

    expect(fake.calls).toEqual([{ command: "get_app_snapshot", args: undefined }]);
    expect(result).toBe(snapshot);
  });

  it("起動時の指定の取得は get_launch_options を引数なしで呼び、届いた指定を返す", async () => {
    const fake = createFakeIpc({ get_launch_options: () => ({ renderer: "webgl", unicode: "graphemes", verifyGrid: true }) });
    const ipc = createIpc(fake.transport);

    const result = await ipc.getLaunchOptions();

    expect(fake.calls).toEqual([{ command: "get_launch_options", args: undefined }]);
    expect(result).toEqual({ renderer: "webgl", unicode: "graphemes", verifyGrid: true });
  });

  it("準備完了の通知は notify_app_ready を引数なしで 1 回だけ呼ぶ", async () => {
    const fake = createFakeIpc();
    const ipc = createIpc(fake.transport);

    await expect(ipc.notifyAppReady()).resolves.toBeUndefined();

    expect(fake.calls).toEqual([{ command: "notify_app_ready", args: undefined }]);
  });

  it("Rust 側の失敗はそのまま呼び出し元に返す", async () => {
    const fake = createFakeIpc({
      get_app_snapshot: () => {
        throw new Error("失敗");
      },
    });
    const ipc = createIpc(fake.transport);

    await expect(ipc.getAppSnapshot()).rejects.toThrow("失敗");
  });

  it("画面の失敗の報告は report_frontend_error に message を渡す", async () => {
    const fake = createFakeIpc();
    const ipc = createIpc(fake.transport);

    await ipc.reportFrontendError("壊れた");

    expect(fake.calls).toEqual([{ command: "report_frontend_error", args: { message: "壊れた" } }]);
  });

  it("写しのイベントを購読し、届いた写しを渡し、やめる関数で外れる", async () => {
    const fake = createFakeIpc();
    const ipc = createIpc(fake.transport);
    const seen: string[] = [];

    const unlisten = await ipc.onAppSnapshot((s) => seen.push(s.appVersion));
    fake.emit("app-snapshot", sampleSnapshot({ appVersion: "1" }));
    unlisten();
    fake.emit("app-snapshot", sampleSnapshot({ appVersion: "2" }));

    expect(seen).toEqual(["1"]);
    expect(fake.listenerCount("app-snapshot")).toBe(0);
  });

  it("メニューの操作のイベント(menu-command)を購読し、届いた操作を渡し、やめる関数で外れる", async () => {
    const fake = createFakeIpc();
    const ipc = createIpc(fake.transport);
    const seen: string[] = [];

    const unlisten = await ipc.onMenuCommand((command) => seen.push(command));
    fake.emit("menu-command", "select-all");
    unlisten();
    fake.emit("menu-command", "select-all");

    expect(seen).toEqual(["select-all"]);
    expect(fake.listenerCount("menu-command")).toBe(0);
  });

  it("終了の確認の要求(quit-requested)を購読し、動作中の一覧を渡し、やめる関数で外れる", async () => {
    const fake = createFakeIpc();
    const ipc = createIpc(fake.transport);
    const seen: unknown[] = [];
    const running = [
      { terminalId: "terminal-1", projectId: "p1", projectName: "app", label: "ターミナル 1" },
    ];

    const unlisten = await ipc.onQuitRequested((request) => seen.push(request.running));
    fake.emit("quit-requested", { running });
    unlisten();
    fake.emit("quit-requested", { running: [] });

    expect(seen).toEqual([running]);
    expect(fake.listenerCount("quit-requested")).toBe(0);
  });

  it("ツリーの変更のイベント(tree-changed)を購読し、届いた中身を渡し、やめる関数で外れる", async () => {
    const fake = createFakeIpc();
    const ipc = createIpc(fake.transport);
    const seen: TreeChange[] = [];
    const change: TreeChange = { projectId: "p1", dirs: ["", "src"], fullReload: false };

    const unlisten = await ipc.onTreeChanged((c) => seen.push(c));
    fake.emit("tree-changed", change);
    unlisten();
    fake.emit("tree-changed", { projectId: "p1", dirs: [], fullReload: true });

    expect(seen).toEqual([change]);
    expect(fake.listenerCount("tree-changed")).toBe(0);
    expect(fake.calls).toEqual([]);
  });

  it("確認して終了は confirm_quit を引数なしで呼ぶ", async () => {
    const fake = createFakeIpc();
    const ipc = createIpc(fake.transport);

    await expect(ipc.confirmQuit()).resolves.toBeUndefined();

    expect(fake.calls).toEqual([{ command: "confirm_quit", args: undefined }]);
  });

  it("入力・処理済みの量・寸法はそれぞれのコマンドに camelCase の引数で届く", async () => {
    const fake = createFakeIpc();
    const ipc = createIpc(fake.transport);

    await ipc.writeTerminal("t1", "ls\r");
    await ipc.ackTerminalOutput("t1", 42);
    await ipc.resizeTerminal("t1", 30, 100);

    expect(fake.calls).toEqual([
      { command: "write_terminal", args: { terminalId: "t1", data: "ls\r" } },
      { command: "ack_terminal_output", args: { terminalId: "t1", bytes: 42 } },
      { command: "resize_terminal", args: { terminalId: "t1", rows: 30, cols: 100 } },
    ]);
  });
});

describe("タブの操作とバイト列の入力(T33b)", () => {
  it("新しいタブは new_terminal を引数なしで呼び、開いたタブの ID を返す", async () => {
    const fake = createFakeIpc({ new_terminal: () => "terminal-7" });
    const ipc = createIpc(fake.transport);

    await expect(ipc.newTerminal()).resolves.toBe("terminal-7");

    expect(fake.calls).toEqual([{ command: "new_terminal", args: undefined }]);
  });

  it("タブを閉じる・選ぶは close_terminal・select_terminal に camelCase の引数で届く", async () => {
    const fake = createFakeIpc();
    const ipc = createIpc(fake.transport);

    await expect(ipc.closeTerminal("t1")).resolves.toBeUndefined();
    await expect(ipc.selectTerminal("t2")).resolves.toBeUndefined();

    expect(fake.calls).toEqual([
      { command: "close_terminal", args: { terminalId: "t1" } },
      { command: "select_terminal", args: { terminalId: "t2" } },
    ]);
  });

  it("バイト列の入力は write_terminal_bytes に 0〜255 の数の配列で届く", async () => {
    const fake = createFakeIpc();
    const ipc = createIpc(fake.transport);

    await ipc.writeTerminalBytes("t1", new Uint8Array([27, 91, 77, 32, 255, 128, 0]));

    expect(fake.calls).toEqual([
      { command: "write_terminal_bytes", args: { terminalId: "t1", data: [27, 91, 77, 32, 255, 128, 0] } },
    ]);
  });

  it("タブの操作の失敗はそのまま呼び出し元に返す", async () => {
    const fake = createFakeIpc({
      new_terminal: () => {
        throw "プロジェクトが選ばれていないので、ターミナルを開けない";
      },
    });
    const ipc = createIpc(fake.transport);

    await expect(ipc.newTerminal()).rejects.toBe("プロジェクトが選ばれていないので、ターミナルを開けない");
  });
});

describe("プロジェクトの操作(T33c)", () => {
  const PROJECT = "3f2b8c1e-4d5a-4b6c-8d7e-9f0a1b2c3d4e";

  it("フォルダの追加は add_project_by_dialog を引数なしで呼び、結果をそのまま返す(パスは受け取らない)", async () => {
    const fake = createFakeIpc({ add_project_by_dialog: () => ({ kind: "added", projectId: PROJECT }) });
    const ipc = createIpc(fake.transport);

    await expect(ipc.addProjectByDialog()).resolves.toEqual({ kind: "added", projectId: PROJECT });

    expect(fake.calls).toEqual([{ command: "add_project_by_dialog", args: undefined }]);
  });

  it("選択・色の変更・パスのコピーはそれぞれのコマンドに camelCase の引数で届く", async () => {
    const fake = createFakeIpc();
    const ipc = createIpc(fake.transport);

    await expect(ipc.selectProject(PROJECT)).resolves.toBeUndefined();
    await expect(ipc.setProjectColor(PROJECT, "teal")).resolves.toBeUndefined();
    await expect(ipc.copyProjectPath(PROJECT)).resolves.toBeUndefined();

    expect(fake.calls).toEqual([
      { command: "select_project", args: { projectId: PROJECT } },
      { command: "set_project_color", args: { projectId: PROJECT, color: "teal" } },
      { command: "copy_project_path", args: { projectId: PROJECT } },
    ]);
  });

  it("フォルダの中身は list_directory にプロジェクトの ID と相対パスだけを渡し、応答をそのまま返す", async () => {
    const listing: DirListing = { kind: "listed", entries: [{ name: "src", kind: "directory" }] };
    const fake = createFakeIpc({ list_directory: ({ path }) => (path === "" ? listing : { kind: "notFound" }) });
    const ipc = createIpc(fake.transport);

    await expect(ipc.listDirectory(PROJECT, "")).resolves.toEqual(listing);
    await expect(ipc.listDirectory(PROJECT, "src/gone")).resolves.toEqual({ kind: "notFound" });

    expect(fake.calls).toEqual([
      { command: "list_directory", args: { projectId: PROJECT, path: "" } },
      { command: "list_directory", args: { projectId: PROJECT, path: "src/gone" } },
    ]);
  });

  it("フォルダの中身の既定の応答は空の一覧", async () => {
    const ipc = createIpc(createFakeIpc().transport);

    await expect(ipc.listDirectory(PROJECT, "")).resolves.toEqual({ kind: "listed", entries: [] });
  });

  it("隠しファイル表示の切替は set_show_hidden に camelCase の引数で届く", async () => {
    const fake = createFakeIpc();
    const ipc = createIpc(fake.transport);

    await expect(ipc.setShowHidden(PROJECT, true)).resolves.toBeUndefined();
    await expect(ipc.setShowHidden(PROJECT, false)).resolves.toBeUndefined();

    expect(fake.calls).toEqual([
      { command: "set_show_hidden", args: { projectId: PROJECT, showHidden: true } },
      { command: "set_show_hidden", args: { projectId: PROJECT, showHidden: false } },
    ]);
  });

  it("作成・名称変更・移動・コピーはそれぞれのコマンドに ID と相対パスと名前だけを渡し、対処が無ければキーごと省く", async () => {
    const fake = createFakeIpc({ create_entry: () => ({ kind: "done", select: "src/a.ts" }) });
    const ipc = createIpc(fake.transport);

    await expect(ipc.createEntry(PROJECT, "src", "a.ts", "file")).resolves.toEqual({
      kind: "done",
      select: "src/a.ts",
    });
    await expect(ipc.renameEntry(PROJECT, "src/a.ts", "b.ts")).resolves.toEqual({ kind: "done", select: null });
    await ipc.moveEntry(PROJECT, "a.md", "docs");
    await ipc.copyEntry(PROJECT, "a.md", "docs");

    expect(fake.calls).toEqual([
      { command: "create_entry", args: { projectId: PROJECT, parent: "src", name: "a.ts", kind: "file" } },
      { command: "rename_entry", args: { projectId: PROJECT, path: "src/a.ts", newName: "b.ts" } },
      { command: "move_entry", args: { projectId: PROJECT, source: "a.md", destDir: "docs" } },
      { command: "copy_entry", args: { projectId: PROJECT, source: "a.md", destDir: "docs" } },
    ]);
    for (const call of fake.calls) expect(call.args).not.toHaveProperty("resolution");
  });

  it("衝突のあとの呼び直しは対処を resolution で渡し、衝突の応答はそのまま返す", async () => {
    const fake = createFakeIpc({
      move_entry: ({ resolution }) =>
        resolution === undefined ? { kind: "conflict", name: "a.md" } : { kind: "done", select: "docs/a.md" },
    });
    const ipc = createIpc(fake.transport);

    await expect(ipc.moveEntry(PROJECT, "a.md", "docs")).resolves.toEqual({ kind: "conflict", name: "a.md" });
    await expect(ipc.moveEntry(PROJECT, "a.md", "docs", "replace")).resolves.toEqual({
      kind: "done",
      select: "docs/a.md",
    });
    await ipc.copyEntry(PROJECT, "a.md", "docs", "keepBoth");
    await ipc.createEntry(PROJECT, "", "notes", "folder", "keepBoth");
    await ipc.renameEntry(PROJECT, "b.md", "a.md", "replace");
    await ipc.pasteEntries(PROJECT, "docs", "keepBoth");

    expect(fake.calls).toEqual([
      { command: "move_entry", args: { projectId: PROJECT, source: "a.md", destDir: "docs" } },
      { command: "move_entry", args: { projectId: PROJECT, source: "a.md", destDir: "docs", resolution: "replace" } },
      { command: "copy_entry", args: { projectId: PROJECT, source: "a.md", destDir: "docs", resolution: "keepBoth" } },
      {
        command: "create_entry",
        args: { projectId: PROJECT, parent: "", name: "notes", kind: "folder", resolution: "keepBoth" },
      },
      { command: "rename_entry", args: { projectId: PROJECT, path: "b.md", newName: "a.md", resolution: "replace" } },
      { command: "paste_entries", args: { projectId: PROJECT, destDir: "docs", resolution: "keepBoth" } },
    ]);
  });

  it("ゴミ箱・コピー・切り取り・ペーストはそれぞれのコマンドに相対パスで届き、見つからないなどの応答をそのまま返す", async () => {
    const fake = createFakeIpc({ trash_entry: () => ({ kind: "notFound" }) });
    const ipc = createIpc(fake.transport);

    await expect(ipc.trashEntry(PROJECT, "gone.txt")).resolves.toEqual({ kind: "notFound" });
    await expect(ipc.copyEntries(PROJECT, ["a.md", "src"])).resolves.toEqual({ kind: "done", select: null });
    await expect(ipc.cutEntries(PROJECT, ["a.md"])).resolves.toEqual({ kind: "done", select: null });
    await expect(ipc.pasteEntries(PROJECT, "")).resolves.toEqual({ kind: "done", select: null });

    expect(fake.calls).toEqual([
      { command: "trash_entry", args: { projectId: PROJECT, path: "gone.txt" } },
      { command: "copy_entries", args: { projectId: PROJECT, paths: ["a.md", "src"] } },
      { command: "cut_entries", args: { projectId: PROJECT, paths: ["a.md"] } },
      { command: "paste_entries", args: { projectId: PROJECT, destDir: "" } },
    ]);
  });

  it("finder_で表示と開くは_reveal_entry_と_open_entry_に_id_と相対パスだけを渡し、応答をそのまま返す", async () => {
    const fake = createFakeIpc({ open_entry: () => ({ kind: "failed", message: "フォルダは開けません" }) });
    const ipc = createIpc(fake.transport);

    await expect(ipc.revealEntry(PROJECT, "")).resolves.toEqual({ kind: "done", select: null });
    await expect(ipc.revealEntry(PROJECT, "src/a b.ts")).resolves.toEqual({ kind: "done", select: null });
    await expect(ipc.openEntry(PROJECT, "src")).resolves.toEqual({ kind: "failed", message: "フォルダは開けません" });

    expect(fake.calls).toEqual([
      { command: "reveal_entry", args: { projectId: PROJECT, path: "" } },
      { command: "reveal_entry", args: { projectId: PROJECT, path: "src/a b.ts" } },
      { command: "open_entry", args: { projectId: PROJECT, path: "src" } },
    ]);
  });

  it("パスのコピーは_copy_entry_path_に相対か絶対かを渡し、パスは受け取らない", async () => {
    const fake = createFakeIpc();
    const ipc = createIpc(fake.transport);

    await expect(ipc.copyEntryPath(PROJECT, "src/a b.ts", "relative")).resolves.toEqual({
      kind: "done",
      select: null,
    });
    await ipc.copyEntryPath(PROJECT, "", "absolute");

    expect(fake.calls).toEqual([
      { command: "copy_entry_path", args: { projectId: PROJECT, path: "src/a b.ts", kind: "relative" } },
      { command: "copy_entry_path", args: { projectId: PROJECT, path: "", kind: "absolute" } },
    ]);
  });

  it("ターミナルへの挿入は_insert_entry_path_に相対パスと宛先の_id_だけを渡し、挿入する文字列を送らない", async () => {
    const fake = createFakeIpc({
      insert_entry_path: ({ terminalId }) =>
        terminalId === "gone" ? { kind: "failed", message: "挿入先のターミナルが見つかりません" } : { kind: "done", select: null },
    });
    const ipc = createIpc(fake.transport);

    await expect(ipc.insertEntryPath(PROJECT, ["src/a b.ts"], "terminal-1")).resolves.toEqual({
      kind: "done",
      select: null,
    });
    await expect(ipc.insertEntryPath(PROJECT, ["a.md"], "gone")).resolves.toEqual({
      kind: "failed",
      message: "挿入先のターミナルが見つかりません",
    });

    expect(fake.calls).toEqual([
      { command: "insert_entry_path", args: { projectId: PROJECT, paths: ["src/a b.ts"], terminalId: "terminal-1" } },
      { command: "insert_entry_path", args: { projectId: PROJECT, paths: ["a.md"], terminalId: "gone" } },
    ]);
    // 画面が作った文字列(エスケープ済みのパスなど)を PTY へ書くコマンドは呼ばない。
    expect(fake.calls.some((c) => c.command === "write_terminal" || c.command === "write_terminal_bytes")).toBe(false);
  });

  it("ファイル操作の Rust 側の失敗はそのまま呼び出し元に返す", async () => {
    const fake = createFakeIpc({
      rename_entry: () => {
        throw new Error("プロジェクトが見つからない");
      },
    });
    const ipc = createIpc(fake.transport);

    await expect(ipc.renameEntry(PROJECT, "a.md", "b.md")).rejects.toThrow("プロジェクトが見つからない");
  });

  it("削除は remove_project に確認の有無を渡し、確認が要るときは動作中の一覧を返す", async () => {
    const running = [{ terminalId: "terminal-1", projectId: PROJECT, projectName: "app", label: "ターミナル 1" }];
    const fake = createFakeIpc({
      remove_project: ({ confirmed }) =>
        confirmed ? { kind: "removed" } : { kind: "needsConfirmation", running },
    });
    const ipc = createIpc(fake.transport);

    await expect(ipc.removeProject(PROJECT, false)).resolves.toEqual({ kind: "needsConfirmation", running });
    await expect(ipc.removeProject(PROJECT, true)).resolves.toEqual({ kind: "removed" });

    expect(fake.calls).toEqual([
      { command: "remove_project", args: { projectId: PROJECT, confirmed: false } },
      { command: "remove_project", args: { projectId: PROJECT, confirmed: true } },
    ]);
  });

  it("再リンクは relink_project を呼び、つなぎ直したか(取り消しなら false)を返す", async () => {
    const fake = createFakeIpc({ relink_project: () => false });
    const ipc = createIpc(fake.transport);

    await expect(ipc.relinkProject(PROJECT)).resolves.toBe(false);

    expect(fake.calls).toEqual([{ command: "relink_project", args: { projectId: PROJECT } }]);
  });

  it("プロジェクトの操作の失敗はそのまま呼び出し元に返す", async () => {
    const fake = createFakeIpc({
      relink_project: () => {
        throw "このフォルダはプロジェクト x として登録済み";
      },
    });
    const ipc = createIpc(fake.transport);

    await expect(ipc.relinkProject(PROJECT)).rejects.toBe("このフォルダはプロジェクト x として登録済み");
  });
});

describe("ペインの寸法", () => {
  it("寸法は set_pane_sizes に camelCase の引数で届く", async () => {
    const fake = createFakeIpc();
    const ipc = createIpc(fake.transport);

    await expect(ipc.setPaneSizes(300, 240)).resolves.toBeUndefined();

    expect(fake.calls).toEqual([{ command: "set_pane_sizes", args: { sidebarWidth: 300, cardAreaHeight: 240 } }]);
  });

  it("折りたたみは set_sidebar_collapsed に collapsed で届く", async () => {
    const fake = createFakeIpc();
    const ipc = createIpc(fake.transport);

    await expect(ipc.setSidebarCollapsed(true)).resolves.toBeUndefined();
    await expect(ipc.setSidebarCollapsed(false)).resolves.toBeUndefined();

    expect(fake.calls).toEqual([
      { command: "set_sidebar_collapsed", args: { collapsed: true } },
      { command: "set_sidebar_collapsed", args: { collapsed: false } },
    ]);
  });
});

describe("connectTerminal", () => {
  it("接続の応答(寸法を反映したか)を connected で返す", async () => {
    const fake = createFakeIpc({ connect_terminal: () => ({ sizeApplied: false }) });
    const ipc = createIpc(fake.transport);

    const sub = ipc.connectTerminal("t1", 24, 80, () => {});

    await expect(sub.connected).resolves.toEqual({ sizeApplied: false });
  });

  it("チャネルを作って connect_terminal に渡し、届いた ArrayBuffer をバイト列で渡す", async () => {
    const fake = createFakeIpc();
    const ipc = createIpc(fake.transport);
    const received: number[][] = [];

    const sub = ipc.connectTerminal("t1", 24, 80, (data) => received.push([...data]));
    await sub.connected;
    const call = fake.calls[0];
    expect(call?.command).toBe("connect_terminal");
    const args = call?.args as { terminalId: string; rows: number; cols: number; onOutput: unknown };
    expect({ ...args, onOutput: undefined }).toEqual({
      terminalId: "t1",
      rows: 24,
      cols: 80,
      onOutput: undefined,
    });
    const channel = fake.channelOf(args.onOutput);
    expect(channel).toBeDefined();

    channel?.send(new Uint8Array([104, 105]).buffer);
    channel?.send("文字列は無視する");

    expect(received).toEqual([[104, 105]]);
  });

  it("バイト列にできない中身は onUnreadable に渡す(close の後は渡さない)", async () => {
    const fake = createFakeIpc();
    const ipc = createIpc(fake.transport);
    const received: Uint8Array[] = [];
    const unreadable: unknown[] = [];

    const sub = ipc.connectTerminal(
      "t1",
      24,
      80,
      (data) => received.push(data),
      (message) => unreadable.push(message),
    );
    await sub.connected;
    fake.channels[0]?.send("文字列");
    fake.channels[0]?.send([256]);
    sub.close();
    fake.channels[0]?.send("閉じた後");

    expect(received).toEqual([]);
    expect(unreadable).toEqual(["文字列", [256]]);
  });

  it("close の後に届いた出力は捨て、チャネルを閉じる。2 回呼んでもよい", async () => {
    const fake = createFakeIpc();
    const ipc = createIpc(fake.transport);
    const received: Uint8Array[] = [];

    const sub = ipc.connectTerminal("t1", 24, 80, (data) => received.push(data));
    await sub.connected;
    sub.close();
    sub.close();
    fake.channels[0]?.send(new Uint8Array([1]).buffer);

    expect(received).toEqual([]);
    expect(fake.channels[0]?.closed).toBe(true);
  });

  it("接続が失敗したらチャネルを閉じ、失敗を connected で返す", async () => {
    const fake = createFakeIpc({
      connect_terminal: () => {
        throw "シェルを起動できなかった";
      },
    });
    const ipc = createIpc(fake.transport);

    const sub = ipc.connectTerminal("t1", 24, 80, () => {});

    await expect(sub.connected).rejects.toBe("シェルを起動できなかった");
    expect(fake.channels[0]?.closed).toBe(true);
  });
});

describe("toBytes", () => {
  it("ArrayBuffer・型付き配列・バイトの配列をバイト列にする", () => {
    expect([...(toBytes(new Uint8Array([1, 2]).buffer) ?? [])]).toEqual([1, 2]);
    const view = new Uint8Array([9, 1, 2, 9]).subarray(1, 3);
    expect([...(toBytes(view) ?? [])]).toEqual([1, 2]);
    expect([...(toBytes([0, 255]) ?? [])]).toEqual([0, 255]);
  });

  it("それ以外(文字列、範囲外の数、null)は null", () => {
    expect(toBytes("ab")).toBeNull();
    expect(toBytes([256])).toBeNull();
    expect(toBytes([1.5])).toBeNull();
    expect(toBytes(null)).toBeNull();
  });
});
