// 写し(AppSnapshot)から画面に出す値を取り出す関数(設計書 §6.1)。写しは書き換えない。
// 部品(T34c・T34d)はストアから写しを読み、ここの関数で必要な値にする。

import type { AppSnapshot } from "../ipc/generated/AppSnapshot";
import type { DisplayMode } from "../ipc/generated/DisplayMode";
import type { EditorId } from "../ipc/generated/EditorId";
import type { EditorTabSnapshot } from "../ipc/generated/EditorTabSnapshot";
import type { ProjectCardSnapshot } from "../ipc/generated/ProjectCardSnapshot";
import type { ProjectColor } from "../ipc/generated/ProjectColor";
import type { ProjectId } from "../ipc/generated/ProjectId";
import type { RelPath } from "../ipc/generated/RelPath";
import type { TerminalId } from "../ipc/generated/TerminalId";
import type { TerminalStatus } from "../ipc/generated/TerminalStatus";
import type { TerminalTabSnapshot } from "../ipc/generated/TerminalTabSnapshot";
import type { TileGrid } from "../ipc/generated/TileGrid";
import type { TreeSelectionKind } from "../logic/keys/clipboard";
import type { ShortcutTable } from "../logic/keys/shortcutTable";
import type { TreeMenuContext, TreeMenuTarget } from "../logic/menu/treeMenu";
import { flattenRows, type TreeModel, type TreeRow } from "../logic/tree/treeModel";
import { nameOf, parentPath } from "../logic/tree/treePath";

/** 選択中のプロジェクト。選択が無い・一覧に無い・写しが無いなら null。 */
export function selectedProject(snapshot: AppSnapshot | null): ProjectCardSnapshot | null {
  if (snapshot === null || snapshot.selectedProjectId === null) return null;
  return snapshot.projects.find((p) => p.id === snapshot.selectedProjectId) ?? null;
}

/** プロジェクトのタブ(タブの順)。写しのターミナルの一覧に無い ID は飛ばす。 */
export function projectTabs(snapshot: AppSnapshot | null, projectId: ProjectId): TerminalTabSnapshot[] {
  const project = snapshot?.projects.find((p) => p.id === projectId);
  if (snapshot === null || project === undefined) return [];
  const byId = new Map(snapshot.terminals.map((t) => [t.id, t]));
  return project.terminalIds.flatMap((id) => byId.get(id) ?? []);
}

/** 表示中のターミナル(選択中のプロジェクトの表示中のタブ)。タブが 0 枚・写しに無いなら null。 */
export function shownTerminal(snapshot: AppSnapshot | null): TerminalTabSnapshot | null {
  const activeId = selectedProject(snapshot)?.activeTerminalId ?? null;
  if (snapshot === null || activeId === null) return null;
  return snapshot.terminals.find((t) => t.id === activeId) ?? null;
}

/** プロジェクトのエディタのタブ(開いた順。M4.8)。写しのエディタの一覧に無い ID は飛ばす。 */
export function projectEditorTabs(snapshot: AppSnapshot | null, projectId: ProjectId): EditorTabSnapshot[] {
  const project = snapshot?.projects.find((p) => p.id === projectId);
  if (snapshot === null || project === undefined) return [];
  const byId = new Map(snapshot.editors.map((e) => [e.id, e]));
  return project.editorIds.flatMap((id) => byId.get(id) ?? []);
}

/**
 * いま出しているエディタ(M4.8。ADR-015 の 3。Rust の `Workspace::shown_editor` と同じ規則)。タブ表示で、選択中の
 * プロジェクトの表示中のエディタがあるときだけ。タイル表示ではエディタを出さないので null。
 */
export function shownEditor(snapshot: AppSnapshot | null): EditorTabSnapshot | null {
  if (snapshot === null || snapshot.display.mode !== "tabs") return null;
  const id = selectedProject(snapshot)?.shownEditorId ?? null;
  if (id === null) return null;
  return snapshot.editors.find((e) => e.id === id) ?? null;
}

