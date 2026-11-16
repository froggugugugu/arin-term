import { describe, expect, it } from "vitest";

import { createIpc } from "../ipc/client";
import type { DirListing } from "../ipc/generated/DirListing";
import type { FileOpOutcome } from "../ipc/generated/FileOpOutcome";
import type { RunningTerminal } from "../ipc/generated/RunningTerminal";
import { createFakeIpc, deferred, type FakeHandlers, sampleSnapshot, sampleTerminal } from "../test/fakeIpc";
import { createActions } from "./actions";
import { createAppStore } from "./appStore";
import { activeTreeModel, activeTreeRows } from "./selectors";
import { createTreeController, createTreeStore } from "./treeStore";
import { createUiStore } from "./uiStore";

function setup(handlers: Partial<FakeHandlers> = {}, fs: Map<string, DirListing> = new Map()) {
  const fake = createFakeIpc({
    list_directory: ({ projectId, path }) => fs.get(`${projectId}:${path}`) ?? { kind: "notFound" },
    ...handlers,
  });
  const ipc = createIpc(fake.transport);
  const ui = createUiStore();
  const app = createAppStore();
  const tree = createTreeStore();
  const errors: string[] = [];
  const reportError = (m: string) => errors.push(m);
  const treeController = createTreeController({ ipc, tree, app, ui, reportError });
  const actions = createActions({ ipc, ui, app, tree, treeController, reportError });
  return { fake, ui, app, tree, errors, actions, fs };
}

const running: RunningTerminal = { terminalId: "t1", projectId: "p1", projectName: "app", label: "ターミナル 1" };

describe("actions(文字の大きさ・透過と設定ファイル。M4.6・M4.7)", () => {
  it("刻むと_step_font_size_を差分で呼ぶ", async () => {
    const { fake, errors, actions } = setup();

    await actions.stepFontSize("terminal", 1);
    await actions.stepFontSize("interface", -1);
    await actions.stepFontSize("terminal", 1);

    // 今の値を足した数ではなく差分を送る(写しが届く前に続けて押しても押した回数どおりに動く。ADR-013 の 2)
    expect(fake.calls).toEqual([
      { command: "step_font_size", args: { target: "terminal", delta: 1 } },
      { command: "step_font_size", args: { target: "interface", delta: -1 } },
      { command: "step_font_size", args: { target: "terminal", delta: 1 } },
    ]);
    expect(errors).toEqual([]);
  });

  it("透過を刻むと_step_translucency_を%の差分で呼ぶ", async () => {
    const { fake, errors, actions } = setup();

    await actions.stepTranslucency("window", 5);
    await actions.stepTranslucency("terminal", -5);

    expect(fake.calls).toEqual([
      { command: "step_translucency", args: { target: "window", delta: 5 } },
      { command: "step_translucency", args: { target: "terminal", delta: -5 } },
    ]);
    expect(errors).toEqual([]);
  });

  it("既定に戻すで_reset_appearance_を呼ぶ", async () => {
    const { fake, errors, actions } = setup();

    await actions.resetAppearance();

    expect(fake.calls).toEqual([{ command: "reset_appearance", args: undefined }]);
    expect(errors).toEqual([]);
  });

  it("設定ファイルを開くで_open_settings_file_を呼ぶ", async () => {
    const { fake, errors, actions } = setup();

    await actions.openSettingsFile();

    expect(fake.calls).toEqual([{ command: "open_settings_file", args: undefined }]);
    expect(errors).toEqual([]);
  });

  it("文字の大きさと設定ファイルのコマンドの失敗は報告し_Promise_は失敗しない", async () => {
    const { errors, actions } = setup({
      step_font_size: () => {
        throw "壊れた";
      },
      step_translucency: () => {
        throw "壊れた";
      },
      reset_appearance: () => {
        throw "壊れた";
      },
      open_settings_file: () => {
        throw "開けない";
      },
    });

    await expect(actions.stepFontSize("terminal", 1)).resolves.toBeUndefined();
    await expect(actions.stepTranslucency("window", 5)).resolves.toBeUndefined();
    await expect(actions.resetAppearance()).resolves.toBeUndefined();
    await expect(actions.openSettingsFile()).resolves.toBeUndefined();

    expect(errors).toEqual([
      "文字の大きさを変えられなかった: 壊れた",
      "透過を変えられなかった: 壊れた",
      "見た目を既定に戻せなかった: 壊れた",
      "設定ファイルを開けなかった: 開けない",
    ]);
  });
});

