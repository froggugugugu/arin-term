import type { Actions } from "../../state/actions";
import type { ProjectCard } from "../snapshotTypes";
import { TreeHeader } from "../tree/TreeHeader";

interface TreePaneProps {
  /** 選択中のプロジェクト。無ければ null(何も出さない)。 */
  project: ProjectCard | null;
  actions: Pick<Actions, "relinkProject" | "removeProject">;
}

/**
 * 左ペインの下段(M2 の分。計画 M2 の【仮定】16)。プロジェクト色のヘッダと空の領域だけを出す。
 * 選択中のプロジェクトが見つからないときは、ツリーの位置に案内と「再リンク…」「一覧から削除」を出す(設計書 §8「見つからない」)。
 */
export function TreePane({ project, actions }: TreePaneProps) {
  if (project === null) return null;
  return (
    <>
      <TreeHeader name={project.name} color={project.color} />
      <div className="tree-area__body">
        {project.missing && (
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
        )}
      </div>
    </>
  );
}
