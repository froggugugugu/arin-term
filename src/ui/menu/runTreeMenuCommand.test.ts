import { describe, expect, it } from "vitest";

import type { TreeMenuTarget } from "../../logic/menu/treeMenu";
import { applyDirListing, createTreeModel, selectPath } from "../../logic/tree/treeModel";
import { createAppStore } from "../../state/appStore";
import { createTreeStore } from "../../state/treeStore";
import { createUiStore } from "../../state/uiStore";
import { sampleSnapshot } from "../../test/fakeIpc";
import type { ProjectCard } from "../snapshotTypes";
import {
  createTreeClipboard,
  runTreeMenuCommand,
  runTreeShortcut,
  treeCommandScope,
  type TreeMenuActions,
  treeMenuContextOf,
} from "./runTreeMenuCommand";

function project(overrides: Partial<ProjectCard> = {}): ProjectCard {
  return {
    id: "p1",
    name: "app",
    color: "blue",
    displayPath: "~/dev/app",
    missing: false,
    showHidden: false,
    terminalIds: ["t1"],
    activeTerminalId: "t1",
    editorIds: [],
    shownEditorId: null,
    ...overrides,
  };
}

function fakeActions() {
  const calls: unknown[][] = [];
  const record =
    (name: string) =>
    async (...args: unknown[]) =>
      void calls.push([name, ...args]);
  const actions: TreeMenuActions = {
    openEntry: record("openEntry"),
    openEntryExternally: record("openEntryExternally"),
    expandDir: (path) => void calls.push(["expandDir", path]),
    revealEntry: record("revealEntry"),
    insertEntryPath: record("insertEntryPath"),
    copyEntryPath: record("copyEntryPath"),
    cutEntries: record("cutEntries"),
    copyEntries: record("copyEntries"),
    pasteEntries: record("pasteEntries"),
    trashEntry: record("trashEntry"),
    setShowHidden: record("setShowHidden"),
    collapseAllDirs: () => void calls.push(["collapseAllDirs"]),
  };
  return { actions, calls };
}

/** 選択中のプロジェクト p1 のツリー(ルートに src と a.ts、選択は selected)。 */
function treeWith(selected: string | null) {
  const tree = createTreeStore();
  let model = applyDirListing(createTreeModel(), "", [
    { name: "src", kind: "directory" },
    { name: "a.ts", kind: "file" },
  ]);
  model = selectPath(model, selected);
  tree.setState({ activeProjectId: "p1", trees: new Map([["p1", model]]) });
  return tree;
}

const run = (target: TreeMenuTarget, command: Parameters<typeof runTreeMenuCommand>[3], p = project()) => {
  const { actions, calls } = fakeActions();
  const ui = createUiStore();
  void runTreeMenuCommand(actions, ui, treeCommandScope(p, target), command);
  return { calls, ui };
};

