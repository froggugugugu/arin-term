// 画面のツリーの状態(設計書 §6.1 の treeStore、計画 T43a)と、その読み込みの制御。Rust には送らない(§6.3)。
// モジュールの読み込み時には作らない。main.tsx が生成関数で作り、props で渡す(設計書 §11)。
//
// - プロジェクトごとに T41a の表示モデル(src/logic/tree/treeModel.ts)を持つ。切り替えても起動中はメモリに残し、
//   戻ったら読み込み済みのフォルダを読み直す(計画【仮定】4、設計書 §7.1「プロジェクトの切替」)。保存はしない
// - ツリーを出すのは、選択中で「見つからない」でないプロジェクトだけ(activeProjectId)。写しから消えたプロジェクトは忘れる
// - 外からの変更(tree-changed)は activeProjectId の分だけ受け、更新の保留(UpdateGate)の間は積む。保留の理由は uiStore の
//   メニュー・インライン編集・確認・ドラッグから決め(FR-037、【仮定】18)、すべて解けたら積んだフォルダをまとめて読み直す
// - 遅れて届いた応答は捨てる: 中身の問い合わせ 1 件ごとに番号を振り、(プロジェクト, フォルダ) の最新の番号だけを受ける。
//   届いた時点で、破棄済み・そのプロジェクトがもう表示中でない・同じフォルダをあとで求め直した、のどれかなら何も変えない
// - 中身の問い合わせの失敗(コマンドの Err)は reportError(診断)に 1 行で報告し、ツリーの知らせを出して、そのフォルダの
//   読み込み中の印を外して閉じる(開いた印のまま空に見せない)
// - 表示中のプロジェクトが替わったら、前のツリーの表示だけの状態(uiStore.resetTreeUi。前のプロジェクトの知らせを含む)を片付ける
//   (M4 ではタイルのクリックでも替わる。T47e)
//
// 部品(T43b〜T43d)は TreeStore を読み、開閉・選択は actions(./actions.ts)から呼ぶ(actions がここの TreeController を使う)。

import { createStore, type StoreApi } from "zustand/vanilla";

import type { ArinIpc } from "../ipc/client";
import type { DirListing } from "../ipc/generated/DirListing";
import type { ProjectId } from "../ipc/generated/ProjectId";
import type { RelPath } from "../ipc/generated/RelPath";
import type { TreeChange } from "../ipc/generated/TreeChange";
import { describeError } from "../logic/describeError";
import {
  applyDirListing,
  collapseAll,
  collapseDir,
  createTreeModel,
  dirsToReload,
  expandDir,
  forgetDir,
  isLoaded,
  loadedDirs,
  selectPath,
  type TreeModel,
  toggleDir,
} from "../logic/tree/treeModel";
import { isSameOrDescendant, nameOf, parentPath } from "../logic/tree/treePath";
import {
  createUpdateGate,
  type HoldReason,
  holdUpdates,
  offerUpdates,
  releaseUpdates,
  type UpdateGate,
} from "../logic/tree/updateGate";
import type { AppStore } from "./appStore";
import { selectedProject } from "./selectors";
import type { UiState, UiStore } from "./uiStore";

export interface TreeState {
  /** ツリーを出しているプロジェクト(選択中で「見つからない」でないもの)。無ければ null。 */
  readonly activeProjectId: ProjectId | null;
  /** プロジェクトごとの表示モデル。一度ツリーを出したプロジェクトだけが入る(写しから消えたら外す)。 */
  readonly trees: ReadonlyMap<ProjectId, TreeModel>;
  /** 更新の保留(activeProjectId のツリーの分)。プロジェクトを切り替えると積んだフォルダは捨てる(戻ったら読み直すため)。 */
  readonly gate: UpdateGate;
}

export type TreeStore = StoreApi<TreeState>;

export function createTreeStore(): TreeStore {
  return createStore<TreeState>()(() => ({
    activeProjectId: null,
    trees: new Map(),
    gate: createUpdateGate(),
  }));
}

