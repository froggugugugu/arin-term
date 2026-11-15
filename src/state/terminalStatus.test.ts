import { describe, expect, it } from "vitest";

import type { TerminalStatus } from "../ipc/generated/TerminalStatus";
import { sampleSnapshot, sampleTerminal } from "../test/fakeIpc";
import { createAppStore } from "./appStore";
import { describeTerminalStatus, forwardTerminalStatuses } from "./terminalStatus";

describe("describeTerminalStatus(シェルの状態を画面に出す文にする)", () => {
  it("起動前と実行中は何も出さない", () => {
    expect(describeTerminalStatus({ kind: "notStarted" })).toBeNull();
    expect(describeTerminalStatus({ kind: "running" })).toBeNull();
  });

  it("終了は終了コードかシグナルを添える", () => {
    expect(describeTerminalStatus({ kind: "exited", code: 0, signal: null })).toBe("シェルが終了した(終了コード 0)");
    expect(describeTerminalStatus({ kind: "exited", code: null, signal: "SIGHUP" })).toBe(
      "シェルが終了した(シグナル SIGHUP)",
    );
    expect(describeTerminalStatus({ kind: "exited", code: 1, signal: "SIGKILL" })).toBe(
      "シェルが終了した(終了コード 1、シグナル SIGKILL)",
    );
    expect(describeTerminalStatus({ kind: "exited", code: null, signal: null })).toBe("シェルが終了した");
  });

  it("起動の失敗は理由を添える", () => {
    expect(describeTerminalStatus({ kind: "failed", message: "PTY を開けなかった" })).toBe(
      "シェルを起動できなかった: PTY を開けなかった",
    );
  });
});

describe("forwardTerminalStatuses(写しの状態をターミナルの送信側へ伝える。S-3)", () => {
  const terminal = (id: string, status: TerminalStatus) => sampleTerminal(id, status);

  it("いまの写しと、届いた写しごとに、全ターミナルの状態を伝える。外したら伝えない", () => {
    const store = createAppStore();
    store.getState().replaceSnapshot(sampleSnapshot({ terminals: [terminal("t1", { kind: "running" })] }));
    const seen: [string, string][] = [];

    const stop = forwardTerminalStatuses(store, (id, status) => seen.push([id, status.kind]));
    store.getState().replaceSnapshot(
      sampleSnapshot({
        terminals: [terminal("t1", { kind: "exited", code: 0, signal: null }), terminal("t2", { kind: "running" })],
      }),
    );
    store.getState().reportStartupError("写しは変わらない");
    stop();
    store.getState().replaceSnapshot(sampleSnapshot({ terminals: [terminal("t2", { kind: "exited", code: 1, signal: null })] }));

    expect(seen).toEqual([
      ["t1", "running"],
      ["t1", "exited"],
      ["t2", "running"],
    ]);
  });

  it("写しがまだ無ければ何も伝えない", () => {
    const seen: string[] = [];

    forwardTerminalStatuses(createAppStore(), (id) => seen.push(id));

    expect(seen).toEqual([]);
  });
});
