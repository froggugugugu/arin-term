import "./TreeView.css";

import {
  type FocusEvent,
  type MouseEvent,
  useCallback,
  useId,
  useLayoutEffect,
  useRef,
  useState,
} from "react";
import { useStore } from "zustand";

import type { TreeRow as TreeRowValue } from "../../logic/tree/treeModel";
import { computeRowWindow, revealRowOffset } from "../../logic/tree/virtualWindow";
import type { Actions } from "../../state/actions";
import { activeTreeModel, activeTreeRows } from "../../state/selectors";
import type { TreeState, TreeStore } from "../../state/treeStore";
import type { UiStore } from "../../state/uiStore";
import type { ProjectCard } from "../snapshotTypes";
import { TreeHeader } from "./TreeHeader";
import { TreeRow } from "./TreeRow";

interface FileTreeViewProps {
  /** ツリーを出すプロジェクト(選択中で「見つからない」でないもの)。切り替えたら key で作り直す(スクロールを戻すため)。 */
  project: ProjectCard;
  tree: TreeStore;
  ui: UiStore;
  actions: Pick<Actions, "selectTreePath" | "toggleDir" | "openEntry">;
}

/** 見える範囲の上下に余分に描く行の数。 */
const OVERSCAN = 6;
/** 行の高さを CSS から読めなかったときの値(tokens.css の --tree-row-height と同じ。【仮定】5)。 */
const FALLBACK_ROW_HEIGHT = 22;

const NO_ROWS: readonly TreeRowValue[] = [];

/** スクロールの位置と見える高さ。窓(描く行の範囲)が変わるときだけ更新する。 */
interface Viewport {
  readonly scrollTop: number;
  readonly height: number;
}

/** 要素の CSS の --tree-row-height(px)。読めなければ既定の値。 */
function readRowHeight(element: HTMLElement): number {
  const value = Number.parseFloat(getComputedStyle(element).getPropertyValue("--tree-row-height"));
  return Number.isFinite(value) && value > 0 ? value : FALLBACK_ROW_HEIGHT;
}

/**
 * ファイルツリー(Container。設計書 §9.1、ADR-007 の (c)、NFR-005)。treeStore の表示中のツリーを読み、
 * ルートの行(ヘッダ)と、T41a の窓(computeRowWindow)で決めた見える行だけを描く。上下は余白で埋め、
 * スクロールの高さは全部の行ぶんにする。スクロールで窓が変わらないあいだは描き直さない。
 *
 * - WAI-ARIA の tree。フォーカスはツリーの要素 1 つが持ち(Tab で入れる)、選んだ行を aria-activedescendant で指す
 *   (矢印キーでの移動は M3 では入れない。計画【仮定】6)
 * - フォーカスの出入りを uiStore.setTreeFocused で伝える(T43c のキーとコピー・ペーストの振り分けが使う)。
 *   選択の塗りの色(アクセント色かグレーか)は data-focused で CSS が決める
 * - 選択が変わったら、その行が見えるようにスクロールする(revealRowOffset。作った項目を選んだときなど)
 * - クリックで選択、開閉の印で開閉、ダブルクリックでフォルダは開閉・それ以外は既定のアプリで開く(計画【仮定】6)。
 *   行の無い余白のクリックで選択を外す
 *
 * T43c は、行・ヘッダ・余白の右クリックと、フォーカス中の keydown をここに足す。T43d は行の pointer のイベントを足す。
 */
