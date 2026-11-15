import type { KeyboardEvent, MouseEvent, Ref } from "react";

import { projectColorStyle } from "../design/palette";
import type { ProjectCard as ProjectCardValue } from "../snapshotTypes";

interface ProjectCardProps {
  project: ProjectCardValue;
  selected: boolean;
  /** クリック・Enter・Space。 */
  onSelect: () => void;
  /** 右クリック・Shift+F10 で、メニューを開く位置(画面の CSS ピクセル)。 */
  onOpenMenu: (position: { x: number; y: number }) => void;
  buttonRef?: Ref<HTMLButtonElement>;
}

/**
 * プロジェクトのカード 1 枚(FR-008。1 行)。macOS のサイドバーの項目にならい、プロジェクト色の小さな丸い印と名前を並べる
 * (FR-060。印の色はツリーのヘッダ・タブの印と同じ)。
 * 表示用のパス(`~` 省略)はツールチップに出し、カードには常時出さない(C-03・C-25)。
 * 選択中は aria-current と角の丸い淡い塗り・太字で、見つからないは「見つかりません」の文字で示す(色だけに頼らない)。
 * 実行中の印(FR-012)とブランチ名(FR-011)は M6。
 */
export function ProjectCard({ project, selected, onSelect, onOpenMenu, buttonRef }: ProjectCardProps) {
  const onContextMenu = (event: MouseEvent<HTMLButtonElement>) => {
    event.preventDefault();
    onOpenMenu({ x: event.clientX, y: event.clientY });
  };
  const onKeyDown = (event: KeyboardEvent<HTMLButtonElement>) => {
    if ((event.key === "F10" && event.shiftKey) || event.key === "ContextMenu") {
      event.preventDefault();
      const rect = event.currentTarget.getBoundingClientRect();
      onOpenMenu({ x: rect.left + 8, y: rect.bottom });
    }
  };
  const classes = ["project-card", "project-colored"];
  if (selected) classes.push("project-card--selected");
  if (project.missing) classes.push("project-card--missing");

  return (
    <button
      ref={buttonRef}
      type="button"
      className={classes.join(" ")}
      data-testid="project-card"
      data-project-id={project.id}
      aria-current={selected ? "true" : undefined}
      title={project.displayPath}
      style={projectColorStyle(project.color)}
      onClick={onSelect}
      onContextMenu={onContextMenu}
      onKeyDown={onKeyDown}
    >
      <span className="project-dot" aria-hidden="true" />
      <span className="project-card__name">{project.name}</span>
      {project.missing && <span className="project-card__badge">見つかりません</span>}
    </button>
  );
}
