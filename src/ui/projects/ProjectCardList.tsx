import "./ProjectCardList.css";

import { type KeyboardEvent, type MouseEvent, useRef, useState } from "react";
import { useStore } from "zustand";

import { buildCardMenu } from "../../logic/menu/cardMenu";
import type { Actions } from "../../state/actions";
import type { AppStore } from "../../state/appStore";
import type { UiStore } from "../../state/uiStore";
import { ContextMenu, type ContextMenuEntry } from "../menu/ContextMenu";
import { PlusIcon } from "../tree/icons";
import { runCardMenuCommand } from "../menu/runCardMenuCommand";
import type { ProjectId } from "../snapshotTypes";
import { ProjectCard } from "./ProjectCard";

/** カード以外の場所の右クリックのメニュー。見出しの + と同じ操作だけを出す。 */
const AREA_MENU: readonly ContextMenuEntry<"add-project">[] = [
  { kind: "command", label: "フォルダを追加…", command: "add-project" },
];

interface ProjectCardListProps {
  store: AppStore;
  ui: UiStore;
  actions: Actions;
}

/**
 * 左ペイン上段(FR-001・FR-008)。小見出し「プロジェクト」と、その右の「フォルダを追加」(+ のアイコンのボタン)、
 * カードの一覧(多いときは一覧だけがスクロールする)。Finder のサイドバーの見出しと項目にならう。
 * プロジェクトが 0 件なら、一覧の代わりに案内と文言のある「フォルダを追加」のボタンを出す(空の状態の主操作。
 * 設計書 §8「プロジェクト 0 件」)。そのときは見出しの + を出さない(同じ名前のボタンを 2 つにしない)。
 * カードは上下の矢印でも移れる。カードの右クリック(または Shift+F10)で、src/logic/menu の buildCardMenu の項目をそのままメニューに出す。
 * カード以外の場所(見出し・一覧の余白・0 件の案内)の右クリックでは「フォルダを追加…」だけのメニューを出す。
 */
export function ProjectCardList({ store, ui, actions }: ProjectCardListProps) {
  const projects = useStore(store, (s) => s.snapshot?.projects ?? null);
  const selectedId = useStore(store, (s) => s.snapshot?.selectedProjectId ?? null);
  const cardMenu = useStore(ui, (s) => s.cardMenu);
  const cardRefs = useRef(new Map<ProjectId, HTMLButtonElement>());
  // カード以外の場所の右クリックのメニュー(開いた位置)。表示だけの状態なので部品の中に持つ。
  const [areaMenu, setAreaMenu] = useState<{ x: number; y: number } | null>(null);

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

  const addProject = () => void actions.addProjectByDialog();

  // カード以外の場所(見出し・一覧の余白・0 件の案内)の右クリック。カードは自分のメニューを開いて既定の動作を止めるので、
  // 止められていなければここで開く。
  const onPaneContextMenu = (event: MouseEvent<HTMLDivElement>) => {
    if (event.defaultPrevented) return;
    event.preventDefault();
    if (ui.getState().cardMenu !== null) ui.getState().closeCardMenu();
    setAreaMenu({ x: event.clientX, y: event.clientY });
  };

  return (
    <div className="card-list-pane" onContextMenu={onPaneContextMenu}>
      <div className="sidebar-section">
        <h2 className="sidebar-section__title">プロジェクト</h2>
        {list.length > 0 && (
          <button
            type="button"
            className="icon-button sidebar-section__action"
            aria-label="フォルダを追加"
            title="フォルダを追加…"
            onClick={addProject}
          >
            <PlusIcon />
          </button>
        )}
      </div>
      {list.length === 0 ? (
        <div className="projects-empty" data-testid="projects-empty">
          <p className="projects-empty__title">プロジェクトがありません</p>
          <p className="projects-empty__hint">フォルダを追加すると、そのフォルダでターミナルが開きます。</p>
          <button type="button" className="button projects-empty__action" onClick={addProject}>
            フォルダを追加
          </button>
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
      {areaMenu !== null && (
        <ContextMenu
          key={`area:${areaMenu.x}:${areaMenu.y}`}
          label="プロジェクト"
          entries={AREA_MENU}
          x={areaMenu.x}
          y={areaMenu.y}
          onClose={() => setAreaMenu(null)}
          onSelect={() => {
            setAreaMenu(null);
            addProject();
          }}
        />
      )}
    </div>
  );
}
