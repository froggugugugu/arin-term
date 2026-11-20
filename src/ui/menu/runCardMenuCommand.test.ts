import { describe, expect, it } from "vitest";

import { type CardMenuActions, runCardMenuCommand } from "./runCardMenuCommand";

function fakeActions() {
  const calls: unknown[][] = [];
  const actions: CardMenuActions = {
    newTerminal: async (id) => void calls.push(["newTerminal", id]),
    copyProjectPath: async (id) => void calls.push(["copyProjectPath", id]),
    setProjectColor: async (id, color) => void calls.push(["setProjectColor", id, color]),
    relinkProject: async (id) => void calls.push(["relinkProject", id]),
    removeProject: async (id) => void calls.push(["removeProject", id]),
  };
  return { actions, calls };
}

describe("runCardMenuCommand(カードのメニューで選ばれた項目を操作に渡す)", () => {
  it("新規ターミナルは、メニューを開いたカードのプロジェクトを渡して開く", async () => {
    const { actions, calls } = fakeActions();
    await runCardMenuCommand(actions, "p2", { type: "new-terminal" });
    expect(calls).toEqual([["newTerminal", "p2"]]);
  });

  it("絶対パスをコピーはパスのコピーを呼ぶ(選択の操作は呼ばない)", async () => {
    const { actions, calls } = fakeActions();
    await runCardMenuCommand(actions, "p1", { type: "copy-path" });
    expect(calls).toEqual([["copyProjectPath", "p1"]]);
  });

  it("色を選ぶと色の変更を呼ぶ", async () => {
    const { actions, calls } = fakeActions();
    await runCardMenuCommand(actions, "p2", { type: "set-color", color: "teal" });
    expect(calls).toEqual([["setProjectColor", "p2", "teal"]]);
  });

  it("再リンクと一覧から削除はそれぞれの操作を呼ぶ", async () => {
    const { actions, calls } = fakeActions();
    await runCardMenuCommand(actions, "p3", { type: "relink" });
    await runCardMenuCommand(actions, "p3", { type: "remove" });
    expect(calls).toEqual([
      ["relinkProject", "p3"],
      ["removeProject", "p3"],
    ]);
  });
});
