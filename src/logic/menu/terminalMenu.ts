// ターミナルの右クリックのメニュー(ADR-020 の 3)。DOM を使わない純粋な関数。

/** 選ばれたときの操作。 */
export type TerminalMenuCommand = "copy" | "paste" | "select-all";

/** メニューの 1 行(src/ui/menu/ContextMenu.tsx の ContextMenuEntry と同じ形)。 */
export type TerminalMenuEntry =
  | {
      readonly kind: "command";
      readonly label: string;
      readonly command: TerminalMenuCommand;
      readonly enabled: boolean;
    }
  | { readonly kind: "separator" };

/** メニューを組み立てる。選択が無ければ「コピー」を選べなくする(macOS と同じく出したまま薄くする)。 */
export function buildTerminalMenu(hasSelection: boolean): readonly TerminalMenuEntry[] {
  return [
    { kind: "command", label: "コピー", command: "copy", enabled: hasSelection },
    { kind: "command", label: "ペースト", command: "paste", enabled: true },
    { kind: "separator" },
    { kind: "command", label: "すべてを選択", command: "select-all", enabled: true },
  ];
}
