import { describe, expect, it } from "vitest";

import type { ProjectCardSnapshot } from "../ipc/generated/ProjectCardSnapshot";
import { sampleSnapshot, sampleTerminal } from "../test/fakeIpc";
import { DEFAULT_SHORTCUT_TABLE } from "../logic/keys/defaultShortcuts";
import { applyDirListing, createTreeModel, selectPath, type TreeModel } from "../logic/tree/treeModel";
import {
  activeTreeModel,
  activeTreeRows,
  presentation,
  projectTabs,
  selectedProject,
  selectedTreeTarget,
  shownTerminal,
  treeMenuContext,
  treeSelectionKind,
  visibleTerminalIds,
} from "./selectors";

const project = (id: string, terminalIds: string[], activeTerminalId: string | null): ProjectCardSnapshot => ({
  id,
  name: id,
  color: "blue",
  displayPath: `~/${id}`,
  missing: false,
  showHidden: false,
  terminalIds,
  activeTerminalId,
});

const running = { kind: "running" } as const;

// A: a1・a2(a2 を表示中)、B: b1(表示中)、C: タブ 0 枚。
const snapshot = sampleSnapshot({
  projects: [project("A", ["a1", "a2"], "a2"), project("B", ["b1"], "b1"), project("C", [], null)],
  selectedProjectId: "A",
  terminals: [
    sampleTerminal("a1", running, { projectId: "A" }),
    sampleTerminal("a2", running, { projectId: "A", label: "ターミナル 2" }),
    sampleTerminal("b1", running, { projectId: "B" }),
  ],
});

describe("写しから表示用の値を取り出す関数", () => {
  it("selectedProject は選択中のプロジェクト。選択が無い・見つからない・写しが無いなら null", () => {
    expect(selectedProject(snapshot)?.id).toBe("A");
    expect(selectedProject({ ...snapshot, selectedProjectId: null })).toBeNull();
    expect(selectedProject({ ...snapshot, selectedProjectId: "Z" })).toBeNull();
    expect(selectedProject(null)).toBeNull();
  });

  it("projectTabs はプロジェクトのタブをタブの順に返す。写しに無い ID は飛ばす", () => {
    expect(projectTabs(snapshot, "A").map((t) => t.id)).toEqual(["a1", "a2"]);
    expect(projectTabs(snapshot, "C")).toEqual([]);
    expect(projectTabs(snapshot, "Z")).toEqual([]);
    expect(projectTabs({ ...snapshot, terminals: snapshot.terminals.slice(1) }, "A").map((t) => t.id)).toEqual([
      "a2",
    ]);
    expect(projectTabs(null, "A")).toEqual([]);
  });

  it("shownTerminal は選択中のプロジェクトの表示中のタブ。タブが 0 枚・写しに無いなら null", () => {
    expect(shownTerminal(snapshot)?.id).toBe("a2");
    expect(shownTerminal({ ...snapshot, selectedProjectId: "B" })?.id).toBe("b1");
    expect(shownTerminal({ ...snapshot, selectedProjectId: "C" })).toBeNull();
    expect(shownTerminal({ ...snapshot, terminals: [] })).toBeNull();
    expect(shownTerminal(null)).toBeNull();
  });

  it("presentation は表示中の 1 枚と、隠して取り込みだけ続ける残りの全部(全プロジェクト)", () => {
    expect(presentation(snapshot)).toEqual({ shown: ["a2"], background: ["a1", "b1"] });
    expect(presentation({ ...snapshot, selectedProjectId: "C" })).toEqual({
      shown: [],
      background: ["a1", "a2", "b1"],
    });
    expect(presentation(null)).toEqual({ shown: [], background: [] });
  });
});

