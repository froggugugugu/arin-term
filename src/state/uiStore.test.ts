import { describe, expect, it } from "vitest";

import type { RunningTerminal } from "../ipc/generated/RunningTerminal";
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

  it("生成関数ごとに別のストアになる", () => {
    const a = createUiStore();
    const b = createUiStore();

    a.getState().openQuitConfirmation([running("t1")]);

    expect(b.getState().quitConfirmation).toBeNull();
  });
});
