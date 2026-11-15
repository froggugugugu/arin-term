import { projectColorStyle } from "../design/palette";
import type { ProjectColor } from "../snapshotTypes";

interface TreeHeaderProps {
  name: string;
  color: ProjectColor;
}

/**
 * ツリーのヘッダ(左ペイン下段の上端)。選択中のプロジェクトの名前を、カードのタイトル部と同じプロジェクト色で出す(FR-060)。
 * ツリーの中身は M3(計画 M2 の【仮定】16)。
 */
export function TreeHeader({ name, color }: TreeHeaderProps) {
  return (
    <h2 className="tree-header project-tinted" data-testid="tree-header" style={projectColorStyle(color)}>
      {name}
    </h2>
  );
}