describe("runTreeMenuCommand(ツリーのメニューで選ばれた項目を操作に渡す)", () => {
  it("開くはファイルなら既定のアプリで開き_フォルダなら開き_ルートでは何もしない", () => {
    expect(run({ kind: "file", path: "a.ts" }, { type: "open", path: "a.ts" }).calls).toEqual([["openEntry", "p1", "a.ts"]]);
    expect(run({ kind: "folder", path: "src" }, { type: "open", path: "src" }).calls).toEqual([["expandDir", "src"]]);
    expect(run({ kind: "root" }, { type: "open", path: "" }).calls).toEqual([]);
  });

  it("md_の開くはエディタの振り分けの_openEntry_へ_既定のアプリで開くは_openEntryExternally_へ", () => {
    // .md をエディタで開くか既定のアプリで開くかは actions.openEntry が決める(M4.8)
    expect(run({ kind: "file", path: "a.md" }, { type: "open", path: "a.md" }).calls).toEqual([["openEntry", "p1", "a.md"]]);
    expect(run({ kind: "file", path: "a.md" }, { type: "open-external", path: "a.md" }).calls).toEqual([
      ["openEntryExternally", "p1", "a.md"],
    ]);
  });

  it("パスの操作はプロジェクトとパスを付けて呼び_挿入は表示中のタブへ", () => {
    const file = { kind: "file", path: "a b.ts" } as const;
    expect(run(file, { type: "reveal", path: "a b.ts" }).calls).toEqual([["revealEntry", "p1", "a b.ts"]]);
    expect(run(file, { type: "copy-path", path: "a b.ts", style: "relative" }).calls).toEqual([
      ["copyEntryPath", "p1", "a b.ts", "relative"],
    ]);
    expect(run(file, { type: "insert-path", path: "a b.ts" }).calls).toEqual([
      ["insertEntryPath", "p1", ["a b.ts"], "t1"],
    ]);
    // タブが無ければ挿入しない(メニューでは無効)。
    expect(run(file, { type: "insert-path", path: "a b.ts" }, project({ activeTerminalId: null })).calls).toEqual([]);
  });

  it("切り取り_コピー_ペースト_ゴミ箱はそれぞれの操作を呼ぶ", () => {
    const file = { kind: "file", path: "a.ts" } as const;
    expect(run(file, { type: "cut", path: "a.ts" }).calls).toEqual([["cutEntries", "p1", ["a.ts"]]]);
    expect(run(file, { type: "copy", path: "a.ts" }).calls).toEqual([["copyEntries", "p1", ["a.ts"]]]);
    expect(run(file, { type: "paste", destination: "" }).calls).toEqual([["pasteEntries", "p1", ""]]);
    expect(run(file, { type: "trash", path: "a.ts" }).calls).toEqual([["trashEntry", "p1", "a.ts"]]);
  });

  it("名称変更と新規作成はインライン編集を始める", () => {
    const rename = run({ kind: "file", path: "a.ts" }, { type: "rename", path: "a.ts" });
    expect(rename.calls).toEqual([]);
    expect(rename.ui.getState().inlineEdit).toEqual({ mode: "rename", projectId: "p1", path: "a.ts" });

    const file = run({ kind: "folder", path: "src" }, { type: "new-file", parent: "src" });
    expect(file.ui.getState().inlineEdit).toEqual({ mode: "create", projectId: "p1", parent: "src", kind: "file" });
    const folder = run({ kind: "blank" }, { type: "new-folder", parent: "" });
    expect(folder.ui.getState().inlineEdit).toEqual({ mode: "create", projectId: "p1", parent: "", kind: "folder" });
  });

  it("隠しファイルの表示は反対の値にし_すべて折りたたむは折りたたむ", () => {
    expect(run({ kind: "blank" }, { type: "toggle-hidden" }).calls).toEqual([["setShowHidden", "p1", true]]);
    expect(run({ kind: "blank" }, { type: "toggle-hidden" }, project({ showHidden: true })).calls).toEqual([
      ["setShowHidden", "p1", false],
    ]);
    expect(run({ kind: "blank" }, { type: "collapse-all" }).calls).toEqual([["collapseAllDirs"]]);
  });
});

describe("treeMenuContextOf(メニューに渡す画面の値)", () => {
  it("表示中のタブの有無と隠しファイルの表示をプロジェクトから読む", () => {
    const ui = createUiStore();
    const shortcuts = ui.getState().shortcuts;
    expect(treeMenuContextOf(project(), shortcuts)).toEqual({ hasTab: true, showHidden: false, shortcuts });
    expect(treeMenuContextOf(project({ activeTerminalId: null, showHidden: true }), shortcuts)).toEqual({
      hasTab: false,
      showHidden: true,
      shortcuts,
    });
  });
});

