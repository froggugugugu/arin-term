import { describe, expect, it } from "vitest";

import { createIpc } from "../ipc/client";
import type { AppSnapshot } from "../ipc/generated/AppSnapshot";
import type { TerminalStatus } from "../ipc/generated/TerminalStatus";
import { TerminalManager } from "../terminal/TerminalManager";
import { createFakeIpc, sampleSnapshot, sampleTerminal } from "../test/fakeIpc";
import { createFakeTerminalEngine } from "../test/fakeTerminalEngine";
import { createAppStore } from "./appStore";
import { syncTerminals, type TerminalSyncTarget } from "./terminalSync";

/** 伝えられたことを順に記録する受け手。 */
function recorder() {
  const seen: string[] = [];
  /** 真ん中クリックの設定は別に記録する(ほかの並びの検査を変えないため)。 */
  const middle: boolean[] = [];
  const target: TerminalSyncTarget = {
    setStatus: (id, status) => seen.push(`status ${id} ${status.kind}`),
    retain: (ids) => seen.push(`retain ${[...ids].join(",")}`),
    present: (shown, background) => seen.push(`present [${shown.join(",")}] [${background.join(",")}]`),
    setFontSize: (px) => seen.push(`font ${px}`),
    setMiddleClickPaste: (enabled) => middle.push(enabled),
  };
  return { seen, middle, target };
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
        editorIds: [],
        shownEditorId: null,
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
        editorIds: [],
        shownEditorId: null,
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
      "font 15",
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
            editorIds: [],
            shownEditorId: null,
          },
        ],
        selectedProjectId: "A",
      }),
    );

    expect(seen).toEqual(["font 15", "retain ", "present [] []"]);
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

  it("写しのターミナルの文字の大きさを_present_より前に伝える", () => {
    const store = createAppStore();
    const { seen, target } = recorder();
    syncTerminals(store, target);

    store.getState().replaceSnapshot(withTerminalFontSize(twoProjects("A"), 20));

    expect(seen[0]).toBe("font 20");
    expect(seen.indexOf("font 20")).toBeLessThan(seen.findIndex((line) => line.startsWith("present")));
  });

  it("写しの大きさが同じなら何もしない", () => {
    const store = createAppStore();
    const { seen, target } = recorder();
    syncTerminals(store, target);

    store.getState().replaceSnapshot(withTerminalFontSize(twoProjects("A"), 20));
    store.getState().replaceSnapshot(withTerminalFontSize(twoProjects("B"), 20));
    // 画面の大きさだけが変わった写し: ターミナルには伝えない。
    const base = withTerminalFontSize(twoProjects("B"), 20);
    store.getState().replaceSnapshot({
      ...base,
      fontSizes: { ...base.fontSizes, interface: { ...base.fontSizes.interface, value: 16 } },
    });
    store.getState().replaceSnapshot(withTerminalFontSize(twoProjects("B"), 21));

    expect(seen.filter((line) => line.startsWith("font"))).toEqual(["font 20", "font 21"]);
  });

  it("真ん中クリックの設定は変わったときだけ伝える(ADR-020)", () => {
    const store = createAppStore();
    const { middle, target } = recorder();
    syncTerminals(store, target);

    store.getState().replaceSnapshot(twoProjects("A"));
    store.getState().replaceSnapshot(twoProjects("B"));
    store.getState().replaceSnapshot({ ...twoProjects("B"), middleClickPaste: false });
    store.getState().replaceSnapshot({ ...twoProjects("A"), middleClickPaste: false });
    store.getState().replaceSnapshot(twoProjects("A"));

    expect(middle).toEqual([true, false, true]);
  });

  it("隠したまま作るサーフェスも写しの大きさで作る", () => {
    const store = createAppStore();
    const engine = createFakeTerminalEngine();
    const manager = new TerminalManager(engine, createIpc(createFakeIpc().transport), { reportError: () => {} });
    syncTerminals(store, manager);

    // プロジェクト A を表示中。B のターミナル b1 は写しが届いた時点で隠したまま作る。
    store.getState().replaceSnapshot(withTerminalFontSize(twoProjects("A"), 20));

    const b1 = engine.created.find((s) => s.id === "b1");
    expect(b1?.visible).toBe(false);
    expect(b1?.createdFontSize).toBe(20);
  });
});

/** 写しのターミナルの文字の大きさを替える。 */
function withTerminalFontSize(snapshot: AppSnapshot, value: number): AppSnapshot {
  return {
    ...snapshot,
    fontSizes: { ...snapshot.fontSizes, terminal: { ...snapshot.fontSizes.terminal, value } },
  };
}
