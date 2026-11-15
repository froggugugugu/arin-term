// メニューバーから画面へ届く操作(`menu-command` イベント)を、フォーカス位置で振り分ける(設計書 §9.1)。
// DOM を使わない純粋な関数。M2 で届くのは「すべてを選択」だけ(計画 T33e)。

import type { FocusArea } from "./focus";

/** メニューから届く操作の種類。 */
export type MenuCommand = "select-all";

/** 操作が届いたときのフォーカス位置。コピー・ペーストのフォーカス位置(`ClipboardFocus`)もこの形を満たす。 */
export interface MenuCommandFocus {
  readonly kind: FocusArea;
}

/**
 * 振り分けた先。
 * - `select-all-terminal`: フォーカスのあるターミナルを全選択する
 * - `none`: 何もしない(画面の既定の動作に任せる)
 */
export type MenuCommandAction = "select-all-terminal" | "none";

/** メニューから届いた操作を、フォーカス位置で振り分ける。 */
export function routeMenuCommand(command: MenuCommand, focus: MenuCommandFocus): MenuCommandAction {
  switch (command) {
    case "select-all":
      return focus.kind === "terminal" ? "select-all-terminal" : "none";
  }
}
