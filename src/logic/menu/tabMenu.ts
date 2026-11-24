// タブの右クリックのメニュー(タブ表示のタブバー。ADR-019)。DOM を使わない純粋な関数。
//
// 項目は「タブを閉じる」「ほかのタブを閉じる」「これより右を閉じる」の 3 つ。「右」はタブバーに見えている並び
// (ターミナルのタブ → エディタのタブ)での右。閉じるタブの組み立てもここで行い、画面は close_tabs にそのまま渡す。

/** タブバーのタブ 1 枚(種類と ID)。 */
export interface TabRef {
  readonly kind: "terminal" | "editor";
  readonly id: string;
}

/** 選ばれたときの操作。 */
export type TabMenuCommand = "close" | "close-others" | "close-right";

/** メニューの 1 行(src/ui/menu/ContextMenu.tsx の ContextMenuEntry と同じ形)。 */
export interface TabMenuEntry {
  readonly kind: "command";
  readonly label: string;
  readonly command: TabMenuCommand;
  readonly enabled: boolean;
}

/** 閉じるタブ(種類ごと。タブバーの順)。 */
export interface TabsToClose {
  readonly terminalIds: readonly string[];
  readonly editorIds: readonly string[];
}

const sameTab = (a: TabRef, b: TabRef) => a.kind === b.kind && a.id === b.id;

/**
 * メニューを組み立てる。order はタブバーの並び(ターミナル → エディタ)、target は右クリックしたタブ。
 * ほかのタブが無ければ「ほかのタブを閉じる」、右にタブが無ければ「これより右を閉じる」を選べなくする(macOS と同じく出したまま薄くする)。
 */
export function buildTabMenu(order: readonly TabRef[], target: TabRef): readonly TabMenuEntry[] {
  const index = order.findIndex((tab) => sameTab(tab, target));
  return [
    { kind: "command", label: "タブを閉じる", command: "close", enabled: index >= 0 },
    { kind: "command", label: "ほかのタブを閉じる", command: "close-others", enabled: index >= 0 && order.length > 1 },
    { kind: "command", label: "これより右を閉じる", command: "close-right", enabled: index >= 0 && index < order.length - 1 },
  ];
}

/** 操作で閉じるタブ。target が並びに無ければ何も閉じない。 */
export function tabsToClose(order: readonly TabRef[], target: TabRef, command: TabMenuCommand): TabsToClose {
  const index = order.findIndex((tab) => sameTab(tab, target));
  if (index < 0) return { terminalIds: [], editorIds: [] };
  const closing =
    command === "close"
      ? [target]
      : command === "close-others"
        ? order.filter((_, i) => i !== index)
        : order.slice(index + 1);
  return {
    terminalIds: closing.filter((tab) => tab.kind === "terminal").map((tab) => tab.id),
    editorIds: closing.filter((tab) => tab.kind === "editor").map((tab) => tab.id),
  };
}
