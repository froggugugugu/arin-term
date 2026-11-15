import "./ProjectCardList.css";

import { type KeyboardEvent, useRef } from "react";
import { useStore } from "zustand";

import { buildCardMenu } from "../../logic/menu/cardMenu";
import type { Actions } from "../../state/actions";
import type { AppStore } from "../../state/appStore";
import type { UiStore } from "../../state/uiStore";
import { ContextMenu } from "../menu/ContextMenu";
import { runCardMenuCommand } from "../menu/runCardMenuCommand";
import type { ProjectId } from "../snapshotTypes";
import { ProjectCard } from "./ProjectCard";

interface ProjectCardListProps {
  store: AppStore;
  ui: UiStore;
  actions: Actions;
}

/**
 * 左ペイン上段(FR-001・FR-008)。カードの一覧(多いときは一覧だけがスクロールする)と「フォルダを追加」のボタン。
 * プロジェクトが 0 件なら案内を出す(設計書 §8「プロジェクト 0 件」)。
 * カードは上下の矢印でも移れる。カードの右クリック(または Shift+F10)で、src/logic/menu の buildCardMenu の項目をそのままメニューに出す。
 */
export function ProjectCardList({ store, ui, actions }: ProjectCardListProps) {
  const projects = useStore(store, (s) => s.snapshot?.projects ?? null);
  const selectedId = useStore(store, (s) => s.snapshot?.selectedProjectId ?? null);
  const cardMenu = useStore(ui, (s) => s.cardMenu);
  const cardRefs = useRef(new Map<ProjectId, HTMLButtonElement>());

  const list = projects ?? [];
  const menuProject = cardMenu === null ? undefined : list.find((p) => p.id === cardMenu.projectId);

  // 上下の矢印でカードを移る(macOS の既定では Tab がボタンに止まらないため。端では止まる)。
  const onListKeyDown = (event: KeyboardEvent<HTMLUListElement>) => {
    const step = event.key === "ArrowDown" ? 1 : event.key === "ArrowUp" ? -1 : 0;
    if (step === 0) return;
    const ids = list.map((p) => p.id);
    const current = ids.findIndex((id) => cardRefs.current.get(id) === document.activeElement);
    if (current < 0) return;
    event.preventDefault();
    const next = ids[Math.min(ids.length - 1, Math.max(0, current + step))];
    if (next !== undefined) cardRefs.current.get(next)?.focus();
  };

  const closeMenu = (restoreFocus: boolean) => {
    const id = ui.getState().cardMenu?.projectId;
    ui.getState().closeCardMenu();
    if (restoreFocus && id !== undefined) cardRefs.current.get(id)?.focus();
  };

  return (
    <div className="card-list-pane">
      {list.length === 0 ? (
        <div className="projects-empty" data-testid="projects-empty">
          <p className="projects-empty__title">プロジェクトがありません</p>
          <p className="projects-empty__hint">フォルダを追加すると、そのフォルダでターミナルが開きます。</p>
        </div>
      ) : (
        <ul className="card-list" data-testid="card-list" aria-label="プロジェクト一覧" onKeyDown={onListKeyDown}>
          {list.map((project) => {
            const selected = project.id === selectedId;
            return (
              <li key={project.id} className="card-list__item">
                <ProjectCard
                  project={project}
                  selected={selected}
                  buttonRef={(el) => {
                    if (el === null) cardRefs.current.delete(project.id);
                    else cardRefs.current.set(project.id, el);
                  }}
                  onSelect={() => {
                    if (!selected) void actions.selectProject(project.id);
                  }}
                  onOpenMenu={(position) => ui.getState().openCardMenu(project.id, position)}
                />
              </li>
            );
          })}
        </ul>
      )}
      <div className="card-list-pane__footer">
        <button type="button" className="button add-project" onClick={() => void actions.addProjectByDialog()}>
          <span aria-hidden="true">+</span> フォルダを追加
        </button>
      </div>
      {cardMenu !== null && menuProject !== undefined && (
        <ContextMenu
          key={`${cardMenu.projectId}:${cardMenu.x}:${cardMenu.y}`}
          label={menuProject.name}
          entries={buildCardMenu({ missing: menuProject.missing, color: menuProject.color })}
          x={cardMenu.x}
          y={cardMenu.y}
          onClose={closeMenu}
          onSelect={(command) => {
            closeMenu(true);
            void runCardMenuCommand(actions, menuProject.id, command);
          }}
        />
      )}
    </div>
  );
}