export interface TreeControllerDeps {
  ipc: Pick<ArinIpc, "listDirectory">;
  tree: TreeStore;
  /** 写し。選択中のプロジェクトと「見つからない」を読む。 */
  app: AppStore;
  /** 保留の理由(メニュー・インライン編集・確認・ドラッグ)を読む。 */
  ui: UiStore;
  /** 失敗の報告先(ErrorReporter.report)。 */
  reportError: (message: string) => void;
}

/** ツリーの操作。どれも表示中のツリー(activeProjectId)だけを変え、ほかのプロジェクトの指定は何もしない。 */
export interface TreeController {
  /** フォルダを開く。未読込なら中身を求める(FR-013)。 */
  expand(path: RelPath): void;
  /** フォルダを閉じる。 */
  collapse(path: RelPath): void;
  /** 開閉を切り替える。 */
  toggle(path: RelPath): void;
  /** すべて折りたたむ。 */
  collapseAll(): void;
  /** 項目を選ぶ(空の文字列はルート、null は選択を外す)。 */
  select(path: RelPath | null): void;
  /**
   * 項目を選び、見えるように祖先のフォルダを開く(ファイル操作のあと。作った・移した先の項目)。
   * 開けるのは、親の中身が読み込み済みでフォルダとして載っている祖先だけ。
   */
  reveal(projectId: ProjectId, path: RelPath): void;
  /**
   * フォルダを読み直す(ファイル操作のあと。【仮定】20)。読み込み済みのフォルダだけ。保留中なら積む。
   */
  refresh(projectId: ProjectId, dirs: readonly RelPath[]): void;
  /**
   * 読み込み済みと読み込み中のフォルダをすべて読み直す(隠しファイル表示の切替。Rust は切替で tree-changed を送らない)。
   * それまでに求めていた中身の応答は捨てる(切替の前の除外で読んだものなので)。読み込み済みのものは保留中なら積む。
   */
  reloadAll(projectId: ProjectId): void;
  /** tree-changed の受け口。表示中のプロジェクトの分だけ受ける。 */
  handleTreeChange(change: TreeChange): void;
  /** 写しと uiStore の購読をやめる。以後に届いた応答と変更では状態を変えない。 */
  dispose(): void;
}

/** 保留の理由ごとに、uiStore のどの状態で保留するか(【仮定】18)。 */
const HOLDS: ReadonlyArray<readonly [HoldReason, (ui: UiState) => boolean]> = [
  ["contextMenu", (ui) => ui.treeMenu !== null],
  ["nameInput", (ui) => ui.inlineEdit !== null],
  ["dialog", (ui) => ui.treeConflict !== null || ui.moveConfirmation !== null],
  ["drag", (ui) => ui.treeDragging],
];

/** フォルダの中身を読めなかったときのツリーの知らせ。 */
export const UNREADABLE_DIR_NOTICE = "フォルダを読めませんでした";

/** 写しから、ツリーを出すプロジェクトを決める。 */
function activeProjectOf(app: AppStore): ProjectId | null {
  const project = selectedProject(app.getState().snapshot);
  return project === null || project.missing ? null : project.id;
}

/** 祖先のフォルダ(ルートを除き、上から順)。`a/b/c.ts` → `a`、`a/b`。 */
function ancestorsOf(path: RelPath): RelPath[] {
  const result: RelPath[] = [];
  let parent = parentPath(path);
  while (parent !== null && parent !== "") {
    result.unshift(parent);
    parent = parentPath(parent);
  }
  return result;
}

