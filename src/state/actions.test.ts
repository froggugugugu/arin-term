import { describe, expect, it } from "vitest";

import { createIpc } from "../ipc/client";
import type { RunningTerminal } from "../ipc/generated/RunningTerminal";
import { createFakeIpc, type FakeHandlers } from "../test/fakeIpc";
import { createActions } from "./actions";
import { createUiStore } from "./uiStore";

function setup(handlers: Partial<FakeHandlers> = {}) {
  const fake = createFakeIpc(handlers);
  const ui = createUiStore();
  const errors: string[] = [];
  const actions = createActions({ ipc: createIpc(fake.transport), ui, reportError: (m) => errors.push(m) });
  return { fake, ui, errors, actions };
}

const running: RunningTerminal = { terminalId: "t1", projectId: "p1", projectName: "app", label: "ターミナル 1" };

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

describe("actions(プロジェクトの削除の確認。FR-003)", () => {
  it("動作中が無ければ確認せずに削除する", async () => {
    const { fake, ui, actions } = setup();

    await actions.removeProject("p1");

    expect(fake.calls).toEqual([{ command: "remove_project", args: { projectId: "p1", confirmed: false } }]);
    expect(ui.getState().removeConfirmation).toBeNull();
  });

  it("動作中があれば確認を開き、確認すると確認済みで呼び直して閉じる", async () => {
    const { fake, ui, actions } = setup({
      remove_project: ({ confirmed }) => (confirmed ? { kind: "removed" } : { kind: "needsConfirmation", running: [running] }),
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
      remove_project: () => ({ kind: "needsConfirmation", running: [running] }),
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
      remove_project: ({ confirmed }) => ({ kind: "needsConfirmation", running: confirmed ? [other] : [running] }),
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
