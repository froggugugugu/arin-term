import { useStore } from "zustand";

import type { Actions } from "../../state/actions";
import type { TreeStore } from "../../state/treeStore";
import type { UiStore } from "../../state/uiStore";
import { ConflictDialog } from "../dialogs/ConflictDialog";
import { MoveConfirmDialog } from "../dialogs/MoveConfirmDialog";
import type { TreeMenuActions } from "../menu/runTreeMenuCommand";
import type { ProjectCard } from "../snapshotTypes";
import { FileTreeView } from "../tree/FileTreeView";
import { CloseIcon, CollapseAllIcon, EyeIcon } from "../tree/icons";
import { TreeHeader } from "../tree/TreeHeader";

interface TreePaneProps {
  /** 選択中のプロジェクト。無ければ null(何も出さない。カード一覧が「プロジェクトがありません」を出す)。 */
  project: ProjectCard | null;
  tree: TreeStore;
  ui: UiStore;
  actions: TreeMenuActions &
    Pick<
      Actions,
      | "relinkProject"
      | "removeProject"
      | "selectTreePath"
      | "toggleDir"
      | "expandDir"
      | "collapseDir"
      | "commitInlineEdit"
      | "resolveConflict"
      | "cancelConflict"
      | "dropEntry"
      | "confirmMove"
      | "cancelMove"
      | "focusTile"
    >;
}

/**
 * 左ペインの下段(設計書 §9.1)。選択中のプロジェクトのファイルツリー(FileTreeView)と、下端の小さな道具の帯
 * (すべて折りたたむ・隠しファイルを表示。Xcode のナビゲータの下端の帯にならう)、ツリーの知らせ(1 行)を出す。
 * ファイル操作の名前の衝突の確認(ConflictDialog。モーダル。T43c)と、ドロップでの移動の確認(MoveConfirmDialog。T43d)もここに置く。
 * 選択中のプロジェクトが見つからないときは、ツリーの代わりに案内と「再リンク…」「一覧から削除」を出す(設計書 §8「見つからない」)。
 */
export function TreePane({ project, tree, ui, actions }: TreePaneProps) {
  if (project === null) return null;
  if (project.missing) {
    return (
      <>
        <TreeHeader name={project.name} color={project.color} />
        <div className="tree-area__body">
          <div className="tree-missing" data-testid="tree-missing">
            <p className="tree-missing__title">フォルダが見つかりません</p>
            <p className="tree-missing__path">{project.displayPath}</p>
            <div className="tree-missing__actions">
              <button type="button" className="button" onClick={() => void actions.relinkProject(project.id)}>
                再リンク…
              </button>
              <button type="button" className="button" onClick={() => void actions.removeProject(project.id)}>
                一覧から削除
              </button>
            </div>
          </div>
        </div>
      </>
    );
  }
  return (
    <>
      <FileTreeView key={project.id} project={project} tree={tree} ui={ui} actions={actions} />
      <TreeNotice ui={ui} />
      <MoveConfirmDialog ui={ui} actions={actions} rootName={project.name} />
      <ConflictDialog ui={ui} actions={actions} />
      <div className="tree-toolbar" data-testid="tree-toolbar">
        <button
          type="button"
          className="tree-toolbar__button"
          aria-label="すべて折りたたむ"
          title="すべて折りたたむ"
          onClick={() => actions.collapseAllDirs()}
        >
          <CollapseAllIcon />
        </button>
        <button
          type="button"
          className="tree-toolbar__button"
          aria-label="隠しファイルを表示"
          aria-pressed={project.showHidden}
          title={project.showHidden ? "隠しファイルを隠す" : "隠しファイルを表示"}
          onClick={() => void actions.setShowHidden(project.id, !project.showHidden)}
        >
          <EyeIcon />
        </button>
      </div>
    </>
  );
}

/** ツリーの知らせ(操作の結果の 1 行。「見つかりません」など)。閉じるまで出す。 */
function TreeNotice({ ui }: { ui: UiStore }) {
  const notice = useStore(ui, (s) => s.treeNotice);
  if (notice === null) return null;
  return (
    <div className="tree-notice" role="status" data-testid="tree-notice">
      <p className="tree-notice__text" title={notice.message}>
        {notice.message}
      </p>
      <button
        type="button"
        className="tree-notice__close"
        aria-label="知らせを閉じる"
        onClick={() => ui.getState().dismissTreeNotice()}
      >
        <CloseIcon />
      </button>
    </div>
  );
}