describe("actions(コマンドの包み)", () => {
  it("actions_は正しいコマンドを呼ぶ", async () => {
    const { fake, errors, actions } = setup();

    await actions.addProjectByDialog();
    await actions.selectProject("p1");
    await actions.relinkProject("p1");
    await actions.setProjectColor("p1", "green");
    await actions.copyProjectPath("p2");
    await actions.newTerminal();
    await actions.closeTerminal("t1");
    await actions.selectTerminal("t2");
    await actions.setPaneSizes(300, 150);
    await actions.setSidebarCollapsed(true);

    expect(fake.calls).toEqual([
      { command: "add_project_by_dialog", args: undefined },
      { command: "select_project", args: { projectId: "p1" } },
      { command: "relink_project", args: { projectId: "p1" } },
      { command: "set_project_color", args: { projectId: "p1", color: "green" } },
      { command: "copy_project_path", args: { projectId: "p2" } },
      { command: "new_terminal", args: undefined },
      { command: "close_terminal", args: { terminalId: "t1" } },
      { command: "select_terminal", args: { terminalId: "t2" } },
      { command: "set_pane_sizes", args: { sidebarWidth: 300, cardAreaHeight: 150 } },
      { command: "set_sidebar_collapsed", args: { collapsed: true } },
    ]);
    expect(errors).toEqual([]);
  });

  it("コマンドの失敗は報告し、Promise は失敗しない", async () => {
    const { errors, actions } = setup({
      select_project: () => {
        throw "見つからない";
      },
      new_terminal: () => {
        throw new Error("プロジェクトが無い");
      },
    });

    await expect(actions.selectProject("p9")).resolves.toBeUndefined();
    await expect(actions.newTerminal()).resolves.toBeUndefined();

    expect(errors).toEqual([
      "プロジェクトを選べなかった: 見つからない",
      "新しいターミナルを開けなかった: プロジェクトが無い",
    ]);
  });
});