describe("見えるターミナルの並び(T47d。Rust の Workspace::visible_terminals と同じ規則)", () => {
  // 4 枚(A: a1・a2、B: b1・b2)。a2 にフォーカス(=選択中の A の表示中のタブ)。
  const four = sampleSnapshot({
    projects: [project("A", ["a1", "a2"], "a2"), project("B", ["b1", "b2"], "b1")],
    selectedProjectId: "A",
    terminals: [
      sampleTerminal("a1", running, { projectId: "A" }),
      sampleTerminal("a2", running, { projectId: "A" }),
      sampleTerminal("b1", running, { projectId: "B" }),
      sampleTerminal("b2", running, { projectId: "B" }),
    ],
  });
  const display = (overrides: Partial<typeof four.display>) => ({
    ...four,
    display: {
      ...four.display,
      pageCount: 1,
      grid: { cols: 2, rows: 2 },
      tileIds: ["a1", "a2", "b1", "b2"],
      focusedTerminalId: "a2",
      ...overrides,
    },
  });

  it("タブ表示では表示中のタブ_1_枚だけ(タブ表示ではタイルの並びを使わない)", () => {
    expect(visibleTerminalIds(display({ mode: "tabs" }))).toEqual(["a2"]);
    expect(visibleTerminalIds({ ...display({ mode: "tabs" }), selectedProjectId: null })).toEqual([]);
    expect(visibleTerminalIds(null)).toEqual([]);
  });

  it("タイル表示では表示中のページのタイル(並びの順)", () => {
    const tiles = display({ mode: "tiles" });

    expect(visibleTerminalIds(tiles)).toEqual(["a1", "a2", "b1", "b2"]);
    expect(presentation(tiles)).toEqual({ shown: ["a1", "a2", "b1", "b2"], background: [] });
  });

  it("タイル表示のページが替わると、そのページのタイルが見え、ほかは隠すものになる", () => {
    // 2 ページ目に b1・b2 だけがある形(グリッドとページは Rust が計算したものをそのまま使う)。
    const page2 = display({ mode: "tiles", page: 1, pageCount: 2, grid: { cols: 2, rows: 1 }, tileIds: ["b1", "b2"] });

    expect(presentation(page2)).toEqual({ shown: ["b1", "b2"], background: ["a1", "a2"] });
  });

  it("拡大中はフォーカス中の_1_枚だけ(タイルの並びは使わない)", () => {
    const maximized = display({ mode: "tiles", maximized: true, focusedTerminalId: "b2" });

    expect(presentation(maximized)).toEqual({ shown: ["b2"], background: ["a1", "a2", "b1"] });
    expect(visibleTerminalIds(display({ mode: "tiles", maximized: true, focusedTerminalId: null }))).toEqual([]);
  });

  it("写しのターミナルの一覧に無い_id_は見える並びに入れない", () => {
    expect(visibleTerminalIds(display({ mode: "tiles", tileIds: ["a1", "gone", "b1"] }))).toEqual(["a1", "b1"]);
    expect(visibleTerminalIds(display({ mode: "tiles", maximized: true, focusedTerminalId: "gone" }))).toEqual([]);
  });

  it("0_枚のタイル表示では何も見えない", () => {
    const empty = sampleSnapshot({ display: { ...sampleSnapshot().display, mode: "tiles" } });

    expect(presentation(empty)).toEqual({ shown: [], background: [] });
  });
});

describe("ツリーの選択子(T43a)", () => {
  const model: TreeModel = applyDirListing(createTreeModel(), "", [
    { name: "src", kind: "directory" },
    { name: "a.md", kind: "file" },
    { name: "lib", kind: "directoryLink" },
  ]);
  const view = (m: TreeModel | null) => ({
    activeProjectId: m === null ? null : "p1",
    trees: new Map(m === null ? [] : [["p1", m] as const]),
  });

  it("表示中のツリーが無ければモデルは null で行は空", () => {
    expect(activeTreeModel(view(null))).toBeNull();
    expect(activeTreeRows(view(null))).toEqual([]);
    expect(selectedTreeTarget(view(null))).toBeNull();
  });

  it("同じモデルの行は同じ配列を返す", () => {
    const rows = activeTreeRows(view(model));
    expect(rows.map((r) => r.path)).toEqual(["src", "a.md", "lib"]);
    expect(activeTreeRows(view(model))).toBe(rows);
  });

  it("選択をメニューの場所と選択の種類にする", () => {
    const at = (path: string | null) => view(selectPath(model, path));
    expect(selectedTreeTarget(at(null))).toBeNull();
    expect(treeSelectionKind(at(null))).toBe("none");
    expect(selectedTreeTarget(at(""))).toEqual({ kind: "root" });
    expect(treeSelectionKind(at(""))).toBe("root");
    expect(selectedTreeTarget(at("src"))).toEqual({ kind: "folder", path: "src" });
    expect(selectedTreeTarget(at("a.md"))).toEqual({ kind: "file", path: "a.md" });
    // フォルダへのリンクは中に作らないので file として扱う(T41b の TreeMenuTarget)。
    expect(selectedTreeTarget(at("lib"))).toEqual({ kind: "file", path: "lib" });
    expect(treeSelectionKind(at("a.md"))).toBe("entry");
    expect(selectedTreeTarget(at("gone.ts"))).toBeNull();
  });

  it("メニューの文脈は表示中のタブと隠しファイル表示を写しから読む", () => {
    const snapshot = sampleSnapshot({
      projects: [{ ...project("p1", ["t1"], "t1"), showHidden: true }],
      selectedProjectId: "p1",
      terminals: [sampleTerminal("t1", running, { projectId: "p1" })],
    });

    expect(treeMenuContext(snapshot, DEFAULT_SHORTCUT_TABLE)).toEqual({
      hasTab: true,
      showHidden: true,
      shortcuts: DEFAULT_SHORTCUT_TABLE,
    });
    expect(treeMenuContext(null, [])).toEqual({ hasTab: false, showHidden: false, shortcuts: [] });
  });
});
