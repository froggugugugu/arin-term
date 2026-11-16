// メニューバーから画面へ届く操作(`menu-command` イベント)を、フォーカス位置で振り分ける(設計書 §9.1)。
// DOM を使わない純粋な関数。M2 で「すべてを選択」(計画 T33e)、M4.6 で「設定…」(ADR-013 の 6。⌘, もここに届く)。

import type { FocusArea } from "./focus";

/** メニューから届く操作の種類(生成した `src/ipc/generated/MenuCommand.ts` と同じ文字列)。 */
export type MenuCommand = "select-all" | "open-settings";

/** 操作が届いたときのフォーカス位置。コピー・ペーストのフォーカス位置(`ClipboardFocus`)もこの形を満たす。 */
export interface MenuCommandFocus {
  /** `editor` はエディタ(M4.8)。キーの振り分け(FocusArea)には入れず、メニューの振り分けだけが見る。 */
  readonly kind: FocusArea | "editor";
}

/**
 * 振り分けた先。
 * - `select-all-terminal`: フォーカスのあるターミナルを全選択する
 * - `select-all-editor`: フォーカスのあるエディタの文書の全体を選ぶ(M4.8。ADR-015 の 6)
 * - `open-settings`: 設定画面を開く(フォーカス位置によらない。ターミナルには渡さない)
 * - `none`: 何もしない(画面の既定の動作に任せる)
 */
export type MenuCommandAction = "select-all-terminal" | "select-all-editor" | "open-settings" | "none";

/** メニューから届いた操作を、フォーカス位置で振り分ける。 */
export function routeMenuCommand(command: MenuCommand, focus: MenuCommandFocus): MenuCommandAction {
  switch (command) {
    case "select-all":
      if (focus.kind === "terminal") return "select-all-terminal";
      return focus.kind === "editor" ? "select-all-editor" : "none";
    case "open-settings":
      return "open-settings";
  }
}
