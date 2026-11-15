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
    present: (shown, background) => seen.push(`present [${shown.join(",")}] [${background.join(",")}]`),
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
        showHidden: false,
        terminalIds: ["a1"],
        activeTerminalId: "a1",
      },
      {
        id: "B",
        name: "B",
        color: "blue",
        displayPath: "~/B",
        missing: false,
        showHidden: false,
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

    const stop = syncTerminals(store, target);
    store.getState().replaceSnapshot(twoProjects("B"));
    store.getState().reportStartupError("写しは変わらない");
    stop();
    store.getState().replaceSnapshot(twoProjects("A"));

    expect(seen).toEqual([
      "status a1 running",
      "status b1 running",
      "retain a1,b1",
      "present [a1] [b1]",
      "status a1 running",
      "status b1 running",
      "retain a1,b1",
      "present [b1] [a1]",
    ]);
  });

  it("写しから消えたターミナル(タブを閉じた)は残すものに入らない", () => {
    const store = createAppStore();
    const { seen, target } = recorder();
    syncTerminals(store, target);

    store.getState().replaceSnapshot(
      sampleSnapshot({
        projects: [
          {
            id: "A",
            name: "A",
            color: "red",
            displayPath: "~/A",
            missing: false,
            showHidden: false,
            terminalIds: [],
            activeTerminalId: null,
          },
        ],
        selectedProjectId: "A",
      }),
    );

    expect(seen).toEqual(["retain ", "present [] []"]);
  });

  it("タイル表示では表示中のページのタイルを並びで伝え、ページが替わると新しいページの並びを伝える", () => {
    const store = createAppStore();
    const { seen, target } = recorder();
    const tiles = (page: number, tileIds: string[]) => {
      const base = twoProjects("A");
      return {
        ...base,
        display: {
          ...base.display,
          mode: "tiles" as const,
          page,
          pageCount: 2,
          grid: { cols: 1, rows: 1 },
          tileIds,
          focusedTerminalId: tileIds[0] ?? null,
        },
      };
    };
    syncTerminals(store, target);

    store.getState().replaceSnapshot(tiles(0, ["a1"]));
    store.getState().replaceSnapshot(tiles(1, ["b1"]));

    expect(seen.filter((line) => line.startsWith("present"))).toEqual(["present [a1] [b1]", "present [b1] [a1]"]);
  });

  it("タイル表示で全部が見えるときは、隠すものを空で伝える", () => {
    const store = createAppStore();
    const { seen, target } = recorder();
    const base = twoProjects("A");
    syncTerminals(store, target);

    store.getState().replaceSnapshot({
      ...base,
      display: { ...base.display, mode: "tiles", pageCount: 1, grid: { cols: 2, rows: 1 }, tileIds: ["a1", "b1"] },
    });

    expect(seen.at(-1)).toBe("present [a1,b1] []");
  });

  it("写しがまだ無ければ何も伝えない", () => {
    const { seen, target } = recorder();

    syncTerminals(createAppStore(), target);

    expect(seen).toEqual([]);
  });
});
