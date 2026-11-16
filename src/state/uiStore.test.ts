import { describe, expect, it } from "vitest";

import type { RunningTerminal } from "../ipc/generated/RunningTerminal";
import { DEFAULT_SHORTCUT_TABLE } from "../logic/keys/defaultShortcuts";
import { createUiStore } from "./uiStore";

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
    expect(ui.getState().quitConfirmation).toEqual({ running: [running("t1")] });

    ui.getState().closeQuitConfirmation();
    expect(ui.getState().quitConfirmation).toBeNull();
  });

  it("終了の確認の要求が複数回届いても確認は 1 つで、動作中の一覧は最新に置き換わる", () => {
    const ui = createUiStore();
    const seen: unknown[] = [];
    ui.subscribe((s) => seen.push(s.quitConfirmation));

    ui.getState().openQuitConfirmation([running("t1")]);
    ui.getState().openQuitConfirmation([running("t1"), running("t2", "ターミナル 2")]);

    expect(ui.getState().quitConfirmation).toEqual({ running: [running("t1"), running("t2", "ターミナル 2")] });
    ui.getState().closeQuitConfirmation();
    expect(ui.getState().quitConfirmation).toBeNull();
    expect(seen).toHaveLength(3);
  });

  it("削除の確認はプロジェクトと動作中の一覧を持ち、閉じると消える", () => {
    const ui = createUiStore();

    ui.getState().openRemoveConfirmation("p1", [running("t1")]);
    expect(ui.getState().removeConfirmation).toEqual({ projectId: "p1", running: [running("t1")] });

    ui.getState().openRemoveConfirmation("p2", []);
    expect(ui.getState().removeConfirmation).toEqual({ projectId: "p2", running: [] });

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
