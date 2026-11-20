// ファイルツリーの矢印キーの動き(WAI-ARIA の tree の作法)。DOM を使わない純粋な関数。
//
// - ↓ / ↑: 見えている行の次・前を選ぶ。端では止まる。何も選んでいなければ ↓ は先頭、↑ は末尾
// - →: 閉じたフォルダは開く。開いているフォルダは最初の子へ移る
// - ←: 開いているフォルダは閉じる。それ以外は親のフォルダへ移る(親は開いたまま。本人が 2026-11-20 に選んだ)

import type { TreeRow } from "./treeModel";
import { parentPath } from "./treePath";

/** 受ける矢印キー(`KeyboardEvent.key` の値)。 */
export type TreeArrowKey = "ArrowUp" | "ArrowDown" | "ArrowLeft" | "ArrowRight";

/** 矢印キーで行う操作。 */
export type TreeArrowAction =
  | { readonly type: "select"; readonly path: string }
  | { readonly type: "expand"; readonly path: string }
  | { readonly type: "collapse"; readonly path: string };

/** 矢印キーか。 */
export function isTreeArrowKey(key: string): key is TreeArrowKey {
  return key === "ArrowUp" || key === "ArrowDown" || key === "ArrowLeft" || key === "ArrowRight";
}

/**
 * 矢印キーの操作を決める。何もしないときは null。
 * `rows` は見えている行(上から順)、`selected` は選択中のパス(ルートの見出しは空の文字列、選択なしは null)。
 */
export function treeArrowAction(
  rows: readonly TreeRow[],
  selected: string | null,
  key: TreeArrowKey,
): TreeArrowAction | null {
  const index = selected === null || selected === "" ? -1 : rows.findIndex((row) => row.path === selected);
  const row = index < 0 ? undefined : rows[index];
  const select = (target: TreeRow | undefined): TreeArrowAction | null =>
    target === undefined ? null : { type: "select", path: target.path };

  switch (key) {
    case "ArrowDown":
      // 選択なし・ルートの見出し・見えなくなった行からは、先頭へ。
      return select(rows[index + 1]);
    case "ArrowUp":
      if (selected === "") return null;
      return select(row === undefined ? rows[rows.length - 1] : rows[index - 1]);
    case "ArrowRight": {
      if (row === undefined || !row.expandable) return null;
      if (!row.expanded) return { type: "expand", path: row.path };
      // 開いていれば最初の子へ(すぐ下の行が 1 段深ければ子。空・読み込み中なら動かない)。
      const next = rows[index + 1];
      return next !== undefined && next.depth > row.depth ? select(next) : null;
    }
    case "ArrowLeft": {
      if (row === undefined) return null;
      if (row.expandable && row.expanded) return { type: "collapse", path: row.path };
      const parent = parentPath(row.path);
      // いちばん上の階層(親がルート)では動かない。
      return parent === null || parent === "" ? null : { type: "select", path: parent };
    }
  }
}