describe("actions(表示の操作。T47e)", () => {
  /** a1・a2(a1 にフォーカス)のタイル表示の写しを入れる。 */
  function displaySetup(handlers: Partial<FakeHandlers> = {}) {
    const t = setup(handlers);
    t.app.getState().replaceSnapshot(
      sampleSnapshot({
        terminals: [sampleTerminal("a1", { kind: "running" }), sampleTerminal("a2", { kind: "running" })],
        display: {
          mode: "tiles",
          maximized: false,
          page: 0,
          pageCount: 1,
          grid: { cols: 2, rows: 1 },
          tileIds: ["a1", "a2"],
          focusedTerminalId: "a1",
          attentionPages: [],
        },
      }),
    );
    return t;
  }

  it("表示モードの切替でコマンドを呼ぶ", async () => {
    const { fake, errors, actions } = displaySetup();

    await actions.setDisplayMode("tabs");
    await actions.setDisplayMode("tiles");

    expect(fake.calls).toEqual([
      { command: "set_display_mode", args: { mode: "tabs" } },
      { command: "set_display_mode", args: { mode: "tiles" } },
    ]);
    expect(errors).toEqual([]);
  });

  it("フォーカス中のタイルをクリックしてもコマンドを呼ばない", async () => {
    const { fake, actions } = displaySetup();

    await actions.focusTile("a1");

    expect(fake.calls).toEqual([]);
  });

  it("別のタイルへのフォーカスでコマンドを呼ぶ", async () => {
    const { fake, actions } = displaySetup();

    await actions.focusTile("a2");

    expect(fake.calls).toEqual([{ command: "focus_terminal", args: { terminalId: "a2" } }]);
  });

  it("同じタイルへのフォーカスは写しが届くまで送り直さず_届いたら送る", async () => {
    const { fake, actions, app } = displaySetup();

    await actions.focusTile("a2");
    await actions.focusTile("a2");
    expect(fake.calls).toEqual([{ command: "focus_terminal", args: { terminalId: "a2" } }]);

    // 別の写しが届いた(フォーカス中は a1 のまま。Rust 側でタブが閉じたなど)ら、もう一度求められる。
    const snapshot = app.getState().snapshot;
    if (snapshot === null) throw new Error("写しが無い");
    app.getState().replaceSnapshot({ ...snapshot });
    await actions.focusTile("a2");
    expect(fake.calls).toHaveLength(2);
  });

  it("フォーカスのコマンドが失敗したら同じタイルをもう一度求められる", async () => {
    let failing = true;
    const { fake, actions, errors } = displaySetup({
      focus_terminal: () => {
        if (failing) throw "一時的な失敗";
        return null;
      },
    });

    await actions.focusTile("a2");
    failing = false;
    await actions.focusTile("a2");

    expect(fake.calls).toHaveLength(2);
    expect(errors).toEqual(["タイルにフォーカスできなかった: 一時的な失敗"]);
  });

  it("写しがまだ無ければフォーカスのコマンドを呼ぶ", async () => {
    const { fake, actions } = setup();

    await actions.focusTile("a2");

    expect(fake.calls).toEqual([{ command: "focus_terminal", args: { terminalId: "a2" } }]);
  });

  it("ページ送りと拡大のコマンドを呼ぶ", async () => {
    const { fake, actions } = displaySetup();

    await actions.nextTilePage();
    await actions.previousTilePage();
    await actions.toggleTileMaximize();

    expect(fake.calls).toEqual([
      { command: "next_tile_page", args: undefined },
      { command: "previous_tile_page", args: undefined },
      { command: "toggle_tile_maximize", args: undefined },
    ]);
  });

  it("タイトルバーのダブルクリックの拡大は対象のタイルの_ID_を渡す", async () => {
    const { fake, actions } = displaySetup();

    await actions.toggleTileMaximize("a2");

    expect(fake.calls).toEqual([{ command: "toggle_tile_maximize", args: { terminalId: "a2" } }]);
  });

  it("表示の操作の失敗は報告し_Promise_は失敗しない", async () => {
    const fail = () => {
      throw "写しが無い";
    };
    const { errors, actions } = displaySetup({
      set_display_mode: fail,
      focus_terminal: fail,
      next_tile_page: fail,
      previous_tile_page: fail,
      toggle_tile_maximize: fail,
    });

    await expect(actions.setDisplayMode("tabs")).resolves.toBeUndefined();
    await expect(actions.focusTile("a2")).resolves.toBeUndefined();
    await expect(actions.nextTilePage()).resolves.toBeUndefined();
    await expect(actions.previousTilePage()).resolves.toBeUndefined();
    await expect(actions.toggleTileMaximize()).resolves.toBeUndefined();

    expect(errors).toEqual([
      "表示モードを切り替えられなかった: 写しが無い",
      "タイルにフォーカスできなかった: 写しが無い",
      "次のページに送れなかった: 写しが無い",
      "前のページに戻せなかった: 写しが無い",
      "タイルを拡大・元に戻せなかった: 写しが無い",
    ]);
  });
});

