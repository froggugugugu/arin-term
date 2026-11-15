// ツリーの表示モデル(設計書 §5.1 の `TreeModel`・`applyDirListing`、計画 T41a)。DOM を使わない純粋な関数。
//
// 1 つのプロジェクトのツリーを表す。読み込み済みのフォルダの中身、展開の集合、選択 1 つ(【仮定】6)を持ち、
// 表示する行の列は `flattenRows` で作る。どの関数も元の値を変えず、新しい値を返す(zustand に載せるため)。
//
// - フォルダは展開したときに初めて読む(FR-013)。展開して未読込なら「読み込みが要る」を返し、
//   呼び出し側(`src/state` の `treeStore`。計画 T43a)が Rust に中身を求め、届いたら `applyDirListing` で当てる
// - 折りたたんでも子の展開と読み込み済みの中身は覚えておき、もう一度開くと戻す
// - 中身の差分の適用で、消えたフォルダ(とフォルダでなくなった項目)の子孫の中身・展開・選択を外す
// - ルート(空の文字列)は常に開いている(【仮定】1)。`expanded` にはルートを入れない

import { isSameOrDescendant, joinPath, parentPath } from "./treePath";
import type { TreeChange, TreeEntry, TreeEntryKind } from "./types";

/** ツリーの表示モデル。 */
export interface TreeModel {
  /** 読み込み済みのフォルダの中身(キーはルート基準の相対パス。ルートは空の文字列)。読み込んだ順に並ぶ。 */
  readonly listings: ReadonlyMap<string, readonly TreeEntry[]>;
  /** 展開しているフォルダ。折りたたまれた祖先の下にあっても覚えておく。 */
  readonly expanded: ReadonlySet<string>;
  /** 中身を求めて、まだ届いていないフォルダ。 */
  readonly loading: ReadonlySet<string>;
  /** 選んでいる項目。空の文字列はルート(ヘッダ。【仮定】1)、`null` は選択なし。 */
  readonly selected: string | null;
}

/** 表示する 1 行。 */
export interface TreeRow {
  /** ルート基準の相対パス。 */
  readonly path: string;
  readonly name: string;
  readonly kind: TreeEntryKind;
  /** ルートの直下が 0。 */
  readonly depth: number;
  /** 開閉の印を出すか(フォルダだけ。フォルダへのリンクは展開しない)。 */
  readonly expandable: boolean;
  readonly expanded: boolean;
  /** 開いていて中身を待っている。 */
  readonly loading: boolean;
}

/** 開閉の結果。`needsLoad` が真なら、呼び出し側がそのフォルダの中身を求める。 */
export interface ExpandResult {
  readonly model: TreeModel;
  readonly needsLoad: boolean;
}

/** 作った直後のモデル。ルートの中身を待っている(呼び出し側がルートを読む)。 */
export function createTreeModel(): TreeModel {
  return { listings: new Map(), expanded: new Set(), loading: new Set([""]), selected: null };
}

/** フォルダの中身が読み込み済みか。 */
export function isLoaded(model: TreeModel, dir: string): boolean {
  return model.listings.has(dir);
}

/** 読み込み済みのフォルダの一覧(読み込んだ順)。プロジェクトに戻ったときの読み直しに使う(【仮定】4)。 */
export function loadedDirs(model: TreeModel): string[] {
  return [...model.listings.keys()];
}

/** 親の中身から項目を探す。親が未読込か、見つからなければ `undefined`。 */
function findEntry(model: TreeModel, path: string): TreeEntry | undefined {
  const parent = parentPath(path);
  if (parent === null) {
    return undefined;
  }
  const name = path.slice(parent === "" ? 0 : parent.length + 1);
  return model.listings.get(parent)?.find((entry) => entry.name === name);
}

/** `path` 自身と子孫の中身・展開・読み込み中・選択を外す。 */
function dropSubtree(model: TreeModel, path: string): TreeModel {
  const keep = (key: string): boolean => !isSameOrDescendant(key, path);
  return {
    listings: new Map([...model.listings].filter(([key]) => keep(key))),
    expanded: new Set([...model.expanded].filter(keep)),
    loading: new Set([...model.loading].filter(keep)),
    selected: model.selected !== null && !keep(model.selected) ? null : model.selected,
  };
}

/** フォルダを開く。未読込で、まだ求めていなければ読み込み中にして `needsLoad: true` を返す。 */
export function expandDir(model: TreeModel, dir: string): ExpandResult {
  if (findEntry(model, dir)?.kind !== "directory") {
    // ルート(常に開いている)、フォルダでない項目、知らない項目
    return { model, needsLoad: false };
  }
  const needsLoad = !model.listings.has(dir) && !model.loading.has(dir);
  return {
    model: {
      ...model,
      expanded: new Set(model.expanded).add(dir),
      loading: needsLoad ? new Set(model.loading).add(dir) : model.loading,
    },
    needsLoad,
  };
}

