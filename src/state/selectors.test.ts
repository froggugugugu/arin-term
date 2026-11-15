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
    expect(presentation(snapshot)).toEqual({ shown: "a2", background: ["a1", "b1"] });
    expect(presentation({ ...snapshot, selectedProjectId: "C" })).toEqual({
      shown: null,
      background: ["a1", "a2", "b1"],
    });
    expect(presentation(null)).toEqual({ shown: null, background: [] });
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