/** プロジェクトの開いている文書の親フォルダ(ルート基準。ルートは空の文字列)。外での変更を確かめる対象を選ぶのに使う。 */
export function editorParentDirs(snapshot: AppSnapshot | null, projectId: ProjectId): Map<EditorId, RelPath> {
  return new Map(projectEditorTabs(snapshot, projectId).map((e) => [e.id, parentPath(e.path) ?? ""]));
}

/**
 * 画面に見えるターミナルの並び(0〜9 枚。NFR-009、ADR-011。Rust の `Workspace::visible_terminals` と同じ規則)。
 * - タブ表示: 選択中のプロジェクトの表示中のタブ(0〜1 枚)。エディタを出しているときは 0 枚(M4.8)
 * - タイル表示: 写しの `display.tileIds`(表示中のページのタイル。並びの順。グリッドとページは Rust が計算したもの)
 * - 拡大中: フォーカス中の 1 枚(`display.focusedTerminalId`)
 * 写しのターミナルの一覧に無い ID は入れない。写しが無ければ空。
 */
export function visibleTerminalIds(snapshot: AppSnapshot | null): TerminalId[] {
  if (snapshot === null) return [];
  const { display } = snapshot;
  if (display.mode === "tabs") {
    if (shownEditor(snapshot) !== null) return [];
    const shown = shownTerminal(snapshot);
    return shown === null ? [] : [shown.id];
  }
  const known = new Set(snapshot.terminals.map((t) => t.id));
  const ids = display.maximized
    ? display.focusedTerminalId === null
      ? []
      : [display.focusedTerminalId]
    : display.tileIds;
  return ids.filter((id) => known.has(id));
}

/** 見えるターミナルの並びと、隠して出力の取り込みだけを続けるターミナル(残りの全部。写しの順)。 */
export interface Presentation {
  readonly shown: readonly TerminalId[];
  readonly background: readonly TerminalId[];
}

/** 写しから、見えるターミナルの並びと隠すターミナルを決める(FR-042、FR-052、NFR-009)。 */
export function presentation(snapshot: AppSnapshot | null): Presentation {
  const shown = visibleTerminalIds(snapshot);
  const visible = new Set(shown);
  const background = (snapshot?.terminals ?? []).map((t) => t.id).filter((id) => !visible.has(id));
  return { shown, background };
}

// ── タイル(M4。T47e)。写しの display をタイルの部品(T47f・T47g)がそのまま描ける形にする ──

/** タイル 1 枚(表示中のページのもの)。 */
export interface TileItem {
  readonly id: TerminalId;
  /** タイトルバーの文字列「プロジェクト名: タブ名」(FR-061)。持ち主の無いターミナルはタブ名だけ。 */
  readonly title: string;
  readonly projectId: ProjectId | null;
  /** プロジェクト色(FR-060。色の印)。持ち主の無いターミナルは null。 */
  readonly color: ProjectColor | null;
  readonly status: TerminalStatus;
  /** アテンションの印(FR-055)。 */
  readonly attention: boolean;
  /** フォーカス中のタイルか(=選択中のプロジェクトの表示中のタブ)。 */
  readonly focused: boolean;
  /**
   * 表示中のページの中の並び順(1 から。写しの display.tileIds の位置。⌘1〜9 の番号と同じ。ADR-014 の 4)。
   * 表示中のページに無いタイル(拡大中のフォーカス中のタイルの補い)は 0。
   */
  readonly position: number;
}

/** ページ表示(`1/2` と前後のボタン)。ページが 2 つ以上のタイル表示(拡大中でない)だけで出す(【仮定】3)。 */
export interface TilePaging {
  /** `1/2` の形(ページは 1 から数える)。 */
  readonly label: string;
  /** 前・次のページへ進めるか(端では進めない。循環しない)。 */
  readonly canGoPrevious: boolean;
  readonly canGoNext: boolean;
  /** 前側・次側のページにアテンションの印のあるタイルがあるか(FR-055 の 2 つ目)。 */
  readonly previousAttention: boolean;
  readonly nextAttention: boolean;
}

