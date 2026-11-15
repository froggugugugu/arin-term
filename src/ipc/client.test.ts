import { describe, expect, it } from "vitest";

import { createFakeIpc, sampleSnapshot } from "../test/fakeIpc";
import { createIpc, toBytes } from "./client";

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