describe("actions(プロジェクトの削除の確認。FR-003)", () => {
  it("動作中が無ければ確認せずに削除する", async () => {
    const { fake, ui, actions } = setup();

    await actions.removeProject("p1");

    expect(fake.calls).toEqual([{ command: "remove_project", args: { projectId: "p1", confirmed: false } }]);
    expect(ui.getState().removeConfirmation).toBeNull();
  });

  it("動作中があれば確認を開き、確認すると確認済みで呼び直して閉じる", async () => {
    const { fake, ui, actions } = setup({
      remove_project: ({ confirmed }) => (confirmed ? { kind: "removed" } : { kind: "needsConfirmation", running: [running], unsaved: [] }),
    });

    await actions.removeProject("p1");
    expect(ui.getState().removeConfirmation).toEqual({ projectId: "p1", running: [running] });

    await actions.confirmRemoveProject();

    expect(fake.calls.map((c) => c.args)).toEqual([
      { projectId: "p1", confirmed: false },
      { projectId: "p1", confirmed: true },
    ]);
    expect(ui.getState().removeConfirmation).toBeNull();
  });

  it("中止すると確認を閉じ、削除のコマンドを呼ばない", async () => {
    const { fake, ui, actions } = setup({
      remove_project: () => ({ kind: "needsConfirmation", running: [running], unsaved: [] }),
    });
    await actions.removeProject("p1");

    actions.cancelRemoveProject();
    await actions.confirmRemoveProject();

    expect(ui.getState().removeConfirmation).toBeNull();
    expect(fake.calls).toHaveLength(1);
  });

  it("確認済みで呼んでもまだ確認を求められたら、最新の一覧で確認を開き直す", async () => {
    const other: RunningTerminal = { ...running, terminalId: "t2", label: "ターミナル 2" };
    const { ui, actions } = setup({
      remove_project: ({ confirmed }) => ({ kind: "needsConfirmation", running: confirmed ? [other] : [running], unsaved: [] }),
    });
    await actions.removeProject("p1");

    await actions.confirmRemoveProject();

    expect(ui.getState().removeConfirmation).toEqual({ projectId: "p1", running: [other] });
  });

  it("削除の失敗は報告し、確認は閉じたまま", async () => {
    const { ui, errors, actions } = setup({
      remove_project: () => {
        throw "見つからない";
      },
    });

    await actions.removeProject("p1");

    expect(errors).toEqual(["プロジェクトを一覧から削除できなかった: 見つからない"]);
    expect(ui.getState().removeConfirmation).toBeNull();
  });
});

describe("actions(終了の確認。FR-063)", () => {
  it("終了で確認を閉じて confirm_quit を呼ぶ", async () => {
    const { fake, ui, actions } = setup();
    ui.getState().openQuitConfirmation([running]);

    await actions.confirmQuit();

    expect(fake.calls).toEqual([{ command: "confirm_quit", args: undefined }]);
    expect(ui.getState().quitConfirmation).toBeNull();
  });

  it("取り消しは確認を消すだけで、何も呼ばない。確認が開いていなければ終了も呼ばない", async () => {
    const { fake, ui, actions } = setup();
    ui.getState().openQuitConfirmation([running]);

    actions.cancelQuit();
    await actions.confirmQuit();

    expect(ui.getState().quitConfirmation).toBeNull();
    expect(fake.calls).toEqual([]);
  });

  it("confirm_quit の失敗は報告する", async () => {
    const { errors, ui, actions } = setup({
      confirm_quit: () => {
        throw "保存できなかった";
      },
    });
    ui.getState().openQuitConfirmation([running]);

    await actions.confirmQuit();

    expect(errors).toEqual(["アプリを終了できなかった: 保存できなかった"]);
  });
});

// ── ツリー(M3。T43a) ──

const dir = (name: string) => ({ name, kind: "directory" as const });
const file = (name: string) => ({ name, kind: "file" as const });
const listed = (...entries: ReturnType<typeof dir | typeof file>[]): DirListing => ({ kind: "listed", entries });
const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

/** p1 を選んだ写しを入れ、ルートと src を読み込んだ状態にする。 */
async function treeSetup(handlers: Partial<FakeHandlers> = {}) {
  const fs = new Map<string, DirListing>([
    ["p1:", listed(dir("src"), dir("docs"), file("a.md"))],
    ["p1:src", listed(file("a.ts"))],
    ["p1:docs", listed()],
  ]);
  const t = setup(handlers, fs);
  t.app.getState().replaceSnapshot(
    sampleSnapshot({
      projects: [
        {
          id: "p1",
          name: "app",
          color: "blue",
          displayPath: "~/dev/app",
          missing: false,
          showHidden: false,
          terminalIds: [],
          activeTerminalId: null,
          editorIds: [],
          shownEditorId: null,
        },
      ],
      selectedProjectId: "p1",
    }),
  );
  await settle();
  t.actions.expandDir("src");
  await settle();
  t.fake.calls.length = 0;
  const lists = () => t.fake.calls.filter((c) => c.command === "list_directory").map((c) => (c.args as { path: string }).path);
  const ops = () => t.fake.calls.filter((c) => c.command !== "list_directory");
  const selected = () => activeTreeModel(t.tree.getState())?.selected;
  return { ...t, lists, ops, selected };
}