/** 右ペインの表示(タイルの部品が読む形)。グリッドとページは Rust が計算したものをそのまま使う。 */
export interface TileView {
  readonly mode: DisplayMode;
  readonly maximized: boolean;
  /** 表示中のページのグリッド(列×行)。 */
  readonly grid: TileGrid;
  /** 表示中のページのタイル(並びの順)。写しのターミナルの一覧に無い ID は入れない。 */
  readonly tiles: readonly TileItem[];
  /** フォーカス中のタイル(拡大中のタイトル帯に使う)。無ければ null。 */
  readonly focusedTile: TileItem | null;
  /** ページ表示。出さないときは null。 */
  readonly paging: TilePaging | null;
}

const EMPTY_TILE_VIEW: TileView = {
  mode: "tabs",
  maximized: false,
  grid: { cols: 0, rows: 0 },
  tiles: [],
  focusedTile: null,
  paging: null,
};

const tileViewCache = new WeakMap<AppSnapshot, TileView>();
/** 直前に返した結果。中身が同じ部分は前の参照を使い回す(写しが届くたびにタイルを描き直さないため)。 */
let lastTileView: TileView = EMPTY_TILE_VIEW;

function sameStatus(a: TerminalStatus, b: TerminalStatus): boolean {
  return a === b || JSON.stringify(a) === JSON.stringify(b);
}

function sameTile(a: TileItem, b: TileItem): boolean {
  return (
    a.id === b.id &&
    a.title === b.title &&
    a.projectId === b.projectId &&
    a.color === b.color &&
    a.attention === b.attention &&
    a.focused === b.focused &&
    a.position === b.position &&
    sameStatus(a.status, b.status)
  );
}

function samePaging(a: TilePaging | null, b: TilePaging | null): boolean {
  if (a === null || b === null) return a === b;
  return (
    a.label === b.label &&
    a.canGoPrevious === b.canGoPrevious &&
    a.canGoNext === b.canGoNext &&
    a.previousAttention === b.previousAttention &&
    a.nextAttention === b.nextAttention
  );
}

function buildTileView(snapshot: AppSnapshot, previous: TileView): TileView {
  const { display } = snapshot;
  const projects = new Map(snapshot.projects.map((p) => [p.id, p]));
  const terminals = new Map(snapshot.terminals.map((t) => [t.id, t]));
  const reuse = new Map(previous.tiles.map((t) => [t.id, t]));
  if (previous.focusedTile !== null) reuse.set(previous.focusedTile.id, previous.focusedTile);

  const tileOf = (id: TerminalId, position: number): TileItem | null => {
    const terminal = terminals.get(id);
    if (terminal === undefined) return null;
    const project = terminal.projectId === null ? undefined : projects.get(terminal.projectId);
    const item: TileItem = {
      id,
      title: project === undefined ? terminal.label : `${project.name}: ${terminal.label}`,
      projectId: terminal.projectId,
      color: project?.color ?? null,
      status: terminal.status,
      attention: terminal.attention,
      focused: id === display.focusedTerminalId,
      position,
    };
    const old = reuse.get(id);
    return old !== undefined && sameTile(old, item) ? old : item;
  };

  const tiles = display.tileIds.flatMap((id, index) => tileOf(id, index + 1) ?? []);
  const focusedId = display.focusedTerminalId;
  const focusedTile =
    focusedId === null ? null : (tiles.find((t) => t.id === focusedId) ?? tileOf(focusedId, 0));
  const { page, pageCount, attentionPages } = display;
  const paging: TilePaging | null =
    display.mode === "tiles" && !display.maximized && pageCount >= 2
      ? {
          label: `${page + 1}/${pageCount}`,
          canGoPrevious: page > 0,
          canGoNext: page < pageCount - 1,
          previousAttention: attentionPages.some((p) => p < page),
          nextAttention: attentionPages.some((p) => p > page),
        }
      : null;
  const grid =
    previous.grid.cols === display.grid.cols && previous.grid.rows === display.grid.rows ? previous.grid : display.grid;

  const next: TileView = {
    mode: display.mode,
    maximized: display.maximized,
    grid,
    tiles:
      tiles.length === previous.tiles.length && tiles.every((t, i) => t === previous.tiles[i]) ? previous.tiles : tiles,
    focusedTile,
    paging: samePaging(previous.paging, paging) ? previous.paging : paging,
  };
  const unchanged =
    next.mode === previous.mode &&
    next.maximized === previous.maximized &&
    next.grid === previous.grid &&
    next.tiles === previous.tiles &&
    next.focusedTile === previous.focusedTile &&
    next.paging === previous.paging;
  return unchanged ? previous : next;
}

