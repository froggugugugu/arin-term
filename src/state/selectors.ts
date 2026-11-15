// 写し(AppSnapshot)から画面に出す値を取り出す関数(設計書 §6.1)。写しは書き換えない。
// 部品(T34c・T34d)はストアから写しを読み、ここの関数で必要な値にする。

import type { AppSnapshot } from "../ipc/generated/AppSnapshot";
import type { ProjectCardSnapshot } from "../ipc/generated/ProjectCardSnapshot";
import type { ProjectId } from "../ipc/generated/ProjectId";
import type { TerminalId } from "../ipc/generated/TerminalId";
import type { TerminalTabSnapshot } from "../ipc/generated/TerminalTabSnapshot";

/** 選択中のプロジェクト。選択が無い・一覧に無い・写しが無いなら null。 */
export function selectedProject(snapshot: AppSnapshot | null): ProjectCardSnapshot | null {
  if (snapshot === null || snapshot.selectedProjectId === null) return null;
  return snapshot.projects.find((p) => p.id === snapshot.selectedProjectId) ?? null;
}

/** プロジェクトのタブ(タブの順)。写しのターミナルの一覧に無い ID は飛ばす。 */
export function projectTabs(snapshot: AppSnapshot | null, projectId: ProjectId): TerminalTabSnapshot[] {
  const project = snapshot?.projects.find((p) => p.id === projectId);
  if (snapshot === null || project === undefined) return [];
  const byId = new Map(snapshot.terminals.map((t) => [t.id, t]));
  return project.terminalIds.flatMap((id) => byId.get(id) ?? []);
}

/** 表示中のターミナル(選択中のプロジェクトの表示中のタブ)。タブが 0 枚・写しに無いなら null。 */
export function shownTerminal(snapshot: AppSnapshot | null): TerminalTabSnapshot | null {
  const activeId = selectedProject(snapshot)?.activeTerminalId ?? null;
  if (snapshot === null || activeId === null) return null;
  return snapshot.terminals.find((t) => t.id === activeId) ?? null;
}

/** 表示中の 1 枚と、隠して出力の取り込みだけを続けるターミナル(全プロジェクトの残り。写しの順)。 */
export interface Presentation {
  readonly shown: TerminalId | null;
  readonly background: readonly TerminalId[];
}

/** 写しから、表示中のターミナルと隠すターミナルを決める(FR-042)。 */
export function presentation(snapshot: AppSnapshot | null): Presentation {
  const shown = shownTerminal(snapshot)?.id ?? null;
  const background = (snapshot?.terminals ?? []).map((t) => t.id).filter((id) => id !== shown);
  return { shown, background };
}
