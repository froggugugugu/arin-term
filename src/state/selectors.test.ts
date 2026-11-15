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
  tileView,
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

describe("タイルの写しの読み取り(T47e)", () => {
  // 10 枚(A: a1〜a5 は緑、B: b1〜b5 は青)。1 ページ目は 9 枚(3×3)、2 ページ目は b5 の 1 枚。a2 にフォーカス。
  const aIds = ["a1", "a2", "a3", "a4", "a5"];
  const bIds = ["b1", "b2", "b3", "b4", "b5"];
  const ten = sampleSnapshot({
    projects: [
      { ...project("A", aIds, "a2"), name: "app", color: "green" },
      { ...project("B", bIds, "b1"), name: "api" },
    ],
    selectedProjectId: "A",
    terminals: [
      ...aIds.map((id, i) => sampleTerminal(id, running, { projectId: "A", label: `ターミナル ${i + 1}` })),
      ...bIds.map((id, i) => sampleTerminal(id, running, { projectId: "B", label: `ターミナル ${i + 1}` })),
    ],
    display: {
      mode: "tiles",
      maximized: false,
      page: 0,
      pageCount: 2,
      grid: { cols: 3, rows: 3 },
      tileIds: [...aIds, ...bIds.slice(0, 4)],
      focusedTerminalId: "a2",
      attentionPages: [],
    },
  });
  const secondPage = {
    ...ten,
    display: { ...ten.display, page: 1, grid: { cols: 1, rows: 1 }, tileIds: ["b5"], focusedTerminalId: "b5" },
  };

  it("タイルの見出しはプロジェクト名とタブ名をつなぐ", () => {
    const view = tileView(ten);

    expect(view.tiles.map((t) => t.title).slice(0, 2)).toEqual(["app: ターミナル 1", "app: ターミナル 2"]);
    expect(view.tiles[5]?.title).toBe("api: ターミナル 1");
    // 持ち主の無いターミナル(検証画面)はタブ名だけ。
    const orphan = sampleSnapshot({
      terminals: [sampleTerminal("v1", running)],
      display: { ...ten.display, pageCount: 1, grid: { cols: 1, rows: 1 }, tileIds: ["v1"], focusedTerminalId: null },
    });
    expect(tileView(orphan).tiles.map((t) => [t.title, t.projectId, t.color])).toEqual([["ターミナル 1", null, null]]);
  });

  it("タイルの色はそのプロジェクトの色", () => {
    const view = tileView(ten);

    expect(view.tiles.map((t) => t.color)).toEqual([...aIds.map(() => "green"), ...bIds.slice(0, 4).map(() => "blue")]);
    expect(view.tiles.map((t) => t.projectId).slice(4, 6)).toEqual(["A", "B"]);
  });

  it("タイルは表示中のページの並びで_フォーカス中_状態_アテンションを持つ", () => {
    const exited = { kind: "exited", code: 0, signal: null } as const;
    const snapshot = {
      ...ten,
      terminals: ten.terminals.map((t) =>
        t.id === "a3" ? { ...t, status: exited } : t.id === "b1" ? { ...t, attention: true } : t,
      ),
    };
    const view = tileView(snapshot);

    expect(view.tiles.map((t) => t.id)).toEqual([...aIds, ...bIds.slice(0, 4)]);
    expect(view.tiles.filter((t) => t.focused).map((t) => t.id)).toEqual(["a2"]);
    expect(view.tiles[2]?.status).toEqual(exited);
    expect(view.tiles.filter((t) => t.attention).map((t) => t.id)).toEqual(["b1"]);
    expect(view.grid).toEqual({ cols: 3, rows: 3 });
    expect(view.focusedTile?.id).toBe("a2");
  });

  it("ページの文字列は_1/2", () => {
    expect(tileView(ten).paging?.label).toBe("1/2");
    expect(tileView(secondPage).paging?.label).toBe("2/2");
    expect(tileView(secondPage).grid).toEqual({ cols: 1, rows: 1 });
    expect(tileView(secondPage).tiles.map((t) => t.id)).toEqual(["b5"]);
  });

  it("最初のページでは前へ進めない", () => {
    expect(tileView(ten).paging).toMatchObject({ canGoPrevious: false, canGoNext: true });
    // 最後のページでは次へ進めない(循環しない。【仮定】3)。
    expect(tileView(secondPage).paging).toMatchObject({ canGoPrevious: true, canGoNext: false });
  });

  it("前後のページのアテンションを返す", () => {
    const withAttention = (page: typeof ten, attentionPages: number[]) => ({
      ...page,
      display: { ...page.display, attentionPages },
    });

    expect(tileView(ten).paging).toMatchObject({ previousAttention: false, nextAttention: false });
    expect(tileView(withAttention(ten, [1])).paging).toMatchObject({ previousAttention: false, nextAttention: true });
    expect(tileView(withAttention(secondPage, [0])).paging).toMatchObject({
      previousAttention: true,
      nextAttention: false,
    });
    // 表示中のページの印は前後のどちらにも数えない。
    expect(tileView(withAttention(secondPage, [1])).paging).toMatchObject({
      previousAttention: false,
      nextAttention: false,
    });
  });

  it("ページが_1_つならページ表示を出さない", () => {
    const four = {
      ...ten,
      display: { ...ten.display, pageCount: 1, grid: { cols: 2, rows: 2 }, tileIds: ["a1", "a2", "a3", "a4"] },
    };

    expect(tileView(four).paging).toBeNull();
    // タブ表示と拡大中もページ表示を出さない(タイルのグリッドを出していないため)。
    expect(tileView({ ...ten, display: { ...ten.display, mode: "tabs" } }).paging).toBeNull();
    expect(tileView({ ...ten, display: { ...ten.display, maximized: true } }).paging).toBeNull();
  });

  it("表示モードと拡大中と拡大したタイルを返す", () => {
    const maximized = { ...ten, display: { ...ten.display, maximized: true } };

    expect([tileView(ten).mode, tileView(ten).maximized]).toEqual(["tiles", false]);
    expect([tileView(maximized).maximized, tileView(maximized).focusedTile?.title]).toEqual([true, "app: ターミナル 2"]);
    expect(tileView({ ...ten, display: { ...ten.display, mode: "tabs" } }).mode).toBe("tabs");
  });

  it("写しが無い_0_枚のときは空のタイル表示", () => {
    const view = tileView(null);

    expect(view).toEqual({
      mode: "tabs",
      maximized: false,
      grid: { cols: 0, rows: 0 },
      tiles: [],
      focusedTile: null,
      paging: null,
    });
    expect(tileView(sampleSnapshot({ display: { ...sampleSnapshot().display, mode: "tiles" } })).tiles).toEqual([]);
  });

  it("写しのターミナルの一覧に無い_id_はタイルにしない", () => {
    const snapshot = { ...ten, display: { ...ten.display, tileIds: ["a1", "gone"], focusedTerminalId: "gone" } };

    expect(tileView(snapshot).tiles.map((t) => t.id)).toEqual(["a1"]);
    expect(tileView(snapshot).focusedTile).toBeNull();
  });

  it("同じ写しには同じ結果を返し_中身が同じなら新しい写しでも同じ結果を返す", () => {
    const view = tileView(ten);
    expect(tileView(ten)).toBe(view);
    // 表示に関わらない値だけが替わった写し(知らせなど)。
    expect(tileView({ ...ten, notice: "保存できませんでした" })).toBe(view);
  });

  it("替わったタイルだけが新しくなる", () => {
    const view = tileView(ten);
    const next = tileView({
      ...ten,
      terminals: ten.terminals.map((t) => (t.id === "a1" ? { ...t, attention: true } : t)),
    });

    expect(next).not.toBe(view);
    expect(next.tiles[0]).not.toBe(view.tiles[0]);
    expect(next.tiles[1]).toBe(view.tiles[1]);
    expect(next.grid).toBe(view.grid);
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
