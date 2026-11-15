import { describe, expect, it } from "vitest";

import type { TerminalStatus } from "../ipc/generated/TerminalStatus";
import { sampleSnapshot, sampleTerminal } from "../test/fakeIpc";
import { createAppStore } from "./appStore";
import { syncTerminals, type TerminalSyncTarget } from "./terminalSync";

/** 伝えられたことを順に記録する受け手。 */
function recorder() {
  const seen: string[] = [];
  const target: TerminalSyncTarget = {
    setStatus: (id, status) => seen.push(`status ${id} ${status.kind}`),
    retain: (ids) => seen.push(`retain ${[...ids].join(",")}`),
    present: (shown, background) => seen.push(`present ${shown ?? "-"} [${background.join(",")}]`),
  };
  return { seen, target };
}

const running: TerminalStatus = { kind: "running" };

const twoProjects = (selected: string) =>
  sampleSnapshot({
    projects: [
      {
        id: "A",
        name: "A",
        color: "red",
        displayPath: "~/A",
        missing: false,
        terminalIds: ["a1"],
        activeTerminalId: "a1",
      },
      {
        id: "B",
        name: "B",
        color: "blue",
        displayPath: "~/B",
        missing: false,
        terminalIds: ["b1"],
        activeTerminalId: "b1",
      },
    ],
    selectedProjectId: selected,
    terminals: [sampleTerminal("a1", running, { projectId: "A" }), sampleTerminal("b1", running, { projectId: "B" })],
  });

describe("syncTerminals(写しをターミナルの管理へ伝える)", () => {
  it("いまの写しと、届いた写しごとに、状態・残すもの・表示中と隠すものを伝える。外したら伝えない", () => {
    const store = createAppStore();
    store.getState().replaceSnapshot(twoProjects("A"));
    const { seen, target } = recorder();

    const stop = syncTerminals(store, target, { verifyGrid: false });
    store.getState().replaceSnapshot(twoProjects("B"));
    store.getState().reportStartupError("写しは変わらない");
    stop();
    store.getState().replaceSnapshot(twoProjects("A"));

    expect(seen).toEqual([
      "status a1 running",
      "status b1 running",
      "retain a1,b1",
      "present a1 [b1]",
      "status a1 running",
      "status b1 running",
      "retain a1,b1",
      "present b1 [a1]",
    ]);
  });

  it("写しから消えたターミナル(タブを閉じた)は残すものに入らない", () => {
    const store = createAppStore();
    const { seen, target } = recorder();
    syncTerminals(store, target, { verifyGrid: false });

    store.getState().replaceSnapshot(
      sampleSnapshot({
        projects: [
          {
            id: "A",
            name: "A",
            color: "red",
            displayPath: "~/A",
            missing: false,
            terminalIds: [],
            activeTerminalId: null,
          },
        ],
        selectedProjectId: "A",
      }),
    );

    expect(seen).toEqual(["retain ", "present - []"]);
  });

  it("検証画面では表示中と隠すものを伝えない(9 枚とも検証画面が表示を決める)", () => {
    const store = createAppStore();
    const { seen, target } = recorder();
    syncTerminals(store, target, { verifyGrid: true });

    store.getState().replaceSnapshot(sampleSnapshot({ terminals: [sampleTerminal("terminal-1", running)] }));

    expect(seen).toEqual(["status terminal-1 running", "retain terminal-1"]);
  });

  it("写しがまだ無ければ何も伝えない", () => {
    const { seen, target } = recorder();

    syncTerminals(createAppStore(), target, { verifyGrid: false });

    expect(seen).toEqual([]);
  });
});
