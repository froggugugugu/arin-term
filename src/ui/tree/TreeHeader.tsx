import { projectColorStyle } from "../design/palette";
import type { ProjectColor } from "../snapshotTypes";

/** ヘッダをルートの行として選べるようにするときの値(ツリーの中に置くとき)。 */
export interface TreeHeaderRoot {
  /** 要素の ID(ツリーの aria-activedescendant が指す)。 */
  id: string;
  selected: boolean;
  /** クリックでルートを選ぶ。 */
  onSelect: () => void;
}

interface TreeHeaderProps {
  name: string;
  color: ProjectColor;
  /** ルートの行として扱う(計画【仮定】1)。省略すると見出しだけ(「見つからない」のとき)。 */
  root?: TreeHeaderRoot;
}

/**
 * ツリーのヘッダ(左ペイン下段の上端。Presentational)。選択中のプロジェクトの名前を、カードのタイトル部と同じ
 * プロジェクト色で出す(FR-060。色は M2 のまま)。
 * ツリーを出しているときはルートの行(WAI-ARIA の treeitem、1 段目、常に開いている)になり、クリックでルートが選ばれる
 * (計画【仮定】1)。選択は内側の角の丸い塗りで示す(地のプロジェクト色は変えない)。
 * T43c はここに右クリック(フォルダの上のメニュー。名称変更・切り取り・ゴミ箱は無効)を足す。
 */
export function TreeHeader({ name, color, root }: TreeHeaderProps) {
  if (root === undefined) {
    return (
      <h2 className="tree-header project-tinted" data-testid="tree-header" style={projectColorStyle(color)}>
        {name}
      </h2>
    );
  }
  const classes = ["tree-header", "tree-header--root", "project-tinted"];
  if (root.selected) classes.push("tree-header--selected");
  return (
    <div
      id={root.id}
      role="treeitem"
      className={classes.join(" ")}
      data-testid="tree-header"
      aria-level={1}
      aria-expanded={true}
      aria-selected={root.selected}
      title={name}
      style={projectColorStyle(color)}
      onClick={root.onSelect}
    >
      <span className="tree-header__label">{name}</span>
    </div>
  );
}
