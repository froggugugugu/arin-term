import type { RefObject } from "react";
import { createPortal } from "react-dom";

import type { Point } from "../../logic/dnd/dragMachine";
import { FileIcon, FolderIcon } from "./icons";
import type { TreeDragView } from "./useTreeDrag";

interface TreeDragGhostProps {
  view: TreeDragView;
  ghostRef: RefObject<HTMLDivElement | null>;
  /** 写しの左上(useTreeDrag が動かす。描き直しでもこの位置で描く)。 */
  positionRef: RefObject<Point>;
}

/**
 * ドラッグしている項目の写し(Finder のドラッグにならう)。カーソルに付いて動く、アイコンと名前の小さな半透明の札。
 * ⌥ を押していて離すとコピーになるときは、左上に小さな「+」の印を付ける(Finder と同じ発想)。
 * カーソルの下の要素の判定(elementFromPoint)を邪魔しないよう pointer-events を持たない。ペインの overflow で
 * 切られないよう body の直下に置く。飾りなので読み上げない。
 */
export function TreeDragGhost({ view, ghostRef, positionRef }: TreeDragGhostProps) {
  const { x, y } = positionRef.current;
  return createPortal(
    <div
      ref={ghostRef}
      className="tree-drag-ghost"
      data-testid="tree-drag-ghost"
      data-copy={view.copy ? "true" : undefined}
      aria-hidden="true"
      style={{ transform: `translate3d(${x}px, ${y}px, 0)` }}
    >
      <span className={view.item.folder ? "tree-icon tree-icon--folder" : "tree-icon tree-icon--file"}>
        {view.item.folder ? <FolderIcon /> : <FileIcon />}
      </span>
      <span className="tree-drag-ghost__name">{view.item.name}</span>
      {view.copy && (
        <span className="tree-drag-ghost__badge" data-testid="tree-drag-copy-badge">
          <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true" focusable="false">
            <path d="M5 2v6M2 5h6" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
          </svg>
        </span>
      )}
    </div>,
    document.body,
  );
}
