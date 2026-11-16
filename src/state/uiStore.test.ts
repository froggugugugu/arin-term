import { describe, expect, it } from "vitest";

import type { RunningTerminal } from "../ipc/generated/RunningTerminal";
import { DEFAULT_SHORTCUT_TABLE } from "../logic/keys/defaultShortcuts";
import { createUiStore, editorViewMode } from "./uiStore";

const running = (terminalId: string, label = "ターミナル 1"): RunningTerminal => ({
  terminalId,
  projectId: "p1",
  projectName: "app",
  label,
});

describe("uiStore(表示だけの状態)", () => {
  it("作った直後は確認もメニューも開いていない", () => {
    const ui = createUiStore().getState();

    expect(ui.quitConfirmation).toBeNull();
    expect(ui.removeConfirmation).toBeNull();
    expect(ui.cardMenu).toBeNull();
  });

  it("終了の確認の要求で確認が開き、閉じると消える", () => {
    const ui = createUiStore();

    ui.getState().openQuitConfirmation([running("t1")]);
    expect(ui.getState().quitConfirmation).toEqual({ running: [running("t1")], unsaved: [] });

    ui.getState().closeQuitConfirmation();
    expect(ui.getState().quitConfirmation).toBeNull();
  });

  it("終了の確認の要求が複数回届いても確認は 1 つで、動作中の一覧は最新に置き換わる", () => {
    const ui = createUiStore();
    const seen: unknown[] = [];
    ui.subscribe((s) => seen.push(s.quitConfirmation));

    ui.getState().openQuitConfirmation([running("t1")]);
    ui.getState().openQuitConfirmation([running("t1"), running("t2", "ターミナル 2")]);

    expect(ui.getState().quitConfirmation).toEqual({ running: [running("t1"), running("t2", "ターミナル 2")], unsaved: [] });
    ui.getState().closeQuitConfirmation();
    expect(ui.getState().quitConfirmation).toBeNull();
    expect(seen).toHaveLength(3);
  });

  it("削除の確認はプロジェクトと動作中の一覧を持ち、閉じると消える", () => {
    const ui = createUiStore();

    ui.getState().openRemoveConfirmation("p1", [running("t1")]);
    expect(ui.getState().removeConfirmation).toEqual({ projectId: "p1", running: [running("t1")], unsaved: [] });

    ui.getState().openRemoveConfirmation("p2", []);
    expect(ui.getState().removeConfirmation).toEqual({ projectId: "p2", running: [], unsaved: [] });

    ui.getState().closeRemoveConfirmation();
    expect(ui.getState().removeConfirmation).toBeNull();
  });

  it("カードのメニューはプロジェクトと位置を持ち、閉じると消える", () => {
    const ui = createUiStore();

    ui.getState().openCardMenu("p1", { x: 10, y: 20 });
    expect(ui.getState().cardMenu).toEqual({ projectId: "p1", x: 10, y: 20 });

    ui.getState().closeCardMenu();
    expect(ui.getState().cardMenu).toBeNull();
  });

  it("閉じた知らせの文を覚える(最初は無い)", () => {
    const ui = createUiStore();
    expect(ui.getState().dismissedNotice).toBeNull();

    ui.getState().dismissNotice("状態ファイルを退避しました");

    expect(ui.getState().dismissedNotice).toBe("状態ファイルを退避しました");
  });

  it("生成関数ごとに別のストアになる", () => {
    const a = createUiStore();
    const b = createUiStore();

    a.getState().openQuitConfirmation([running("t1")]);

    expect(b.getState().quitConfirmation).toBeNull();
  });
});

