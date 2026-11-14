import { describe, expect, it } from "vitest";

import { createFakeIpc, sampleSnapshot } from "../test/fakeIpc";
import { createIpc } from "./client";

describe("src/ipc の包み", () => {
  it("写しの取得は get_app_snapshot を引数なしで呼び、届いた写しを返す", async () => {
    const snapshot = sampleSnapshot({ appVersion: "9.8.7" });
    const fake = createFakeIpc({ get_app_snapshot: () => snapshot });
    const ipc = createIpc(fake.invoke);

    const result = await ipc.getAppSnapshot();

    expect(fake.calls).toEqual([{ command: "get_app_snapshot", args: undefined }]);
    expect(result).toBe(snapshot);
  });

  it("準備完了の通知は notify_app_ready を引数なしで 1 回だけ呼ぶ", async () => {
    const fake = createFakeIpc();
    const ipc = createIpc(fake.invoke);

    await expect(ipc.notifyAppReady()).resolves.toBeUndefined();

    expect(fake.calls).toEqual([{ command: "notify_app_ready", args: undefined }]);
  });

  it("Rust 側の失敗はそのまま呼び出し元に返す", async () => {
    const fake = createFakeIpc({
      get_app_snapshot: () => {
        throw new Error("失敗");
      },
    });
    const ipc = createIpc(fake.invoke);

    await expect(ipc.getAppSnapshot()).rejects.toThrow("失敗");
  });
});
