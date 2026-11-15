import { describe, expect, it } from "vitest";

import { createIpc } from "../ipc/client";
import type { DirListing } from "../ipc/generated/DirListing";
import type { ProjectCardSnapshot } from "../ipc/generated/ProjectCardSnapshot";
import { createFakeIpc, type Deferred, deferred, sampleSnapshot } from "../test/fakeIpc";
import { createAppStore } from "./appStore";
import { activeTreeModel, activeTreeRows } from "./selectors";
import { createTreeController, createTreeStore } from "./treeStore";
import { createUiStore } from "./uiStore";

const dir = (name: string) => ({ name, kind: "directory" as const });
const file = (name: string) => ({ name, kind: "file" as const });
const listed = (...entries: ReturnType<typeof dir | typeof file>[]): DirListing => ({ kind: "listed", entries });

function project(id: string, overrides: Partial<ProjectCardSnapshot> = {}): ProjectCardSnapshot {
  return {
    id,
    name: id,
    color: "blue",
    displayPath: `~/dev/${id}`,
    missing: false,
    showHidden: false,
    terminalIds: [],
    activeTerminalId: null,
    ...overrides,
  };
}

/** 遅れて届いた応答の処理を済ませる。 */
const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

/**
 * 偽の通信の list_directory は fs の中身を返す(キーは「プロジェクト:パス」)。
 * hold に入れたキーは、テストが解決するまで応答を止める。
 */
function setup() {
  const fs = new Map<string, DirListing>();
  const held = new Map<string, Deferred<DirListing>[]>();
  const holdKeys = new Set<string>();
  const fake = createFakeIpc({
    list_directory: ({ projectId, path }) => {
      const key = `${projectId}:${path}`;
      if (holdKeys.has(key)) {
        const d = deferred<DirListing>();
        held.set(key, [...(held.get(key) ?? []), d]);
        return d.promise;
      }
      return fs.get(key) ?? { kind: "notFound" };
    },
  });
  const app = createAppStore();
  const ui = createUiStore();
  const tree = createTreeStore();
  const errors: string[] = [];
  const controller = createTreeController({
    ipc: createIpc(fake.transport),
    tree,
    app,
    ui,
    reportError: (m) => errors.push(m),
  });
  const show = (selected: string | null, projects: ProjectCardSnapshot[] = [project("p1"), project("p2")]) =>
    app.getState().replaceSnapshot(sampleSnapshot({ projects, selectedProjectId: selected }));
  const listCalls = () =>
    fake.calls.filter((c) => c.command === "list_directory").map((c) => c.args as { projectId: string; path: string });
  const rowPaths = () => activeTreeRows(tree.getState()).map((r) => r.path);
  return { fs, held, holdKeys, fake, app, ui, tree, errors, controller, show, listCalls, rowPaths };
}