export function createTreeController({ ipc, tree, app, ui, reportError }: TreeControllerDeps): TreeController {
  let disposed = false;
  let nextRequest = 0;
  /** (プロジェクト, フォルダ) → 最新の問い合わせの番号。応答を受けたら外す。 */
  const latest = new Map<string, number>();
  const keyOf = (projectId: ProjectId, dir: RelPath) => `${projectId}\u0000${dir}`;

  const modelOf = (projectId: ProjectId): TreeModel | undefined => tree.getState().trees.get(projectId);

  const setModel = (projectId: ProjectId, model: TreeModel) => {
    const trees = new Map(tree.getState().trees).set(projectId, model);
    tree.setState({ trees });
  };

  /** 表示中のプロジェクトのモデルを変える。表示中でなければ何もしない。 */
  const updateActive = (projectId: ProjectId, change: (model: TreeModel) => TreeModel) => {
    if (disposed || tree.getState().activeProjectId !== projectId) return;
    const model = modelOf(projectId);
    if (model !== undefined) setModel(projectId, change(model));
  };

  /** 届いた中身を当てる。選んでいた項目が親の新しい中身から消えていれば、選択を外す【仮定】。 */
  const applyListing = (model: TreeModel, dir: RelPath, listing: DirListing): TreeModel => {
    if (listing.kind !== "listed") return forgetDir(model, dir);
    const next = applyDirListing(model, dir, listing.entries);
    const selected = next.selected;
    if (selected === null || selected === "" || parentPath(selected) !== dir || !isLoaded(next, dir)) return next;
    const name = nameOf(selected);
    return listing.entries.some((entry) => entry.name === name) ? next : selectPath(next, null);
  };

  /** フォルダの中身を求め、届いたら当てる(古い応答は捨てる)。 */
  const load = (projectId: ProjectId, dir: RelPath) => {
    const key = keyOf(projectId, dir);
    nextRequest += 1;
    const request = nextRequest;
    latest.set(key, request);
    const isCurrent = () =>
      !disposed && tree.getState().activeProjectId === projectId && latest.get(key) === request;
    ipc.listDirectory(projectId, dir).then(
      (listing) => {
        if (!isCurrent()) return;
        latest.delete(key);
        updateActive(projectId, (model) => applyListing(model, dir, listing));
      },
      (cause: unknown) => {
        if (!isCurrent()) return;
        latest.delete(key);
        reportError(`フォルダの中身を読めなかった: ${describeError(cause)}`);
        // 開いた印のまま空に見せない(権限の無いフォルダなど)。知らせを出してフォルダを閉じる。
        ui.getState().showTreeNotice(UNREADABLE_DIR_NOTICE, projectId);
        updateActive(projectId, (model) => {
          if (!model.loading.has(dir)) return collapseDir(model, dir);
          const loading = new Set(model.loading);
          loading.delete(dir);
          return collapseDir({ ...model, loading }, dir);
        });
      },
    );
  };

  /** 保留を通して読み直す。今すぐ読むものだけを読む。 */
  const offer = (projectId: ProjectId, dirs: readonly RelPath[]) => {
    if (dirs.length === 0) return;
    const { gate, flush } = offerUpdates(tree.getState().gate, dirs);
    tree.setState({ gate });
    for (const dir of flush) load(projectId, dir);
  };

  /** 保留が解けて積んだフォルダを読み直す(そのあいだに忘れたフォルダは読まない)。 */
  const flushPending = (dirs: readonly RelPath[]) => {
    const projectId = tree.getState().activeProjectId;
    if (projectId === null) return;
    const model = modelOf(projectId);
    if (model === undefined) return;
    for (const dir of dirs) if (isLoaded(model, dir)) load(projectId, dir);
  };

  /** 読み込み中のフォルダを今すぐ求め、読み込み済みのフォルダを保留を通して読み直す。 */
  const reloadEverything = (projectId: ProjectId) => {
    const model = modelOf(projectId);
    if (model === undefined) return;
    const loading = [...model.loading];
    for (const dir of loading) load(projectId, dir);
    offer(
      projectId,
      loadedDirs(model).filter((dir) => !model.loading.has(dir)),
    );
  };

  /** 写しに合わせて、表示中のプロジェクトを替える。 */
  const syncActive = () => {
    if (disposed) return;
    const snapshot = app.getState().snapshot;
    const state = tree.getState();
    // 写しから消えたプロジェクトのツリーは忘れる。
    const known = new Set((snapshot?.projects ?? []).map((p) => p.id));
    let trees = state.trees;
    if ([...trees.keys()].some((id) => !known.has(id))) {
      trees = new Map([...trees].filter(([id]) => known.has(id)));
    }
    const active = activeProjectOf(app);
    if (active === state.activeProjectId) {
      if (trees !== state.trees) tree.setState({ trees });
      return;
    }
    const isNew = active !== null && !trees.has(active);
    if (isNew) trees = new Map(trees).set(active, createTreeModel());
    tree.setState({ activeProjectId: active, trees, gate: { ...state.gate, pending: [] } });
    // 前のツリーのメニュー・入力・確認・ドラッグ・フォーカスの印を片付ける(部品が外れても状態は残るため)。
    // 保留の理由はこの変更の購読(syncHolds)で外れる。
    ui.getState().resetTreeUi(active);
    if (active !== null) reloadEverything(active);
  };

  /** uiStore に合わせて保留の理由をそろえる。最後の理由が外れたら積んだフォルダを読み直す。 */
  const syncHolds = () => {
    if (disposed) return;
    const state = ui.getState();
    let gate = tree.getState().gate;
    const flush: RelPath[] = [];
    for (const [reason, holding] of HOLDS) {
      const want = holding(state);
      if (want && !gate.holds.has(reason)) {
        gate = holdUpdates(gate, reason);
      } else if (!want && gate.holds.has(reason)) {
        const released = releaseUpdates(gate, reason);
        gate = released.gate;
        flush.push(...released.flush);
      }
    }
    if (gate !== tree.getState().gate) tree.setState({ gate });
    flushPending([...new Set(flush)]);
  };

  syncHolds();
  syncActive();
  const unsubscribeApp = app.subscribe((state, previous) => {
    if (state.snapshot !== previous.snapshot) syncActive();
  });
  const unsubscribeUi = ui.subscribe(syncHolds);

  /** 表示中のツリーを、読み込みの要らない変更で変える。開いて未読込なら中身を求める。 */
  const withActive = (change: (model: TreeModel) => { model: TreeModel; needsLoad: RelPath | null }) => {
    if (disposed) return;
    const projectId = tree.getState().activeProjectId;
    if (projectId === null) return;
    const model = modelOf(projectId);
    if (model === undefined) return;
    const result = change(model);
    setModel(projectId, result.model);
    if (result.needsLoad !== null) load(projectId, result.needsLoad);
  };

  return {
    expand: (path) =>
      withActive((model) => {
        const result = expandDir(model, path);
        return { model: result.model, needsLoad: result.needsLoad ? path : null };
      }),
    collapse: (path) => withActive((model) => ({ model: collapseDir(model, path), needsLoad: null })),
    toggle: (path) =>
      withActive((model) => {
        const result = toggleDir(model, path);
        return { model: result.model, needsLoad: result.needsLoad ? path : null };
      }),
    collapseAll: () => withActive((model) => ({ model: collapseAll(model), needsLoad: null })),
    select: (path) => withActive((model) => ({ model: selectPath(model, path), needsLoad: null })),
    reveal: (projectId, path) => {
      if (disposed || tree.getState().activeProjectId !== projectId) return;
      for (const dir of ancestorsOf(path)) {
        withActive((model) => {
          const result = expandDir(model, dir);
          return { model: result.model, needsLoad: result.needsLoad ? dir : null };
        });
      }
      withActive((model) => ({ model: selectPath(model, path), needsLoad: null }));
    },
    refresh: (projectId, dirs) => {
      if (disposed || tree.getState().activeProjectId !== projectId) return;
      const model = modelOf(projectId);
      if (model === undefined) return;
      offer(
        projectId,
        dirs.filter((dir) => isLoaded(model, dir)),
      );
    },
    reloadAll: (projectId) => {
      if (disposed || tree.getState().activeProjectId !== projectId) return;
      for (const key of [...latest.keys()]) if (key.startsWith(`${projectId}\u0000`)) latest.delete(key);
      reloadEverything(projectId);
    },
    handleTreeChange: (change) => {
      if (disposed || tree.getState().activeProjectId !== change.projectId) return;
      const model = modelOf(change.projectId);
      if (model === undefined) return;
      offer(change.projectId, dirsToReload(model, change));
    },
    dispose: () => {
      if (disposed) return;
      disposed = true;
      unsubscribeApp();
      unsubscribeUi();
      latest.clear();
    },
  };
}

/** 項目 path を含む(自身か子孫の)選択か。ゴミ箱に入れた項目の選択を外すのに使う。 */
export function selectionWithin(model: TreeModel, path: RelPath): boolean {
  return model.selected !== null && path !== "" && isSameOrDescendant(model.selected, path);
}
