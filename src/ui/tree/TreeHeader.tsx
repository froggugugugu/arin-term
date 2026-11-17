import { projectColorStyle } from "../design/palette";
import type { ProjectColor } from "../snapshotTypes";

/** ヘッダをルートの行として選べるようにするときの値(ツリーの中に置くとき)。 */
export interface TreeHeaderRoot {
  /** 要素の ID(ツリーの aria-activedescendant が指す)。 */
  id: string;
  selected: boolean;
  /** クリックでルートを選ぶ。 */
  onSelect: () => void;
  /** ドラッグ中で、ルートへ落とせる(T43d)。 */
  dropActive?: boolean;
}

interface TreeHeaderProps {
  name: string;
  color: ProjectColor;
  /** ルートの行として扱う(計画【仮定】1)。省略すると見出しだけ(「見つからない」のとき)。 */
  root?: TreeHeaderRoot;
}

/**
 * ツリーのヘッダ(左ペイン下段の上端。Presentational)。選択中のプロジェクトの名前の前に、カードと同じプロジェクト色の
 * 小さな丸い印を出す(FR-060。地は塗らない)。
 * ツリーを出しているときはルートの行(WAI-ARIA の treeitem、1 段目、常に開いている)になり、クリックでルートが選ばれる
 * (計画【仮定】1)。選択はツリーの行と同じ、内側の角の丸い塗りで示す。
 * 右クリック(フォルダの上のメニュー。名称変更・切り取り・ゴミ箱は無効)はツリー(FileTreeView)が受ける。
 * ドラッグ中にルートへ落とせるときは data-drop-active で強調する(TreeDrag.css)。
 */
export function TreeHeader({ name, color, root }: TreeHeaderProps) {
  if (root === undefined) {
    return (
      <h2 className="tree-header project-colored" data-testid="tree-header" style={projectColorStyle(color)}>
        <span className="project-dot" aria-hidden="true" />
        <span className="tree-header__label">{name}</span>
      </h2>
    );
  }
  const classes = ["tree-header", "tree-header--root", "project-colored"];
  if (root.selected) classes.push("tree-header--selected");
  if (root.dropActive === true) classes.push("tree-header--drop-target");
  return (
    <div
      id={root.id}
      role="treeitem"
      className={classes.join(" ")}
      data-testid="tree-header"
      aria-level={1}
      aria-expanded={true}
      aria-selected={root.selected}
      data-drop-active={root.dropActive === true ? "true" : undefined}
      title={name}
      style={projectColorStyle(color)}
      onClick={root.onSelect}
    >
      <span className="project-dot" aria-hidden="true" />
      <span className="tree-header__label">{name}</span>
    </div>
  );
}