describe("uiStore(ツリーの表示だけの状態。M3)", () => {
  it("作った直後はツリーのメニュー_入力_確認_知らせが無く_フォーカスもドラッグも無く_表は既定の見本", () => {
    const ui = createUiStore().getState();

    expect(ui.treeMenu).toBeNull();
    expect(ui.inlineEdit).toBeNull();
    expect(ui.treeConflict).toBeNull();
    expect(ui.moveConfirmation).toBeNull();
    expect(ui.treeNotice).toBeNull();
    expect(ui.treeDragging).toBe(false);
    expect(ui.treeFocused).toBe(false);
    expect(ui.shortcuts).toBe(DEFAULT_SHORTCUT_TABLE);
  });

  it("ツリーのメニューは場所と位置を持ち_閉じると消える", () => {
    const ui = createUiStore();

    ui.getState().openTreeMenu("p1", { kind: "file", path: "src/a.ts" }, { x: 5, y: 6 });
    expect(ui.getState().treeMenu).toEqual({ projectId: "p1", target: { kind: "file", path: "src/a.ts" }, x: 5, y: 6 });

    ui.getState().closeTreeMenu();
    expect(ui.getState().treeMenu).toBeNull();
  });

  it("インライン編集は新規作成と名称変更を持ち_閉じると消える", () => {
    const ui = createUiStore();

    ui.getState().startInlineEdit({ mode: "create", projectId: "p1", parent: "src", kind: "file" });
    expect(ui.getState().inlineEdit).toEqual({ mode: "create", projectId: "p1", parent: "src", kind: "file" });
    ui.getState().startInlineEdit({ mode: "rename", projectId: "p1", path: "src/a.ts" });
    expect(ui.getState().inlineEdit).toEqual({ mode: "rename", projectId: "p1", path: "src/a.ts" });

    ui.getState().closeInlineEdit();
    expect(ui.getState().inlineEdit).toBeNull();
  });

  it("衝突の確認と移動の確認は開いて閉じられる", () => {
    const ui = createUiStore();
    const request = { op: "move", projectId: "p1", source: "a.md", destDir: "docs" } as const;

    ui.getState().openTreeConflict({ request, name: "a.md", total: 1 });
    ui.getState().openMoveConfirmation({ projectId: "p1", source: "a.md", destDir: "docs" });
    expect(ui.getState().treeConflict).toEqual({ request, name: "a.md", total: 1 });
    expect(ui.getState().moveConfirmation).toEqual({ projectId: "p1", source: "a.md", destDir: "docs" });

    ui.getState().closeTreeConflict();
    ui.getState().closeMoveConfirmation();
    expect(ui.getState().treeConflict).toBeNull();
    expect(ui.getState().moveConfirmation).toBeNull();
  });

  it("ツリーの知らせは出すたびに番号が変わり_閉じると消える", () => {
    const ui = createUiStore();

    ui.getState().showTreeNotice("「a.ts」が見つかりません");
    const first = ui.getState().treeNotice;
    ui.getState().showTreeNotice("「a.ts」が見つかりません");
    const second = ui.getState().treeNotice;

    expect(first?.message).toBe("「a.ts」が見つかりません");
    expect(second?.message).toBe("「a.ts」が見つかりません");
    expect(second?.id).not.toBe(first?.id);
    ui.getState().dismissTreeNotice();
    expect(ui.getState().treeNotice).toBeNull();
  });

  it("設定の知らせは状態の知らせと別に持ち別に閉じられる", () => {
    const ui = createUiStore();
    expect(ui.getState().settingsNotice).toBeNull();
    expect(ui.getState().settingsNoticeDismissed).toBe(false);

    ui.getState().setSettingsNotice("ショートカットの設定の 1 行を使わず、既定のキーのままにしました: x");
    // 状態の知らせを閉じても、設定の知らせは残る
    ui.getState().dismissNotice("状態ファイルを退避しました");
    expect(ui.getState().dismissedNotice).toBe("状態ファイルを退避しました");
    expect(ui.getState().settingsNotice).toBe("ショートカットの設定の 1 行を使わず、既定のキーのままにしました: x");
    expect(ui.getState().settingsNoticeDismissed).toBe(false);

    // 設定の知らせを閉じても、状態の知らせの閉じた印は変わらない
    ui.getState().dismissSettingsNotice();
    expect(ui.getState().settingsNoticeDismissed).toBe(true);
    expect(ui.getState().dismissedNotice).toBe("状態ファイルを退避しました");

    // 同じ知らせが届き直しても(表だけが変わったなど)閉じたまま
    ui.getState().setSettingsNotice("ショートカットの設定の 1 行を使わず、既定のキーのままにしました: x");
    expect(ui.getState().settingsNoticeDismissed).toBe(true);
    // 違う知らせが届いたら出し直す
    ui.getState().setSettingsNotice("設定ファイルの 3 行 5 列目を読めないため、直前の設定のままです");
    expect(ui.getState().settingsNoticeDismissed).toBe(false);
    // 知らせが無くなれば消え、次に同じ文が届いても出す
    ui.getState().dismissSettingsNotice();
    ui.getState().setSettingsNotice(null);
    expect([ui.getState().settingsNotice, ui.getState().settingsNoticeDismissed]).toEqual([null, false]);
    ui.getState().setSettingsNotice("設定ファイルの 3 行 5 列目を読めないため、直前の設定のままです");
    expect(ui.getState().settingsNoticeDismissed).toBe(false);
  });

  it("ツリーのフォーカスとドラッグ中とショートカットの表を切り替えられる", () => {
    const ui = createUiStore();
    const table = DEFAULT_SHORTCUT_TABLE.slice(0, 2);

    ui.getState().setTreeFocused(true);
    ui.getState().setTreeDragging(true);
    ui.getState().setShortcuts(table);

    expect(ui.getState().treeFocused).toBe(true);
    expect(ui.getState().treeDragging).toBe(true);
    expect(ui.getState().shortcuts).toBe(table);
  });

  it("ツリーの片付けは_ほかのプロジェクトのメニュー_入力_確認を閉じ_ドラッグとフォーカスの印を戻す", () => {
    const ui = createUiStore();
    const s = ui.getState();
    s.openTreeMenu("p1", { kind: "blank" }, { x: 0, y: 0 });
    s.startInlineEdit({ mode: "create", projectId: "p2", parent: "", kind: "file" });
    s.openTreeConflict({ request: { op: "paste", projectId: "p1", destDir: "" }, name: "a.ts", total: 1 });
    s.openMoveConfirmation({ projectId: "p2", source: "a", destDir: "b" });
    s.setTreeDragging(true);
    s.setTreeFocused(true);

    ui.getState().resetTreeUi("p2");

    const after = ui.getState();
    expect(after.treeMenu).toBeNull();
    expect(after.treeConflict).toBeNull();
    expect(after.inlineEdit).toEqual({ mode: "create", projectId: "p2", parent: "", kind: "file" });
    expect(after.moveConfirmation).toEqual({ projectId: "p2", source: "a", destDir: "b" });
    expect([after.treeDragging, after.treeFocused]).toEqual([false, false]);

    ui.getState().resetTreeUi(null);
    expect([ui.getState().inlineEdit, ui.getState().moveConfirmation]).toEqual([null, null]);
  });

  it("ツリーの片付けは_ほかのプロジェクトの知らせを閉じ_持ち主の無い知らせは残す", () => {
    const ui = createUiStore();
    ui.getState().showTreeNotice("「a.ts」が見つかりません", "p1");
    expect(ui.getState().treeNotice).toMatchObject({ message: "「a.ts」が見つかりません", projectId: "p1" });

    ui.getState().resetTreeUi("p1");
    expect(ui.getState().treeNotice?.message).toBe("「a.ts」が見つかりません");
    ui.getState().resetTreeUi("p2");
    expect(ui.getState().treeNotice).toBeNull();

    ui.getState().showTreeNotice("フォルダを読めませんでした");
    ui.getState().resetTreeUi("p2");
    expect(ui.getState().treeNotice).toMatchObject({ message: "フォルダを読めませんでした", projectId: null });
  });

  it("ツリーの片付けで消すものが無ければ状態を変えない", () => {
    const ui = createUiStore();
    const before = ui.getState();

    ui.getState().resetTreeUi("p1");

    expect(ui.getState()).toBe(before);
  });
  describe("設定画面(M4.6。ADR-013 の 5)", () => {
    it("作った直後は設定画面が閉じている", () => {
      expect(createUiStore().getState().settingsOpen).toBe(false);
    });

    it("設定を開くのメニューの操作で設定画面が開く", () => {
      const ui = createUiStore();

      ui.getState().openSettings();
      expect(ui.getState().settingsOpen).toBe(true);

      ui.getState().closeSettings();
      expect(ui.getState().settingsOpen).toBe(false);
    });

    it("開いているときにもう一度届いても_1_つのまま", () => {
      const ui = createUiStore();
      const seen: boolean[] = [];
      ui.subscribe((s) => seen.push(s.settingsOpen));

      ui.getState().openSettings();
      ui.getState().openSettings();

      expect(ui.getState().settingsOpen).toBe(true);
      // 2 回目は状態を変えない(部品が開き直さない)
      expect(seen).toEqual([true]);
    });

    it("ほかの確認ダイアログが開いていれば開かない", () => {
      const opens: Array<(ui: ReturnType<typeof createUiStore>) => void> = [
        (ui) => ui.getState().openQuitConfirmation([running("t1")]),
        (ui) => ui.getState().openRemoveConfirmation("p1", [running("t1")]),
        (ui) => ui.getState().openTreeConflict({ request: { op: "paste", projectId: "p1", destDir: "" }, name: "a", total: 1 }),
        (ui) => ui.getState().openMoveConfirmation({ projectId: "p1", source: "a", destDir: "b" }),
      ];
      for (const open of opens) {
        const ui = createUiStore();
        open(ui);

        ui.getState().openSettings();

        expect(ui.getState().settingsOpen).toBe(false);
      }
    });

    it("開くと右クリックのメニューを閉じる", () => {
      const ui = createUiStore();
      ui.getState().openCardMenu("p1", { x: 1, y: 2 });
      ui.getState().openTreeMenu("p1", { kind: "blank" }, { x: 1, y: 2 });

      ui.getState().openSettings();

      expect([ui.getState().settingsOpen, ui.getState().cardMenu, ui.getState().treeMenu]).toEqual([true, null, null]);
    });

    it("設定画面を開いているあいだに終了の確認が届いたら上に重ねて開く", () => {
      const ui = createUiStore();
      ui.getState().openSettings();

      ui.getState().openQuitConfirmation([running("t1")]);

      expect(ui.getState().settingsOpen).toBe(true);
      expect(ui.getState().quitConfirmation).not.toBeNull();
    });
  });
});

