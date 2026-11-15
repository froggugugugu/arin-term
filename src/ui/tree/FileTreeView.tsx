import "./TreeView.css";

import {
  type CSSProperties,
  type FocusEvent,
  type KeyboardEvent,
  type MouseEvent,
  type ReactNode,
  useCallback,
  useId,
  useLayoutEffect,
  useRef,
  useState,
} from "react";
import { useStore } from "zustand";

import { routeShortcutKey } from "../../logic/keys/routeShortcutKey";
import { buildTreeMenu, type TreeMenuTarget } from "../../logic/menu/treeMenu";
import type { TreeRow as TreeRowValue } from "../../logic/tree/treeModel";
import { nameOf } from "../../logic/tree/treePath";
import { computeRowWindow, revealRowOffset } from "../../logic/tree/virtualWindow";
import type { Actions } from "../../state/actions";
import { activeTreeModel, activeTreeRows, selectedTreeTarget } from "../../state/selectors";
import type { TreeState, TreeStore } from "../../state/treeStore";
import type { InlineEdit, UiStore } from "../../state/uiStore";
import { ContextMenu } from "../menu/ContextMenu";
import {
  runTreeMenuCommand,
  runTreeShortcut,
  treeCommandScope,
  type TreeMenuActions,
  treeMenuContextOf,
} from "../menu/runTreeMenuCommand";
import type { ProjectCard } from "../snapshotTypes";
import { FileIcon, FolderIcon } from "./icons";
import { type InlineEditEnd, InlineNameInput } from "./InlineNameInput";
import { TreeHeader } from "./TreeHeader";
import { TreeRow } from "./TreeRow";