const conflict = (name: string): FileOpOutcome => ({ kind: "conflict", name, total: 1 });

describe("actions(ツリーの開閉と選択)", () => {
  it("開閉と選択は表示中のツリーに効く", async () => {
    const t = await treeSetup();

    t.actions.selectTreePath("src/a.ts");
    t.actions.toggleDir("docs");
    await settle();
    expect(t.lists()).toEqual(["docs"]);
    t.actions.collapseDir("src");
    // 隠れた選択は閉じたフォルダに移る(T41a)。
    expect(t.selected()).toBe("src");

    t.actions.collapseAllDirs();
    expect(activeTreeRows(t.tree.getState()).map((r) => r.path)).toEqual(["src", "docs", "a.md"]);
  });
});

describe("actions(ファイル操作の応答)", () => {
  it("新規作成が済んだら作った項目を選ぶ", async () => {
    const t = await treeSetup({ create_entry: () => ({ kind: "done", select: "src/new.ts" }) });
    t.fs.set("p1:src", listed(file("a.ts"), file("new.ts")));

    await t.actions.createEntry("p1", "src", "new.ts", "file");
    await settle();

    expect(t.ops()).toEqual([
      { command: "create_entry", args: { projectId: "p1", parent: "src", name: "new.ts", kind: "file" } },
    ]);
    expect(t.lists()).toEqual(["src"]);
    expect(t.selected()).toBe("src/new.ts");
    expect(activeTreeRows(t.tree.getState()).map((r) => r.path)).toContain("src/new.ts");
  });

  it("閉じたフォルダに作ったら祖先を開いて選ぶ", async () => {
    const t = await treeSetup({ create_entry: () => ({ kind: "done", select: "docs/n" }) });
    t.fs.set("p1:docs", listed(dir("n")));

    await t.actions.createEntry("p1", "docs", "n", "folder");
    await settle();

    expect(activeTreeModel(t.tree.getState())?.expanded.has("docs")).toBe(true);
    expect(t.selected()).toBe("docs/n");
  });

  it("インライン編集を確定すると入力を閉じてから名称変更を呼ぶ", async () => {
    let openDuringCall: unknown = "未呼び出し";
    const t = await treeSetup({
      rename_entry: () => {
        openDuringCall = t.ui.getState().inlineEdit;
        return { kind: "done", select: "src/b.ts" };
      },
    });
    t.ui.getState().startInlineEdit({ mode: "rename", projectId: "p1", path: "src/a.ts" });

    await t.actions.commitInlineEdit("b.ts");

    expect(openDuringCall).toBeNull();
    expect(t.ops()).toEqual([
      { command: "rename_entry", args: { projectId: "p1", path: "src/a.ts", newName: "b.ts" } },
    ]);
  });

  it("インライン編集の新規作成を確定すると作成を呼び_編集が無ければ何も呼ばない", async () => {
    const t = await treeSetup();
    await t.actions.commitInlineEdit("x");
    expect(t.ops()).toEqual([]);

    t.ui.getState().startInlineEdit({ mode: "create", projectId: "p1", parent: "", kind: "folder" });
    await t.actions.commitInlineEdit("lib");

    expect(t.ops()).toEqual([
      { command: "create_entry", args: { projectId: "p1", parent: "", name: "lib", kind: "folder" } },
    ]);
  });

  it("名称変更が見つからないを返したら知らせを出し親を読み直す", async () => {
    const t = await treeSetup({ rename_entry: () => ({ kind: "notFound" }) });
    t.fs.set("p1:src", listed());

    await t.actions.renameEntry("p1", "src/a.ts", "b.ts");
    await settle();

    expect(t.ui.getState().treeNotice?.message).toBe("「a.ts」が見つかりません");
    expect(t.lists()).toEqual(["src"]);
    expect(activeTreeRows(t.tree.getState()).map((r) => r.path)).not.toContain("src/a.ts");
  });

  it("衝突を返したら衝突の確認を開き選んだ対処で呼び直す", async () => {
    const replies: FileOpOutcome[] = [conflict("a.ts"), { kind: "done", select: "src/a 2.ts" }];
    const t = await treeSetup({ paste_entries: () => replies.shift() ?? { kind: "done", select: null } });
    t.fs.set("p1:src", listed(file("a 2.ts"), file("a.ts")));

    await t.actions.pasteEntries("p1", "src");
    expect(t.ui.getState().treeConflict).toEqual({
      request: { op: "paste", projectId: "p1", destDir: "src" },
      name: "a.ts",
      total: 1,
    });
    // 確認の間は更新を保留する。
    expect(t.tree.getState().gate.holds.has("dialog")).toBe(true);

    await t.actions.resolveConflict("keepBoth");

    expect(t.ui.getState().treeConflict).toBeNull();
    expect(t.tree.getState().gate.holds.has("dialog")).toBe(false);
    expect(t.ops()).toEqual([
      { command: "paste_entries", args: { projectId: "p1", destDir: "src" } },
      { command: "paste_entries", args: { projectId: "p1", destDir: "src", resolution: "keepBoth" } },
    ]);
    expect(t.selected()).toBe("src/a 2.ts");
  });

  it("衝突の確認で置き換えるを選ぶと置き換えるで呼び直す(作成・名称変更・移動・コピー)", async () => {
    const t = await treeSetup({
      create_entry: (a) => (a.resolution === undefined ? conflict("n") : { kind: "done", select: null }),
      rename_entry: (a) => (a.resolution === undefined ? conflict("b") : { kind: "done", select: null }),
      move_entry: (a) => (a.resolution === undefined ? conflict("a.md") : { kind: "done", select: null }),
      copy_entry: (a) => (a.resolution === undefined ? conflict("a.md") : { kind: "done", select: null }),
    });

    await t.actions.createEntry("p1", "", "n", "file");
    await t.actions.resolveConflict("replace");
    await t.actions.renameEntry("p1", "a.md", "b");
    await t.actions.resolveConflict("replace");
    await t.actions.moveEntry("p1", "a.md", "docs");
    await t.actions.resolveConflict("replace");
    await t.actions.copyEntry("p1", "a.md", "docs");
    await t.actions.resolveConflict("replace");

    expect(t.ops().map((c) => [c.command, (c.args as { resolution?: string }).resolution])).toEqual([
      ["create_entry", undefined],
      ["create_entry", "replace"],
      ["rename_entry", undefined],
      ["rename_entry", "replace"],
      ["move_entry", undefined],
      ["move_entry", "replace"],
      ["copy_entry", undefined],
      ["copy_entry", "replace"],
    ]);
  });

  it("複数の項目が衝突したら_衝突した数を確認に載せる", async () => {
    const t = await treeSetup({ paste_entries: () => ({ kind: "conflict", name: "a.md", total: 3 }) });

    await t.actions.pasteEntries("p1", "docs");

    expect(t.ui.getState().treeConflict).toEqual({
      request: { op: "paste", projectId: "p1", destDir: "docs" },
      name: "a.md",
      total: 3,
    });
  });

  it("衝突の確認で中止なら呼び直さない", async () => {
    const t = await treeSetup({ rename_entry: () => conflict("b.ts") });
    await t.actions.renameEntry("p1", "src/a.ts", "b.ts");

    t.actions.cancelConflict();
    await t.actions.resolveConflict("replace");

    expect(t.ui.getState().treeConflict).toBeNull();
    expect(t.ops()).toHaveLength(1);
  });

  it("ドロップでの移動は確認を開き確認すると移動のコマンドを呼ぶ", async () => {
    const t = await treeSetup({ move_entry: () => ({ kind: "done", select: "docs/a.md" }) });
    t.fs.set("p1:", listed(dir("src"), dir("docs")));
    t.fs.set("p1:docs", listed(file("a.md")));

    await t.actions.dropEntry("p1", "a.md", "docs", false);
    expect(t.ops()).toEqual([]);
    expect(t.ui.getState().moveConfirmation).toEqual({ projectId: "p1", source: "a.md", destDir: "docs" });

    await t.actions.confirmMove();
    await settle();

    expect(t.ui.getState().moveConfirmation).toBeNull();
    expect(t.ops()).toEqual([{ command: "move_entry", args: { projectId: "p1", source: "a.md", destDir: "docs" } }]);
    expect(t.selected()).toBe("docs/a.md");
  });

  it("移動の確認で中止なら何も呼ばない", async () => {
    const t = await treeSetup();

    await t.actions.dropEntry("p1", "a.md", "docs", false);
    t.actions.cancelMove();
    await t.actions.confirmMove();

    expect(t.ops()).toEqual([]);
  });

  it("option_のドロップは確認なしでコピーのコマンドを呼ぶ", async () => {
    const t = await treeSetup();

    await t.actions.dropEntry("p1", "a.md", "docs", true);

    expect(t.ui.getState().moveConfirmation).toBeNull();
    expect(t.ops()).toEqual([{ command: "copy_entry", args: { projectId: "p1", source: "a.md", destDir: "docs" } }]);
  });

  it("ゴミ箱に入れたら選択を外し親を読み直す", async () => {
    const t = await treeSetup();
    t.actions.selectTreePath("src/a.ts");
    t.fs.set("p1:src", listed());

    await t.actions.trashEntry("p1", "src/a.ts");
    await settle();

    expect(t.ops()).toEqual([{ command: "trash_entry", args: { projectId: "p1", path: "src/a.ts" } }]);
    expect(t.selected()).toBeNull();
    expect(t.lists()).toEqual(["src"]);
  });

  it("名前が正しくない_対象にできない_失敗の応答は1行の知らせを出す", async () => {
    const replies: FileOpOutcome[] = [
      { kind: "invalidName", reason: "containsSlash" },
      { kind: "notAllowed", reason: "rootItself" },
      { kind: "failed", message: "別のディスクへは移動できません" },
    ];
    const t = await treeSetup({ rename_entry: () => replies.shift() ?? { kind: "done", select: null } });
    const notices: string[] = [];

    for (let i = 0; i < 3; i++) {
      await t.actions.renameEntry("p1", "a.md", "x/y");
      notices.push(t.ui.getState().treeNotice?.message ?? "");
    }

    expect(notices).toEqual([
      "名前に「/」は使えません",
      "プロジェクトのフォルダ自体は対象にできません",
      "別のディスクへは移動できません",
    ]);
    expect(t.ui.getState().treeConflict).toBeNull();
  });

  it("隠しファイル表示の切替でコマンドを呼び読み直す", async () => {
    const t = await treeSetup();

    await t.actions.setShowHidden("p1", true);
    await settle();

    expect(t.ops()).toEqual([{ command: "set_show_hidden", args: { projectId: "p1", showHidden: true } }]);
    expect(t.lists()).toEqual(["", "src"]);
  });

  it("デスクトップとパスとペーストボードの操作は正しいコマンドを呼ぶ", async () => {
    const t = await treeSetup();

    await t.actions.revealEntry("p1", "src");
    await t.actions.openEntry("p1", "a.md");
    await t.actions.copyEntryPath("p1", "src/a.ts", "relative");
    await t.actions.insertEntryPath("p1", ["src/a.ts"], "t1");
    await t.actions.copyEntries("p1", ["a.md"]);
    await t.actions.cutEntries("p1", ["a.md"]);

    expect(t.ops()).toEqual([
      { command: "reveal_entry", args: { projectId: "p1", path: "src" } },
      { command: "open_entry", args: { projectId: "p1", path: "a.md" } },
      { command: "copy_entry_path", args: { projectId: "p1", path: "src/a.ts", kind: "relative" } },
      { command: "insert_entry_path", args: { projectId: "p1", paths: ["src/a.ts"], terminalId: "t1" } },
      { command: "copy_entries", args: { projectId: "p1", paths: ["a.md"] } },
      { command: "cut_entries", args: { projectId: "p1", paths: ["a.md"] } },
    ]);
    expect(t.ui.getState().treeNotice).toBeNull();
  });

  it("開く対象が見つからないと知らせを出す", async () => {
    const t = await treeSetup({ open_entry: () => ({ kind: "notFound" }) });

    await t.actions.openEntry("p1", "src/a.ts");

    expect(t.ui.getState().treeNotice?.message).toBe("「a.ts」が見つかりません");
  });

  it("ファイル操作のコマンドの失敗は報告し_Promise_は失敗しない", async () => {
    const t = await treeSetup({
      trash_entry: () => {
        throw "ゴミ箱に届かない";
      },
      set_show_hidden: () => {
        throw "保存できない";
      },
    });

    await expect(t.actions.trashEntry("p1", "a.md")).resolves.toBeUndefined();
    await expect(t.actions.setShowHidden("p1", true)).resolves.toBeUndefined();

    expect(t.errors).toEqual([
      "ゴミ箱に入れられなかった: ゴミ箱に届かない",
      "隠しファイルの表示を切り替えられなかった: 保存できない",
    ]);
    expect(t.lists()).toEqual([]);
  });
});