describe("runTreeShortcut(表のキーを選んでいる項目のメニューの操作にする)", () => {
  const setup = (selected: string | null) => {
    const { actions, calls } = fakeActions();
    const ui = createUiStore();
    return { deps: { actions, ui, tree: treeWith(selected) }, calls, ui };
  };

  it("選んでいる項目に対してメニューの同じ操作を呼ぶ", () => {
    const { deps, calls } = setup("a.ts");
    expect(runTreeShortcut(deps, project(), "tree.trash")).toBe(true);
    expect(runTreeShortcut(deps, project(), "tree.copy-relative-path")).toBe(true);
    expect(calls).toEqual([
      ["trashEntry", "p1", "a.ts"],
      ["copyEntryPath", "p1", "a.ts", "relative"],
    ]);
  });

  it("ルートでは名称変更とゴミ箱を呼ばない(FR-035)", () => {
    const { deps, calls, ui } = setup("");
    expect(runTreeShortcut(deps, project(), "tree.trash")).toBe(false);
    expect(runTreeShortcut(deps, project(), "tree.rename")).toBe(false);
    expect(calls).toEqual([]);
    expect(ui.getState().inlineEdit).toBeNull();
  });

  it("何も選んでいなければ余白のメニューから引き_新規ファイルはルートに作る", () => {
    const { deps, ui } = setup(null);
    expect(runTreeShortcut(deps, project(), "tree.new-file")).toBe(true);
    expect(ui.getState().inlineEdit).toEqual({ mode: "create", projectId: "p1", parent: "", kind: "file" });
    // 余白のメニューに無い項目は何もしない。
    expect(runTreeShortcut(deps, project(), "tree.trash")).toBe(false);
  });
});

describe("createTreeClipboard(コピー・切り取り・ペーストのツリーの受け口)", () => {
  const setup = (selected: string | null, nameInput = false, treeElementFocused = true) => {
    const { actions, calls } = fakeActions();
    const ui = createUiStore();
    const app = createAppStore();
    app.getState().replaceSnapshot(sampleSnapshot({ projects: [project()], selectedProjectId: "p1" }));
    const clipboard = createTreeClipboard({
      actions,
      ui,
      tree: treeWith(selected),
      app,
      isNameInputFocused: () => nameInput,
      isTreeElementFocused: () => treeElementFocused,
    });
    return { clipboard, calls, ui };
  };

  it("ツリーにフォーカスが無ければ null(ターミナルに任せる)", () => {
    const { clipboard } = setup("a.ts");
    expect(clipboard.focus()).toBeNull();
  });

  it("ツリーにフォーカスがあれば選択の種類を返す", () => {
    const entry = setup("a.ts");
    entry.ui.getState().setTreeFocused(true);
    expect(entry.clipboard.focus()).toEqual({ kind: "tree", selection: "entry" });
    const root = setup("");
    root.ui.getState().setTreeFocused(true);
    expect(root.clipboard.focus()).toEqual({ kind: "tree", selection: "root" });
    const none = setup(null);
    none.ui.getState().setTreeFocused(true);
    expect(none.clipboard.focus()).toEqual({ kind: "tree", selection: "none" });
  });

  it("フォーカスの印が残っていても_実際のフォーカスがツリーに無ければ null(ターミナルの操作にする)", () => {
    const { clipboard, ui } = setup("a.ts", false, false);
    ui.getState().setTreeFocused(true);
    expect(clipboard.focus()).toBeNull();
  });

  it("名前の入力中は name-input", () => {
    const { clipboard } = setup("a.ts", true);
    expect(clipboard.focus()).toEqual({ kind: "name-input" });
  });

  it("コピー・切り取り・ペーストは選んでいる項目の操作になる", () => {
    const { clipboard, calls } = setup("src");
    clipboard.run("tree-copy");
    clipboard.run("tree-cut");
    clipboard.run("tree-paste");
    expect(calls).toEqual([
      ["copyEntries", "p1", ["src"]],
      ["cutEntries", "p1", ["src"]],
      ["pasteEntries", "p1", "src"],
    ]);
  });
});