/**
 * 写しから右ペインの表示(タイル・グリッド・ページ表示・拡大中・表示モード)を取り出す(FR-048・FR-051〜055・FR-061)。
 * 同じ写しには同じ結果を返す。写しが替わっても中身が同じなら前の結果(タイルは 1 枚ずつ)の参照を返す
 * (zustand の購読で描き直しを増やさないため)。写しが無ければ空のタブ表示。
 */
export function tileView(snapshot: AppSnapshot | null): TileView {
  if (snapshot === null) return EMPTY_TILE_VIEW;
  const cached = tileViewCache.get(snapshot);
  if (cached !== undefined) {
    lastTileView = cached;
    return cached;
  }
  const view = buildTileView(snapshot, lastTileView);
  tileViewCache.set(snapshot, view);
  lastTileView = view;
  return view;
}

// ── ツリー(M3。T43a)。treeStore.ts の TreeState から読む。循環を避けるため、型は形だけ受け取る ──

/** ツリーの状態のうち、ここで読む部分(TreeState はこの形を満たす)。 */
export interface TreeView {
  readonly activeProjectId: ProjectId | null;
  readonly trees: ReadonlyMap<ProjectId, TreeModel>;
}

/** 表示中のツリーのモデル。ツリーを出していなければ null。 */
export function activeTreeModel(tree: TreeView): TreeModel | null {
  if (tree.activeProjectId === null) return null;
  return tree.trees.get(tree.activeProjectId) ?? null;
}

const EMPTY_ROWS: readonly TreeRow[] = [];
const rowsCache = new WeakMap<TreeModel, TreeRow[]>();

/**
 * 表示中のツリーの行(T41a の flattenRows)。同じモデルには同じ配列を返す(zustand の購読で描き直しを増やさないため)。
 * ツリーを出していなければ空の配列。
 */
export function activeTreeRows(tree: TreeView): readonly TreeRow[] {
  const model = activeTreeModel(tree);
  if (model === null) return EMPTY_ROWS;
  const cached = rowsCache.get(model);
  if (cached !== undefined) return cached;
  const rows = flattenRows(model);
  rowsCache.set(model, rows);
  return rows;
}

/**
 * 選んでいる項目を、メニューの場所(T41b の TreeMenuTarget)にする。ショートカットの振り分け(treeShortcutCommand)に使う。
 * 選択が無い・項目が親の中身に無いなら null。ルートは root、フォルダは folder、それ以外(リンクを含む)は file。
 */
export function selectedTreeTarget(tree: TreeView): TreeMenuTarget | null {
  const model = activeTreeModel(tree);
  const selected = model?.selected ?? null;
  if (model === null || selected === null) return null;
  if (selected === "") return { kind: "root" };
  const name = nameOf(selected);
  const entry = model.listings.get(parentPath(selected) ?? "")?.find((e) => e.name === name);
  if (entry === undefined) return null;
  return entry.kind === "directory" ? { kind: "folder", path: selected } : { kind: "file", path: selected };
}

/** コピー・切り取り・ペーストの振り分け(T41b の routeClipboard)に渡す、ツリーの選択の種類。 */
export function treeSelectionKind(tree: TreeView): TreeSelectionKind {
  const target = selectedTreeTarget(tree);
  if (target === null) return "none";
  return target.kind === "root" ? "root" : "entry";
}

/** ツリーのメニュー(T41b の buildTreeMenu)に渡す画面の値。表示中のタブと隠しファイル表示は写しから読む。 */
export function treeMenuContext(snapshot: AppSnapshot | null, shortcuts: ShortcutTable): TreeMenuContext {
  return {
    hasTab: shownTerminal(snapshot) !== null,
    showHidden: selectedProject(snapshot)?.showHidden ?? false,
    shortcuts,
  };
}