describe("actions(切り替えたあとに届いた前のプロジェクトの応答。M3 の残り。T47e)", () => {
  const card = (id: string) => ({
    id,
    name: id,
    color: "blue" as const,
    displayPath: `~/${id}`,
    missing: false,
    showHidden: false,
    terminalIds: [],
    activeTerminalId: null,
    editorIds: [],
    shownEditorId: null,
  });
  const select = (t: ReturnType<typeof setup>, selectedProjectId: string) =>
    t.app.getState().replaceSnapshot(sampleSnapshot({ projects: [card("p1"), card("p2")], selectedProjectId }));

  /** p1 を表示中にし、ファイル操作の応答を遅らせる。 */
  async function switchSetup() {
    const reply = deferred<FileOpOutcome>();
    const t = setup(
      {
        paste_entries: () => reply.promise,
        rename_entry: () => reply.promise,
        move_entry: () => reply.promise,
      },
      new Map([
        ["p1:", listed(dir("src"))],
        ["p2:", listed(file("b.md"))],
      ]),
    );
    select(t, "p1");
    await settle();
    return { ...t, reply };
  }

  it("プロジェクトを替えたあとに前のプロジェクトの衝突の応答が届いても確認を開かない", async () => {
    const t = await switchSetup();

    const pending = t.actions.pasteEntries("p1", "src");
    select(t, "p2");
    t.reply.resolve(conflict("a.ts"));
    await pending;

    expect(t.ui.getState().treeConflict).toBeNull();
    expect(t.tree.getState().gate.holds.has("dialog")).toBe(false);
    expect(t.errors).toEqual([]);
  });

  it("プロジェクトを替えたあとに届いた前のプロジェクトの失敗の応答では知らせを出さない", async () => {
    const t = await switchSetup();

    const pending = t.actions.renameEntry("p1", "src", "lib");
    select(t, "p2");
    t.reply.resolve({ kind: "failed", message: "書き込めません" });
    await pending;

    expect(t.ui.getState().treeNotice).toBeNull();
  });

  it("前のプロジェクトの衝突の確認で対処を選んだ応答がプロジェクトを替えたあとに届いても確認を開き直さない", async () => {
    const t = await switchSetup();
    t.ui.getState().openTreeConflict({ request: { op: "move", projectId: "p1", source: "a", destDir: "src" }, name: "a", total: 1 });

    const pending = t.actions.resolveConflict("keepBoth");
    select(t, "p2");
    t.reply.resolve(conflict("a"));
    await pending;

    expect(t.ui.getState().treeConflict).toBeNull();
  });

  it("表示中でないプロジェクトへのドロップでは移動の確認を開かない", async () => {
    const t = await switchSetup();
    select(t, "p2");
    await settle();

    await t.actions.dropEntry("p1", "src", "", false);

    expect(t.ui.getState().moveConfirmation).toBeNull();
    expect(t.fake.calls.filter((c) => c.command !== "list_directory")).toEqual([]);
  });

  it("表示中のプロジェクトの応答なら今までどおり確認を開く", async () => {
    const t = await switchSetup();

    const pending = t.actions.pasteEntries("p1", "src");
    t.reply.resolve(conflict("a.ts"));
    await pending;

    expect(t.ui.getState().treeConflict?.request).toEqual({ op: "paste", projectId: "p1", destDir: "src" });
  });
});