describe("uiStore(エディタ。M4.8。ADR-015 の 4・5・8)", () => {
  const unsaved = { editorId: "editor-1", projectName: "app", path: "docs/a.md" };

  it("終了と削除の確認に未保存のエディタの一覧を載せる", () => {
    const ui = createUiStore();

    ui.getState().openQuitConfirmation([], [unsaved]);
    ui.getState().openRemoveConfirmation("p1", [running("t1")], [unsaved]);

    expect(ui.getState().quitConfirmation).toEqual({ running: [], unsaved: [unsaved] });
    expect(ui.getState().removeConfirmation).toEqual({ projectId: "p1", running: [running("t1")], unsaved: [unsaved] });
  });

  it("表示の切替はタブごとに持ち_既定は並べて", () => {
    const ui = createUiStore();

    expect(editorViewMode(ui.getState(), "editor-1")).toBe("split");
    ui.getState().setEditorView("editor-1", "preview");
    ui.getState().setEditorView("editor-2", "edit");

    expect(editorViewMode(ui.getState(), "editor-1")).toBe("preview");
    expect(editorViewMode(ui.getState(), "editor-2")).toBe("edit");
    expect(editorViewMode(ui.getState(), "editor-3")).toBe("split");
  });

  it("帯は出し直すと置き換わり_null_で消える", () => {
    const ui = createUiStore();

    ui.getState().setEditorBanner("editor-1", { kind: "changed" });
    ui.getState().setEditorBanner("editor-1", { kind: "failed", message: "保存できませんでした" });
    expect(ui.getState().editorBanners.get("editor-1")).toEqual({ kind: "failed", message: "保存できませんでした" });

    const before = ui.getState();
    ui.getState().setEditorBanner("editor-2", null);
    // 無いものを消しても状態は変わらない(描き直さない)
    expect(ui.getState()).toBe(before);
    ui.getState().setEditorBanner("editor-1", null);
    expect(ui.getState().editorBanners.has("editor-1")).toBe(false);
  });

  it("閉じる確認と保存の衝突は開閉でき_開いているあいだは設定画面を開かない", () => {
    const ui = createUiStore();

    ui.getState().openUnsavedClose("editor-1");
    expect(ui.getState().unsavedClose).toEqual({ editorId: "editor-1" });
    ui.getState().openSettings();
    expect(ui.getState().settingsOpen).toBe(false);
    ui.getState().closeUnsavedClose();

    ui.getState().openSaveConflict({ editorId: "editor-1", kind: "missing" });
    expect(ui.getState().saveConflict).toEqual({ editorId: "editor-1", kind: "missing" });
    ui.getState().openSettings();
    expect(ui.getState().settingsOpen).toBe(false);
    ui.getState().closeSaveConflict();
    expect(ui.getState().saveConflict).toBeNull();
  });

  it("フォーカスの求めは同じ_id_で受け取ると消え_ほかの_id_では消えない", () => {
    const ui = createUiStore();

    ui.getState().requestEditorFocus("editor-1");
    ui.getState().consumeEditorFocus("editor-2");
    expect(ui.getState().pendingEditorFocus).toBe("editor-1");
    ui.getState().consumeEditorFocus("editor-1");
    expect(ui.getState().pendingEditorFocus).toBeNull();
  });

  it("写しから消えたエディタの切替_帯_プレビュー_確認_フォーカスの求めを片付ける", () => {
    const ui = createUiStore();
    ui.getState().setEditorView("editor-1", "edit");
    ui.getState().setEditorView("editor-2", "edit");
    ui.getState().setEditorBanner("editor-1", { kind: "missing" });
    ui.getState().setEditorPreview("editor-1", []);
    ui.getState().openUnsavedClose("editor-1");
    ui.getState().openSaveConflict({ editorId: "editor-1", kind: "changed" });
    ui.getState().requestEditorFocus("editor-1");

    ui.getState().retainEditors(new Set(["editor-2"]));

    const s = ui.getState();
    expect([...s.editorViews.keys()]).toEqual(["editor-2"]);
    expect(s.editorBanners.size).toBe(0);
    expect(s.editorPreviews.size).toBe(0);
    expect(s.unsavedClose).toBeNull();
    expect(s.saveConflict).toBeNull();
    expect(s.pendingEditorFocus).toBeNull();

    // 何も消えなければ状態は変わらない
    const before = ui.getState();
    ui.getState().retainEditors(new Set(["editor-2"]));
    expect(ui.getState()).toBe(before);
  });
});
