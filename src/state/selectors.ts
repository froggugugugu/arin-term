// 写し(AppSnapshot)から画面に出す値を取り出す関数(設計書 §6.1)。写しは書き換えない。
// 部品(T34c・T34d)はストアから写しを読み、ここの関数で必要な値にする。

import type { AppSnapshot } from "../ipc/generated/AppSnapshot";
import type { ProjectCardSnapshot } from "../ipc/generated/ProjectCardSnapshot";
import type { ProjectId } from "../ipc/generated/ProjectId";
import type { TerminalId } from "../ipc/generated/TerminalId";
import type { TerminalTabSnapshot } from "../ipc/generated/TerminalTabSnapshot";
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

/**
 * 画面に見えるターミナルの並び(0〜9 枚。NFR-009、ADR-011。Rust の `Workspace::visible_terminals` と同じ規則)。
 * - タブ表示: 選択中のプロジェクトの表示中のタブ(0〜1 枚)
 * - タイル表示: 写しの `display.tileIds`(表示中のページのタイル。並びの順。グリッドとページは Rust が計算したもの)
 * - 拡大中: フォーカス中の 1 枚(`display.focusedTerminalId`)
 * 写しのターミナルの一覧に無い ID は入れない。写しが無ければ空。
 */
export function visibleTerminalIds(snapshot: AppSnapshot | null): TerminalId[] {
  if (snapshot === null) return [];
  const { display } = snapshot;
  if (display.mode === "tabs") {
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