/**
 * フォルダを閉じる。子の展開と中身は覚えておく。
 * 選んでいた項目が隠れるなら、閉じたフォルダを選ぶ【仮定】(隠れた行を選んだままにしない)。
 */
export function collapseDir(model: TreeModel, dir: string): TreeModel {
  if (!model.expanded.has(dir)) {
    return model;
  }
  const expanded = new Set(model.expanded);
  expanded.delete(dir);
  const hidesSelection = model.selected !== null && model.selected !== dir && isSameOrDescendant(model.selected, dir);
  return { ...model, expanded, selected: hidesSelection ? dir : model.selected };
}

/** 開閉を切り替える(開閉の印のクリック、フォルダのダブルクリック。【仮定】6)。 */
export function toggleDir(model: TreeModel, dir: string): ExpandResult {
  return model.expanded.has(dir) ? { model: collapseDir(model, dir), needsLoad: false } : expandDir(model, dir);
}

/**
 * すべて折りたたむ。ルートの直下だけが見える。読み込み済みの中身は捨てない(開き直すとすぐ出る)。
 * 選んでいた項目が隠れるなら、それを含むルートの直下の項目を選ぶ。
 */
export function collapseAll(model: TreeModel): TreeModel {
  const selected = model.selected;
  const slash = selected === null ? -1 : selected.indexOf("/");
  return {
    ...model,
    expanded: new Set(),
    selected: selected !== null && slash >= 0 ? selected.slice(0, slash) : selected,
  };
}

/** 項目を選ぶ。空の文字列はルート、`null` は選択を外す。 */
export function selectPath(model: TreeModel, path: string | null): TreeModel {
  return { ...model, selected: path };
}

/**
 * 届いたフォルダの中身を当てる(読み込みと、変更による読み直しの両方)。
 * 前の中身にあって、新しい中身では消えた(またはフォルダでなくなった)項目の子孫の中身・展開・選択を外す。
 * 残った項目の展開と選択はそのまま。
 * 親が未読込か、親の中身にフォルダとして無いフォルダの結果(求めたあとに消えた)は捨てる。
 */
export function applyDirListing(model: TreeModel, dir: string, entries: readonly TreeEntry[]): TreeModel {
  const loading = new Set(model.loading);
  loading.delete(dir);
  if (dir !== "" && findEntry(model, dir)?.kind !== "directory") {
    return dropSubtree({ ...model, loading }, dir);
  }

  let next: TreeModel = { ...model, loading };
  const previous = model.listings.get(dir) ?? [];
  const kinds = new Map(entries.map((entry) => [entry.name, entry.kind]));
  for (const entry of previous) {
    const kind = kinds.get(entry.name);
    if (kind === undefined || (entry.kind === "directory" && kind !== "directory")) {
      next = dropSubtree(next, joinPath(dir, entry.name));
    }
  }
  return { ...next, listings: new Map(next.listings).set(dir, entries) };
}

/**
 * フォルダを忘れる(中身を求めたら見つからない・フォルダでないと返ったとき)。
 * そのフォルダ自身と子孫の中身・展開・選択を外す。ルートを忘れると、ルートの読み込みを待つ状態に戻る。
 */
export function forgetDir(model: TreeModel, dir: string): TreeModel {
  const dropped = dropSubtree(model, dir);
  return dir === "" ? { ...dropped, loading: new Set([""]) } : dropped;
}

/**
 * 表示する行の列。開いていて読み込み済みのフォルダだけをたどる(深さ優先、中身の順のまま)。
 * 行の数は見えている行の数だけになる。描くのは仮想スクロールの窓の中だけ(`virtualWindow`)。
 */
export function flattenRows(model: TreeModel): TreeRow[] {
  const rows: TreeRow[] = [];
  const visit = (dir: string, depth: number): void => {
    for (const entry of model.listings.get(dir) ?? []) {
      const path = joinPath(dir, entry.name);
      const expandable = entry.kind === "directory";
      const expanded = expandable && model.expanded.has(path);
      rows.push({
        path,
        name: entry.name,
        kind: entry.kind,
        depth,
        expandable,
        expanded,
        loading: expanded && model.loading.has(path),
      });
      if (expanded) {
        visit(path, depth + 1);
      }
    }
  };
  visit("", 0);
  return rows;
}

/**
 * 届いた変更から、読み直すフォルダを選ぶ(設計書 §7.1「外部でのファイル変更」)。
 * 読み込み済みのフォルダだけを、重なりを除いて届いた順に返す。未読込のフォルダの変更は無視する
 * (折りたたまれた祖先の目印は M6)。「全部読み直し」の印なら読み込み済みのフォルダをすべて返す。
 * 返した一覧は `UpdateGate` に渡し、保留が無ければ読み直す。
 */
export function dirsToReload(model: TreeModel, change: TreeChange): string[] {
  if (change.fullReload) {
    return loadedDirs(model);
  }
  return [...new Set(change.dirs)].filter((dir) => model.listings.has(dir));
}