describe("treeStore(ツリーの読み込みと保留)", () => {
  it("選択したプロジェクトのルートを読み込む", async () => {
    const t = setup();
    t.fs.set("p1:", listed(dir("src"), file("README.md")));

    t.show("p1");
    await settle();

    expect(t.listCalls()).toEqual([{ projectId: "p1", path: "" }]);
    expect(t.tree.getState().activeProjectId).toBe("p1");
    expect(t.rowPaths()).toEqual(["src", "README.md"]);
  });

  it("展開でフォルダの中身を求め届いたら行が増える", async () => {
    const t = setup();
    t.fs.set("p1:", listed(dir("src"), file("README.md")));
    t.fs.set("p1:src", listed(file("a.ts"), file("b.ts")));
    t.show("p1");
    await settle();

    t.controller.expand("src");
    expect(activeTreeModel(t.tree.getState())?.loading.has("src")).toBe(true);
    await settle();

    expect(t.listCalls()).toEqual([
      { projectId: "p1", path: "" },
      { projectId: "p1", path: "src" },
    ]);
    expect(t.rowPaths()).toEqual(["src", "src/a.ts", "src/b.ts", "README.md"]);
  });

  it("tree-changed_で読み込み済みのフォルダを読み直す", async () => {
    const t = setup();
    t.fs.set("p1:", listed(dir("src")));
    t.fs.set("p1:src", listed(file("a.ts")));
    t.show("p1");
    await settle();
    t.controller.expand("src");
    await settle();
    t.fs.set("p1:src", listed(file("a.ts"), file("new.txt")));

    // 未読込の docs の変更は無視し、読み込み済みの src だけを読み直す。
    t.controller.handleTreeChange({ projectId: "p1", dirs: ["src", "docs"], fullReload: false });
    await settle();

    expect(t.listCalls().slice(2)).toEqual([{ projectId: "p1", path: "src" }]);
    expect(t.rowPaths()).toEqual(["src", "src/a.ts", "src/new.txt"]);
  });

  it("全部読み直しの印で読み込み済みのフォルダをすべて読み直す", async () => {
    const t = setup();
    t.fs.set("p1:", listed(dir("src")));
    t.fs.set("p1:src", listed(file("a.ts")));
    t.show("p1");
    await settle();
    t.controller.expand("src");
    await settle();

    t.controller.handleTreeChange({ projectId: "p1", dirs: [], fullReload: true });
    await settle();

    expect(t.listCalls().slice(2).map((c) => c.path)).toEqual(["", "src"]);
  });

  it("選択中でないプロジェクトの_tree-changed_は受けない", async () => {
    const t = setup();
    t.fs.set("p1:", listed(file("a.ts")));
    t.show("p1");
    await settle();

    t.controller.handleTreeChange({ projectId: "p2", dirs: [""], fullReload: true });
    await settle();

    expect(t.listCalls()).toHaveLength(1);
  });

  it("保留中は読み直さず解けたらまとめて読み直す", async () => {
    const t = setup();
    t.fs.set("p1:", listed(dir("src"), dir("docs")));
    t.fs.set("p1:src", listed(file("a.ts")));
    t.fs.set("p1:docs", listed(file("b.md")));
    t.show("p1");
    await settle();
    t.controller.expand("src");
    t.controller.expand("docs");
    await settle();
    const before = t.listCalls().length;

    // メニューと名前の入力を開くと保留になる(FR-037)。
    t.ui.getState().openTreeMenu("p1", { kind: "blank" }, { x: 0, y: 0 });
    t.ui.getState().startInlineEdit({ mode: "rename", projectId: "p1", path: "src/a.ts" });
    expect([...t.tree.getState().gate.holds].sort()).toEqual(["contextMenu", "nameInput"]);
    t.fs.set("p1:src", listed(file("a.ts"), file("c.ts")));
    t.controller.handleTreeChange({ projectId: "p1", dirs: ["src"], fullReload: false });
    t.controller.handleTreeChange({ projectId: "p1", dirs: ["docs", "src"], fullReload: false });
    await settle();
    expect(t.listCalls()).toHaveLength(before);
    expect(t.rowPaths()).not.toContain("src/c.ts");

    // 理由が 1 つ残っているあいだはまだ読まない。
    t.ui.getState().closeTreeMenu();
    await settle();
    expect(t.listCalls()).toHaveLength(before);

    // すべて解けたら、積んだフォルダを重なりなしでまとめて読む。
    t.ui.getState().closeInlineEdit();
    await settle();
    expect(t.listCalls().slice(before).map((c) => c.path)).toEqual(["src", "docs"]);
    expect(t.rowPaths()).toContain("src/c.ts");
  });

  it("確認ダイアログとドラッグの間も保留する", () => {
    const t = setup();
    t.ui.getState().openTreeConflict({ request: { op: "paste", projectId: "p1", destDir: "" }, name: "a.ts", total: 1 });
    t.ui.getState().setTreeDragging(true);

    expect([...t.tree.getState().gate.holds].sort()).toEqual(["dialog", "drag"]);

    t.ui.getState().closeTreeConflict();
    t.ui.getState().openMoveConfirmation({ projectId: "p1", source: "a", destDir: "b" });
    t.ui.getState().setTreeDragging(false);
    expect([...t.tree.getState().gate.holds]).toEqual(["dialog"]);
  });

  it("プロジェクトを切り替えて戻ると読み込み済みのツリーを保ち読み直す", async () => {
    const t = setup();
    t.fs.set("p1:", listed(dir("src")));
    t.fs.set("p1:src", listed(file("a.ts")));
    t.fs.set("p2:", listed(file("other.txt")));
    t.show("p1");
    await settle();
    t.controller.expand("src");
    t.controller.select("src/a.ts");
    await settle();

    t.show("p2");
    await settle();
    expect(t.rowPaths()).toEqual(["other.txt"]);

    const before = t.listCalls().length;
    t.show("p1");
    // 戻った直後から、前の展開と選択のまま行が出る。
    expect(t.rowPaths()).toEqual(["src", "src/a.ts"]);
    expect(activeTreeModel(t.tree.getState())?.selected).toBe("src/a.ts");
    await settle();
    expect(t.listCalls().slice(before)).toEqual([
      { projectId: "p1", path: "" },
      { projectId: "p1", path: "src" },
    ]);
  });

  it("切り替えたあとに届いた前のプロジェクトの応答は捨てる", async () => {
    const t = setup();
    t.fs.set("p2:", listed(file("other.txt")));
    t.holdKeys.add("p1:");
    t.show("p1");
    await settle();

    t.show("p2");
    await settle();
    t.held.get("p1:")?.[0]?.resolve(listed(file("late.txt")));
    await settle();

    expect(t.tree.getState().trees.get("p1")?.listings.size).toBe(0);
    expect(t.rowPaths()).toEqual(["other.txt"]);
  });

  it("戻ったときは読み込み中だったフォルダも求め直し_前の応答は捨てる", async () => {
    const t = setup();
    t.holdKeys.add("p1:");
    t.fs.set("p2:", listed());
    t.show("p1");
    await settle();
    t.show("p2");
    t.show("p1");
    await settle();

    const [first, second] = t.held.get("p1:") ?? [];
    second?.resolve(listed(file("new.txt")));
    await settle();
    first?.resolve(listed(file("old.txt")));
    await settle();

    expect(t.rowPaths()).toEqual(["new.txt"]);
  });

  it("同じフォルダを求め直したら古い要求の応答は捨てる", async () => {
    const t = setup();
    t.fs.set("p1:", listed(dir("src")));
    t.show("p1");
    await settle();
    t.holdKeys.add("p1:");
    t.controller.handleTreeChange({ projectId: "p1", dirs: [""], fullReload: false });
    t.controller.handleTreeChange({ projectId: "p1", dirs: [""], fullReload: false });
    const [older, newer] = t.held.get("p1:") ?? [];

    newer?.resolve(listed(dir("src"), file("b.ts")));
    await settle();
    older?.resolve(listed(file("stale.ts")));
    await settle();

    expect(t.rowPaths()).toEqual(["src", "b.ts"]);
  });

  it("見つからない_フォルダでない_ルートの外と返ったフォルダは忘れる", async () => {
    const t = setup();
    t.fs.set("p1:", listed(dir("a"), dir("b"), dir("c")));
    t.fs.set("p1:a", { kind: "notFound" });
    t.fs.set("p1:b", { kind: "notADirectory" });
    t.fs.set("p1:c", { kind: "outsideRoot" });
    t.show("p1");
    await settle();

    t.controller.expand("a");
    t.controller.expand("b");
    t.controller.expand("c");
    await settle();

    const model = activeTreeModel(t.tree.getState());
    expect([...(model?.expanded ?? [])]).toEqual([]);
    expect([...(model?.loading ?? [])]).toEqual([]);
  });

  it("見つからないプロジェクトはツリーを出さず読み込まない", async () => {
    const t = setup();

    t.show("p1", [project("p1", { missing: true })]);
    await settle();
    expect(t.tree.getState().activeProjectId).toBeNull();
    expect(t.listCalls()).toEqual([]);

    // 見つかったら読み込む。
    t.fs.set("p1:", listed(file("a.ts")));
    t.show("p1", [project("p1")]);
    await settle();
    expect(t.rowPaths()).toEqual(["a.ts"]);
  });

  it("写しから消えたプロジェクトのツリーを忘れる", async () => {
    const t = setup();
    t.fs.set("p1:", listed(file("a.ts")));
    t.fs.set("p2:", listed());
    t.show("p1");
    await settle();
    t.show("p2");

    t.show("p2", [project("p2")]);

    expect([...t.tree.getState().trees.keys()]).toEqual(["p2"]);
  });

  it("中身の問い合わせが失敗したら報告し読み込み中の印を外す", async () => {
    const t = setup();
    t.fs.set("p1:", listed(dir("src")));
    t.show("p1");
    await settle();
    t.holdKeys.add("p1:src");

    t.controller.expand("src");
    t.held.get("p1:src")?.[0]?.reject("読めない");
    await settle();

    expect(t.errors).toEqual(["フォルダの中身を読めなかった: 読めない"]);
    expect(activeTreeModel(t.tree.getState())?.loading.has("src")).toBe(false);
  });

  it("開いたフォルダの中身を読めなければ_知らせを出してフォルダを閉じる", async () => {
    const t = setup();
    t.fs.set("p1:", listed(dir("secret")));
    t.show("p1");
    await settle();
    t.holdKeys.add("p1:secret");

    t.controller.expand("secret");
    expect(activeTreeModel(t.tree.getState())?.expanded.has("secret")).toBe(true);
    t.held.get("p1:secret")?.[0]?.reject("権限がありません");
    await settle();

    const model = activeTreeModel(t.tree.getState());
    expect(model?.expanded.has("secret")).toBe(false);
    expect(model?.loading.has("secret")).toBe(false);
    expect(t.ui.getState().treeNotice?.message).toBe("フォルダを読めませんでした");
    // 診断への報告は今までどおり。
    expect(t.errors).toEqual(["フォルダの中身を読めなかった: 権限がありません"]);
  });

  it("隠しファイル表示の切替で読み直すと_切替の前に求めた応答は捨てる", async () => {
    const t = setup();
    t.fs.set("p1:", listed(file("a.ts")));
    t.show("p1");
    await settle();
    t.holdKeys.add("p1:");
    t.controller.handleTreeChange({ projectId: "p1", dirs: [""], fullReload: false });
    const [beforeToggle] = t.held.get("p1:") ?? [];

    t.controller.reloadAll("p1");
    const [, afterToggle] = t.held.get("p1:") ?? [];
    beforeToggle?.resolve(listed(file("a.ts")));
    await settle();
    afterToggle?.resolve(listed(file(".env"), file("a.ts")));
    await settle();

    expect(t.rowPaths()).toEqual([".env", "a.ts"]);
  });

  it("選んでいた項目が読み直しで消えたら選択を外す", async () => {
    const t = setup();
    t.fs.set("p1:", listed(file("a.ts"), file("b.ts")));
    t.show("p1");
    await settle();
    t.controller.select("a.ts");
    t.fs.set("p1:", listed(file("b.ts")));

    t.controller.handleTreeChange({ projectId: "p1", dirs: [""], fullReload: false });
    await settle();

    expect(activeTreeModel(t.tree.getState())?.selected).toBeNull();
  });

  it("プロジェクトを切り替えると積んだフォルダを捨てる", async () => {
    const t = setup();
    t.fs.set("p1:", listed(file("a.ts")));
    t.fs.set("p2:", listed());
    t.show("p1");
    await settle();
    t.ui.getState().openTreeMenu("p1", { kind: "blank" }, { x: 0, y: 0 });
    t.controller.handleTreeChange({ projectId: "p1", dirs: [""], fullReload: false });
    expect(t.tree.getState().gate.pending).toEqual([""]);

    t.show("p2");

    expect(t.tree.getState().gate.pending).toEqual([]);
  });

  /** p1 のツリーに、表示だけの状態(メニュー・入力・確認・ドラッグ・フォーカス)をすべて立てる。 */
  const openEverything = (t: ReturnType<typeof setup>, projectId = "p1") => {
    const s = t.ui.getState();
    s.openTreeMenu(projectId, { kind: "blank" }, { x: 0, y: 0 });
    s.startInlineEdit({ mode: "rename", projectId, path: "a.ts" });
    s.openTreeConflict({ request: { op: "paste", projectId, destDir: "" }, name: "a.ts", total: 1 });
    s.openMoveConfirmation({ projectId, source: "a.ts", destDir: "src" });
    s.setTreeDragging(true);
    s.setTreeFocused(true);
  };
  const leftovers = (t: ReturnType<typeof setup>) => {
    const s = t.ui.getState();
    return {
      treeMenu: s.treeMenu,
      inlineEdit: s.inlineEdit,
      treeConflict: s.treeConflict,
      moveConfirmation: s.moveConfirmation,
      treeDragging: s.treeDragging,
      treeFocused: s.treeFocused,
    };
  };
  const NOTHING_LEFT = {
    treeMenu: null,
    inlineEdit: null,
    treeConflict: null,
    moveConfirmation: null,
    treeDragging: false,
    treeFocused: false,
  };

  it("表示中のプロジェクトが見つからなくなると_そのツリーの表示だけの状態を消し_保留を解く", async () => {
    const t = setup();
    t.fs.set("p1:", listed(file("a.ts")));
    t.show("p1");
    await settle();
    openEverything(t);
    expect([...t.tree.getState().gate.holds].sort()).toEqual(["contextMenu", "dialog", "drag", "nameInput"]);

    t.show("p1", [project("p1", { missing: true }), project("p2")]);

    expect(leftovers(t)).toEqual(NOTHING_LEFT);
    expect([...t.tree.getState().gate.holds]).toEqual([]);
  });

  it("選択中のプロジェクトが変わると_前のプロジェクトの表示だけの状態を消す", async () => {
    const t = setup();
    t.fs.set("p1:", listed(file("a.ts")));
    t.fs.set("p2:", listed());
    t.show("p1");
    await settle();
    openEverything(t);

    t.show("p2");

    expect(leftovers(t)).toEqual(NOTHING_LEFT);
  });

  it("消えたプロジェクトの確認は_一覧から削除したあとの別のプロジェクトに出ない", async () => {
    const t = setup();
    t.fs.set("p1:", listed(file("a.ts")));
    t.fs.set("p2:", listed());
    t.show("p1");
    await settle();
    t.ui.getState().openTreeConflict({ request: { op: "paste", projectId: "p1", destDir: "" }, name: "a.ts", total: 1 });
    t.show("p1", [project("p1", { missing: true }), project("p2")]);

    // 一覧から削除すると、p2 が選ばれる。
    t.show("p2", [project("p2")]);

    expect(t.ui.getState().treeConflict).toBeNull();
    expect(t.ui.getState().moveConfirmation).toBeNull();
  });

  it("表示中のプロジェクトが変わらない写しの更新では消さない", async () => {
    const t = setup();
    t.fs.set("p1:", listed(file("a.ts")));
    t.show("p1");
    await settle();
    openEverything(t);

    t.show("p1", [project("p1", { showHidden: true }), project("p2")]);

    expect(leftovers(t)).toEqual({
      treeMenu: expect.objectContaining({ projectId: "p1" }),
      inlineEdit: expect.objectContaining({ projectId: "p1" }),
      treeConflict: expect.objectContaining({ name: "a.ts" }),
      moveConfirmation: expect.objectContaining({ projectId: "p1" }),
      treeDragging: true,
      treeFocused: true,
    });
  });

  it("破棄したあとに届いた応答と変更では状態を変えない", async () => {
    const t = setup();
    t.holdKeys.add("p1:");
    t.show("p1");
    await settle();
    const before = t.tree.getState();

    t.controller.dispose();
    t.held.get("p1:")?.[0]?.resolve(listed(file("a.ts")));
    t.controller.handleTreeChange({ projectId: "p1", dirs: [""], fullReload: true });
    t.show("p2");
    t.ui.getState().openTreeMenu("p1", { kind: "blank" }, { x: 0, y: 0 });
    t.controller.expand("src");
    await settle();

    expect(t.tree.getState()).toBe(before);
    expect(t.listCalls()).toHaveLength(1);
  });

  it("生成関数ごとに別のストアになる", () => {
    const a = createTreeStore();
    const b = createTreeStore();

    a.setState({ activeProjectId: "p1" });

    expect(b.getState().activeProjectId).toBeNull();
  });
});