interface FileTreeViewProps {
  /** ツリーを出すプロジェクト(選択中で「見つからない」でないもの)。切り替えたら key で作り直す(スクロールを戻すため)。 */
  project: ProjectCard;
  tree: TreeStore;
  ui: UiStore;
  actions: TreeMenuActions & Pick<Actions, "selectTreePath" | "toggleDir" | "commitInlineEdit">;
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

/** 新規作成の入力欄を差し込む位置(行の並びの番号)と深さ。作る先のフォルダの最初の子の位置(VSCode と同じ)。 */
interface NewEntrySlot {
  readonly index: number;
  readonly depth: number;
}

function newEntrySlot(rows: readonly TreeRowValue[], parent: string): NewEntrySlot {
  if (parent === "") return { index: 0, depth: 0 };
  const index = rows.findIndex((r) => r.path === parent);
  const parentRow = rows[index];
  // 【仮定】作る先のフォルダが並びに無い(読み直しで消えた)ときは、ルートの先頭に出す(確定すると Rust が判定する)。
  if (parentRow === undefined) return { index: 0, depth: 0 };
  return { index: index + 1, depth: parentRow.depth + 1 };
}

/** 行の並びの番号(rows)を、描く並びの番号(入力欄を含む)にする。 */
function itemIndexOf(rowIndex: number, at: NewEntrySlot | null): number {
  return at !== null && rowIndex >= at.index ? rowIndex + 1 : rowIndex;
}

/** 押された要素から、メニューを開く場所を決める(行は data-path・data-kind、ヘッダはルート、それ以外は余白)。 */
function menuTargetAt(element: Element): TreeMenuTarget {
  const rowElement = element.closest<HTMLElement>('[data-testid="tree-row"]');
  if (rowElement !== null) {
    const path = rowElement.dataset.path ?? "";
    return rowElement.dataset.kind === "directory" ? { kind: "folder", path } : { kind: "file", path };
  }
  if (element.closest('[data-testid="tree-header"]') !== null) return { kind: "root" };
  return { kind: "blank" };
}

/** メニューの場所の選択(右クリックした行を選ぶ。余白は選択を外す)。 */
function selectionOf(target: TreeMenuTarget): string | null {
  switch (target.kind) {
    case "file":
    case "folder":
      return target.path;
    case "root":
      return "";
    case "blank":
      return null;
  }
}

/** 開いたメニューの読み上げの名前。 */
function menuLabel(target: TreeMenuTarget, projectName: string): string {
  return target.kind === "file" || target.kind === "folder" ? nameOf(target.path) : projectName;
}

/**
 * ファイルツリー(Container。設計書 §9.1、ADR-007 の (c)、NFR-005)。treeStore の表示中のツリーを読み、
 * ルートの行(ヘッダ)と、T41a の窓(computeRowWindow)で決めた見える行だけを描く。上下は余白で埋め、
 * スクロールの高さは全部の行ぶんにする。スクロールで窓が変わらないあいだは描き直さない。
 *
 * - WAI-ARIA の tree。フォーカスはツリーの要素 1 つが持ち(Tab で入れる)、選んだ行を aria-activedescendant で指す
 *   (矢印キーでの移動は M3 では入れない。計画【仮定】6)
 * - フォーカスの出入りを uiStore.setTreeFocused で伝える(キーとコピー・ペーストの振り分けが使う)。名前の入力欄に
 *   フォーカスがあるときはツリーのフォーカスにしない。選択の塗りの色(アクセント色かグレーか)は data-focused で CSS が決める
 * - 選択が変わったら、その行が見えるようにスクロールする(revealRowOffset。作った項目を選んだときなど)
 * - クリックで選択、開閉の印で開閉、ダブルクリックでフォルダは開閉・それ以外は既定のアプリで開く(計画【仮定】6)。
 *   行の無い余白のクリックで選択を外す
 * - 右クリック(行・ヘッダ・余白。T43c): 右クリックした場所を選び、src/logic/menu の buildTreeMenu の項目をメニューに出す。
 *   メニューは Esc・外側のクリック・項目の実行で閉じ、閉じたらツリーにフォーカスを戻す。Shift+F10 と
 *   コンテキストメニューのキーでも、選んでいる項目のメニューを開く(カードのメニューと同じ)
 * - キー(T43c): ツリーにフォーカスがあるときの keydown を、ショートカットの表(uiStore.shortcuts)から
 *   routeShortcutKey で引き、選んでいる項目のメニューの同じ項目を実行する(キーの文字列をここに書かない)
 * - インライン編集(T43c): 名称変更は行の名前の位置に、新規作成は作る先のフォルダの最初の子の位置に入力欄を出す
 *   (作る先のフォルダが閉じていれば開く)。確定と Esc ではツリーにフォーカスを戻す
 * - メニュー・入力中の更新の保留(FR-037)は uiStore の状態から treeStore が決める(ここでは立てない)
 *
 * T43d は行の pointer のイベントを足す。
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
  const treeMenu = useStore(ui, (s) => (s.treeMenu?.projectId === projectId ? s.treeMenu : null));
  const shortcuts = useStore(ui, (s) => s.shortcuts);
  const inlineEdit = useStore(ui, (s) => (s.inlineEdit?.projectId === projectId ? s.inlineEdit : null));

  const baseId = useId();
  const rowId = (index: number) => `${baseId}-row-${index}`;
  const rootId = `${baseId}-root`;

  const treeRef = useRef<HTMLDivElement>(null);
  const scrollerRef = useRef<HTMLDivElement>(null);
  const [rowHeight, setRowHeight] = useState(FALLBACK_ROW_HEIGHT);
  const [viewport, setViewport] = useState<Viewport>({ scrollTop: 0, height: 0 });

  // 新規作成の入力欄を差し込む位置。並びの番号は、これより後ろの行で 1 つずれる。
  const createEdit = inlineEdit?.mode === "create" ? inlineEdit : null;
  const slot = createEdit === null ? null : newEntrySlot(rows, createEdit.parent);
  const itemCount = rows.length + (slot === null ? 0 : 1);
  const renamingPath = inlineEdit?.mode === "rename" ? inlineEdit.path : null;

  // 描画のたびに最新の値を置く(スクロールの受け口と選択の追従が読む)。
  const rowsRef = useRef(rows);
  rowsRef.current = rows;
  const itemCountRef = useRef(itemCount);
  itemCountRef.current = itemCount;
  const slotRef = useRef(slot);
  slotRef.current = slot;

  /** スクロールの領域の今の値を測り、窓が変わるときだけ状態を更新する。 */
  const measure = useCallback(() => {
    const element = scrollerRef.current;
    if (element === null) return;
    const next: Viewport = { scrollTop: element.scrollTop, height: element.clientHeight };
    setViewport((prev) => {
      if (prev.height !== next.height) return next;
      const input = { rowHeight, rowCount: itemCountRef.current, overscan: OVERSCAN };
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

  /** 描く並びの番号の行が見えるようにスクロールする。 */
  const revealItem = useCallback(
    (index: number) => {
      const element = scrollerRef.current;
      if (element === null || index < 0) return;
      const offset = revealRowOffset({
        scrollTop: element.scrollTop,
        viewportHeight: element.clientHeight,
        rowHeight,
        index,
      });
      if (offset !== 0) element.scrollTop += offset;
    },
    [rowHeight],
  );

  // 選択が変わったら、その行が見えるようにする(行の数が変わっただけでは動かさない)。
  useLayoutEffect(() => {
    if (selected === null || selected === "") return;
    const index = rowsRef.current.findIndex((r) => r.path === selected);
    if (index >= 0) revealItem(itemIndexOf(index, slotRef.current));
  }, [selected, revealItem]);

  // 新規作成の入力欄が出たら見えるようにし、作る先のフォルダが閉じていれば開く。
  const slotIndex = slot?.index ?? null;
  useLayoutEffect(() => {
    if (slotIndex !== null) revealItem(slotIndex);
  }, [slotIndex, revealItem]);
  const createParent = createEdit?.parent ?? null;
  useLayoutEffect(() => {
    if (createParent === null || createParent === "") return;
    const parentRow = rowsRef.current.find((r) => r.path === createParent);
    if (parentRow !== undefined && parentRow.expandable && !parentRow.expanded) actions.expandDir(createParent);
  }, [createParent, actions]);

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

  /**
   * インライン編集を閉じる。edit は入力欄を出したときの編集(別の編集に置き換わっていたら何もしない)。
   * 【仮定】名前が空か、名称変更で名前が変わっていなければ、何も呼ばずに取り消す(Finder と同じ)。
   */
  const endInlineEdit = useCallback(
    (edit: InlineEdit, end: InlineEditEnd, name: string) => {
      if (ui.getState().inlineEdit !== edit) return;
      if (end !== "blur") treeRef.current?.focus();
      const unchanged = edit.mode === "rename" && name === nameOf(edit.path);
      if (end === "commit" && name !== "" && !unchanged) void actions.commitInlineEdit(name);
      else ui.getState().closeInlineEdit();
    },
    [ui, actions],
  );
  // 入力欄を出したときの編集に結び付ける(別の編集に置き換わったあとの、古い入力欄のフォーカスの外れで閉じない)。
  const onRenameEnd = useCallback(
    (end: InlineEditEnd, name: string) => {
      if (inlineEdit !== null) endInlineEdit(inlineEdit, end, name);
    },
    [inlineEdit, endInlineEdit],
  );

  const onFocus = (event: FocusEvent<HTMLDivElement>) => {
    // 名前の入力欄はツリーのフォーカスにしない(キーとコピー・ペーストを入力欄に任せる)。
    const inInput = event.target instanceof HTMLElement && event.target.closest("[data-tree-name-input]") !== null;
    ui.getState().setTreeFocused(!inInput);
  };
  const onBlur = (event: FocusEvent<HTMLDivElement>) => {
    const next = event.relatedTarget;
    if (next instanceof Node && event.currentTarget.contains(next)) return;
    ui.getState().setTreeFocused(false);
  };
  // 行の無い余白のクリックで選択を外す。
  const onScrollerClick = (event: MouseEvent<HTMLDivElement>) => {
    if ((event.target as Element).closest('[role="treeitem"]') === null) actions.selectTreePath(null);
  };

  const openMenu = (target: TreeMenuTarget, x: number, y: number) => {
    actions.selectTreePath(selectionOf(target));
    ui.getState().openTreeMenu(projectId, target, { x, y });
  };
  const onContextMenu = (event: MouseEvent<HTMLDivElement>) => {
    event.preventDefault();
    openMenu(menuTargetAt(event.target as Element), event.clientX, event.clientY);
  };
  const closeMenu = () => {
    ui.getState().closeTreeMenu();
    treeRef.current?.focus();
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    // ツリーの要素そのものにフォーカスがあるときだけ(名前の入力欄のキーは入力欄が止める)。
    if (event.target !== event.currentTarget || event.nativeEvent.isComposing) return;
    // Shift+F10・コンテキストメニューのキーで、選んでいる項目のメニューを開く(カードのメニューと同じ。WAI-ARIA の作法)。
    if ((event.key === "F10" && event.shiftKey) || event.key === "ContextMenu") {
      event.preventDefault();
      const target = selectedTreeTarget(tree.getState()) ?? { kind: "blank" };
      const anchor = selected === "" ? treeRef.current?.querySelector('[data-testid="tree-header"]') : null;
      const selectedRow = treeRef.current?.querySelector('[data-testid="tree-row"][aria-selected="true"]');
      const rect = (selectedRow ?? anchor ?? scrollerRef.current)?.getBoundingClientRect();
      openMenu(target, (rect?.left ?? 0) + 24, (rect?.bottom ?? 0) - 4);
      return;
    }
    const id = routeShortcutKey(ui.getState().shortcuts, event, "tree");
    if (id === null) return;
    event.preventDefault();
    runTreeShortcut({ actions, ui, tree }, project, id);
  };

  const win = computeRowWindow({
    scrollTop: viewport.scrollTop,
    viewportHeight: viewport.height,
    rowHeight,
    rowCount: itemCount,
    overscan: OVERSCAN,
  });
  const selectedIndex = selected === null || selected === "" ? -1 : rows.findIndex((r) => r.path === selected);
  const selectedItem = selectedIndex < 0 ? -1 : itemIndexOf(selectedIndex, slot);
  const activeDescendant =
    selected === ""
      ? rootId
      : selectedItem >= win.start && selectedItem < win.end
        ? rowId(selectedItem)
        : undefined;
  const empty = !rootPending && rows.length === 0 && slot === null;

  const visible: ReactNode[] = [];
  for (let item = win.start; item < win.end; item++) {
    if (slot !== null && createEdit !== null && item === slot.index) {
      visible.push(
        <NewEntryRow
          key={`new:${createEdit.parent}:${createEdit.kind}`}
          id={rowId(item)}
          edit={createEdit}
          depth={slot.depth}
          onEnd={endInlineEdit}
        />,
      );
      continue;
    }
    const row = rows[slot !== null && item > slot.index ? item - 1 : item];
    if (row === undefined) continue;
    const renaming = row.path === renamingPath;
    visible.push(
      <TreeRow
        key={row.path}
        row={row}
        id={rowId(item)}
        selected={row.path === selected}
        onSelect={onSelect}
        onToggle={onToggle}
        onActivate={onActivate}
        renaming={renaming}
        onRenameEnd={renaming ? onRenameEnd : undefined}
      />,
    );
  }

  return (
    <>
      <div
        ref={treeRef}
        className="file-tree"
        role="tree"
        aria-label={`${project.name} のファイル`}
        aria-activedescendant={activeDescendant}
        aria-busy={rootPending ? true : undefined}
        tabIndex={0}
        data-testid="file-tree"
        data-focused={focused ? "true" : "false"}
        data-has-selection={selected !== null ? "true" : "false"}
        data-menu-open={treeMenu !== null ? "true" : undefined}
        onFocus={onFocus}
        onBlur={onBlur}
        onKeyDown={onKeyDown}
        onContextMenu={onContextMenu}
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
              {visible}
            </div>
          )}
        </div>
      </div>
      {treeMenu !== null && (
        <ContextMenu
          key={`${treeMenu.x}:${treeMenu.y}:${JSON.stringify(treeMenu.target)}`}
          label={menuLabel(treeMenu.target, project.name)}
          entries={buildTreeMenu(treeMenu.target, treeMenuContextOf(project, shortcuts))}
          x={treeMenu.x}
          y={treeMenu.y}
          onClose={closeMenu}
          onSelect={(command) => {
            closeMenu();
            void runTreeMenuCommand(actions, ui, treeCommandScope(project, treeMenu.target), command);
          }}
        />
      )}
    </>
  );
}

interface NewEntryRowProps {
  id: string;
  edit: Extract<InlineEdit, { mode: "create" }>;
  /** 字下げの深さ(作る先のフォルダの子の深さ)。 */
  depth: number;
  onEnd: (edit: InlineEdit, end: InlineEditEnd, name: string) => void;
}

/** 新規作成の入力欄の行(作る項目の種類のアイコンと、名前の入力欄。行の高さはほかの行と同じ)。 */
function NewEntryRow({ id, edit, depth, onEnd }: NewEntryRowProps) {
  const folder = edit.kind === "folder";
  return (
    <div
      id={id}
      role="treeitem"
      className="tree-row tree-row--editing"
      data-testid="tree-new-entry"
      aria-level={depth + 2}
      aria-selected={true}
      style={{ "--tree-depth": depth } as CSSProperties}
    >
      <span className="tree-row__inner">
        <span className="tree-row__chevron-slot" />
        <span className={folder ? "tree-icon tree-icon--folder" : "tree-icon tree-icon--file"}>
          {folder ? <FolderIcon /> : <FileIcon />}
        </span>
        <InlineNameInput
          initialName=""
          selection={null}
          label={folder ? "新しいフォルダの名前" : "新しいファイルの名前"}
          onEnd={(end, name) => onEnd(edit, end, name)}
        />
      </span>
    </div>
  );
}
