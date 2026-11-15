import { type CSSProperties, type MouseEvent, memo } from "react";

import type { TreeRow as TreeRowValue } from "../../logic/tree/treeModel";
import { ChevronIcon, FileIcon, FolderIcon, LinkBadgeIcon } from "./icons";

export interface TreeRowProps {
  row: TreeRowValue;
  /** 要素の ID(ツリーの aria-activedescendant が指す)。 */
  id: string;
  selected: boolean;
  /** クリック(選択)。 */
  onSelect: (path: string) => void;
  /** 開閉の印のクリック(開閉だけ。選択は変えない)。 */
  onToggle: (path: string) => void;
  /** ダブルクリック(フォルダは開閉、それ以外は既定のアプリで開く。【仮定】6)。 */
  onActivate: (row: TreeRowValue) => void;
}

/** フォルダとして描く種類(フォルダへのリンクもフォルダのアイコン。開閉はしない)。 */
const isFolderKind = (kind: TreeRowValue["kind"]) => kind === "directory" || kind === "directoryLink";
const isLinkKind = (kind: TreeRowValue["kind"]) => kind === "directoryLink" || kind === "fileLink";

/**
 * ツリーの 1 行(Presentational。設計書 §9.1)。字下げ・開閉の印・アイコン・名前を出す。名前は文字として差し込む
 * (HTML として解釈させない。設計書 §12)。高さは --tree-row-height で固定(仮想スクロールの窓の計算と同じ値)。
 * 同じ props なら描き直さない(memo)。スクロールで窓が動いても、窓に残った行は描き直さない。
 * 選択の塗りの色(フォーカスの有無)は、ツリーの data-focused を CSS が見て決める(行の props にしない)。
 * T43c・T43d はこの行に右クリック・pointer のイベントを足す(data-path と data-kind で対象を引ける)。
 */
export const TreeRow = memo(function TreeRow({ row, id, selected, onSelect, onToggle, onActivate }: TreeRowProps) {
  const onToggleClick = (event: MouseEvent) => {
    event.stopPropagation();
    onToggle(row.path);
  };
  const onDoubleClick = (event: MouseEvent) => {
    // 開閉の印の上のダブルクリックは、2 回のクリックで開閉済み。
    if ((event.target as Element).closest(".tree-row__chevron") !== null) return;
    onActivate(row);
  };
  const classes = ["tree-row"];
  if (selected) classes.push("tree-row--selected");

  return (
    <div
      id={id}
      role="treeitem"
      className={classes.join(" ")}
      data-testid="tree-row"
      data-path={row.path}
      data-kind={row.kind}
      aria-level={row.depth + 2}
      aria-selected={selected}
      aria-expanded={row.expandable ? row.expanded : undefined}
      aria-busy={row.loading ? true : undefined}
      title={row.name}
      style={{ "--tree-depth": row.depth } as CSSProperties}
      onClick={() => onSelect(row.path)}
      onDoubleClick={onDoubleClick}
    >
      <span className="tree-row__inner">
        <span className="tree-row__chevron-slot">
          {row.expandable && (
            <span
              className={row.expanded ? "tree-row__chevron tree-row__chevron--open" : "tree-row__chevron"}
              data-testid="tree-row-toggle"
              onClick={onToggleClick}
            >
              <ChevronIcon />
            </span>
          )}
        </span>
        <span className={isFolderKind(row.kind) ? "tree-icon tree-icon--folder" : "tree-icon tree-icon--file"}>
          {isFolderKind(row.kind) ? <FolderIcon /> : <FileIcon />}
          {isLinkKind(row.kind) && <LinkBadgeIcon className="tree-icon__badge" />}
        </span>
        <span className="tree-row__name">{row.name}</span>
        {row.loading && <span className="tree-spinner" data-testid="tree-row-spinner" aria-hidden="true" />}
      </span>
    </div>
  );
});