export function FileTreeView({ project, tree, ui, actions }: FileTreeViewProps) {
  const projectId = project.id;
  const isMine = (s: TreeState) => s.activeProjectId === projectId;
  const rows = useStore(tree, (s) => (isMine(s) ? activeTreeRows(s) : NO_ROWS));
  const selected = useStore(tree, (s) => (isMine(s) ? (activeTreeModel(s)?.selected ?? null) : null));
  // ルートの中身がまだ届いていない(表示中のツリーに切り替わる前も含む)。
  const rootPending = useStore(tree, (s) => {
    const model = isMine(s) ? activeTreeModel(s) : null;
    return model === null || !model.listings.has("");
  });
  const focused = useStore(ui, (s) => s.treeFocused);

  const baseId = useId();
  const rowId = (index: number) => `${baseId}-row-${index}`;
  const rootId = `${baseId}-root`;

  const scrollerRef = useRef<HTMLDivElement>(null);
  const [rowHeight, setRowHeight] = useState(FALLBACK_ROW_HEIGHT);
  const [viewport, setViewport] = useState<Viewport>({ scrollTop: 0, height: 0 });
  // 描画のたびに最新の行を置く(スクロールの受け口と選択の追従が読む)。
  const rowsRef = useRef(rows);
  rowsRef.current = rows;

  /** スクロールの領域の今の値を測り、窓が変わるときだけ状態を更新する。 */
  const measure = useCallback(() => {
    const element = scrollerRef.current;
    if (element === null) return;
    const next: Viewport = { scrollTop: element.scrollTop, height: element.clientHeight };
    setViewport((prev) => {
      if (prev.height !== next.height) return next;
      const input = { rowHeight, rowCount: rowsRef.current.length, overscan: OVERSCAN };
      const a = computeRowWindow({ ...input, scrollTop: prev.scrollTop, viewportHeight: prev.height });
      const b = computeRowWindow({ ...input, scrollTop: next.scrollTop, viewportHeight: next.height });
      return a.start === b.start && a.end === b.end ? prev : next;
    });
  }, [rowHeight]);

  useLayoutEffect(() => {
    const element = scrollerRef.current;
    if (element === null) return;
    setRowHeight(readRowHeight(element));
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    return () => observer.disconnect();
  }, [measure]);

  // 選択が変わったら、その行が見えるようにする(行の数が変わっただけでは動かさない)。
  useLayoutEffect(() => {
    const element = scrollerRef.current;
    if (element === null || selected === null || selected === "") return;
    const index = rowsRef.current.findIndex((r) => r.path === selected);
    if (index < 0) return;
    const offset = revealRowOffset({
      scrollTop: element.scrollTop,
      viewportHeight: element.clientHeight,
      rowHeight,
      index,
    });
    if (offset !== 0) element.scrollTop += offset;
  }, [selected, rowHeight]);

  const onSelect = useCallback((path: string) => actions.selectTreePath(path), [actions]);
  const onToggle = useCallback((path: string) => actions.toggleDir(path), [actions]);
  const onActivate = useCallback(
    (row: TreeRowValue) => {
      if (row.kind === "directory") actions.toggleDir(row.path);
      // フォルダへのリンクは展開しない(設計書 §15.2 #8)。開くのコマンドに任せる【仮定】。
      else void actions.openEntry(projectId, row.path);
    },
    [actions, projectId],
  );
  const onSelectRoot = useCallback(() => actions.selectTreePath(""), [actions]);

  const onFocus = () => ui.getState().setTreeFocused(true);
  const onBlur = (event: FocusEvent<HTMLDivElement>) => {
    const next = event.relatedTarget;
    if (next instanceof Node && event.currentTarget.contains(next)) return;
    ui.getState().setTreeFocused(false);
  };
  // 行の無い余白のクリックで選択を外す。
  const onScrollerClick = (event: MouseEvent<HTMLDivElement>) => {
    if ((event.target as Element).closest('[role="treeitem"]') === null) actions.selectTreePath(null);
  };

  const win = computeRowWindow({
    scrollTop: viewport.scrollTop,
    viewportHeight: viewport.height,
    rowHeight,
    rowCount: rows.length,
    overscan: OVERSCAN,
  });
  const visible = rows.slice(win.start, win.end);
  const selectedIndex = selected === null || selected === "" ? -1 : rows.findIndex((r) => r.path === selected);
  const activeDescendant =
    selected === ""
      ? rootId
      : selectedIndex >= win.start && selectedIndex < win.end
        ? rowId(selectedIndex)
        : undefined;
  const empty = !rootPending && rows.length === 0;

  return (
    <div
      className="file-tree"
      role="tree"
      aria-label={`${project.name} のファイル`}
      aria-activedescendant={activeDescendant}
      aria-busy={rootPending ? true : undefined}
      tabIndex={0}
      data-testid="file-tree"
      data-focused={focused ? "true" : "false"}
      data-has-selection={selected !== null ? "true" : "false"}
      onFocus={onFocus}
      onBlur={onBlur}
    >
      <TreeHeader
        name={project.name}
        color={project.color}
        root={{ id: rootId, selected: selected === "", onSelect: onSelectRoot }}
      />
      <div
        ref={scrollerRef}
        className="file-tree__scroller"
        role="group"
        data-testid="tree-scroller"
        onScroll={measure}
        onClick={onScrollerClick}
      >
        {rootPending || empty ? (
          <p className="tree-placeholder" data-testid="tree-placeholder">
            {rootPending ? "読み込み中…" : "このフォルダは空です"}
          </p>
        ) : (
          <div className="file-tree__rows" style={{ paddingTop: win.paddingTop, paddingBottom: win.paddingBottom }}>
            {visible.map((row, offset) => {
              const index = win.start + offset;
              return (
                <TreeRow
                  key={row.path}
                  row={row}
                  id={rowId(index)}
                  selected={row.path === selected}
                  onSelect={onSelect}
                  onToggle={onToggle}
                  onActivate={onActivate}
                />
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}
