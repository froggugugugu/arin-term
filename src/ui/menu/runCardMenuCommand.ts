// カードの右クリックのメニューで選ばれた項目(src/logic/menu/cardMenu.ts の `command`)を、画面の操作に渡す。

import type { CardMenuCommand } from "../../logic/menu/cardMenu";
import type { Actions } from "../../state/actions";
import type { ProjectId } from "../snapshotTypes";

/** メニューが使う操作(Actions の一部)。 */
export type CardMenuActions = Pick<Actions, "newTerminal" | "copyProjectPath" | "setProjectColor" | "relinkProject" | "removeProject">;

/** 項目の操作を呼ぶ。対象はメニューを開いたカードのプロジェクト。どの操作も失敗しない(actions が報告する)。 */
export function runCardMenuCommand(actions: CardMenuActions, projectId: ProjectId, command: CardMenuCommand): Promise<void> {
  switch (command.type) {
    case "new-terminal":
      return actions.newTerminal(projectId);
    case "copy-path":
      return actions.copyProjectPath(projectId);
    case "set-color":
      return actions.setProjectColor(projectId, command.color);
    case "relink":
      return actions.relinkProject(projectId);
    case "remove":
      return actions.removeProject(projectId);
  }
}
