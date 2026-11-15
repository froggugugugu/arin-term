// 画面内のメニュー(カードの右クリックなど)のキーボード操作(設計書 §9.1、WAI-ARIA の menu の型)。DOM を使わない純粋な関数。
// 画面(src/ui/menu)は、選べる項目(区切りを除く)の並びの番号でフォーカスを持ち、ここの結果に従って動かす。

/**
 * 矢印キー・Home・End で、次にフォーカスする項目の番号を返す。上下の矢印は端で反対側に回り込む。
 * `current` が -1(どこにもフォーカスが無い)なら、下の矢印は最初、上の矢印は最後。移動のキーでない・項目が 0 個なら null。
 */
export function moveMenuFocus(current: number, count: number, key: string): number | null {
  if (count <= 0) return null;
  switch (key) {
    case "ArrowDown":
      return current < 0 ? 0 : (current + 1) % count;
    case "ArrowUp":
      return current < 0 ? count - 1 : (current - 1 + count) % count;
    case "Home":
      return 0;
    case "End":
      return count - 1;
    default:
      return null;
  }
}

/** 移動以外のキーの意味。 */
export type MenuKeyAction = "activate" | "open-submenu" | "close-submenu" | "close" | "none";

/** 移動以外のキーの意味を決める。`inSubmenu` は、フォーカスが下の階層(色 ▸ など)の中にあるか。 */
export function menuKeyAction(key: string, { inSubmenu }: { readonly inSubmenu: boolean }): MenuKeyAction {
  switch (key) {
    case "Enter":
    case " ":
      return "activate";
    case "ArrowRight":
      return inSubmenu ? "none" : "open-submenu";
    case "ArrowLeft":
      return inSubmenu ? "close-submenu" : "none";
    case "Escape":
      return inSubmenu ? "close-submenu" : "close";
    case "Tab":
      return "close";
    default:
      return "none";
  }
}
