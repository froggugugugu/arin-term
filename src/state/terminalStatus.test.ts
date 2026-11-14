import { describe, expect, it } from "vitest";

import { describeTerminalStatus } from "./terminalStatus";

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
