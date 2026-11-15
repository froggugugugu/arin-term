import { describe, expect, it } from "vitest";

import type { ProjectCardSnapshot } from "../ipc/generated/ProjectCardSnapshot";
import { sampleSnapshot, sampleTerminal } from "../test/fakeIpc";
import { presentation, projectTabs, selectedProject, shownTerminal } from "./selectors";

const project = (id: string, terminalIds: string[], activeTerminalId: string | null): ProjectCardSnapshot => ({
  id,
  name: id,
  color: "blue",
  displayPath: `~/${id}`,
  missing: false,
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
