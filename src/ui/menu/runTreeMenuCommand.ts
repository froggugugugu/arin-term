// ツリーの右クリックのメニュー・ショートカット・コピー/切り取り/ペーストで選ばれた操作を、画面の操作に渡す(計画 T43c)。
//
// 3 つの入口(メニューの項目、ツリーにフォーカスがあるときの keydown、文書のコピー・切り取り・ペーストのイベント)は、
// どれも src/logic/menu/treeMenu.ts の項目を通す(キーとイベントは treeShortcutCommand でその場所のメニューの同じ項目を引く)。
// そのため有効・無効(ルートでの名称変更・切り取り・ゴミ箱など。FR-035)はメニューとショートカットでそろう。
// キーの文字列はここにも書かない(表から引くのは src/logic/keys。前提 (e))。

import type { ClipboardFocus } from "../../logic/keys/clipboard";
import type { ShortcutCommandId, ShortcutTable } from "../../logic/keys/shortcutTable";
import {
  type TreeMenuCommand,
  type TreeMenuContext,
  type TreeMenuTarget,
  treeShortcutCommand,
} from "../../logic/menu/treeMenu";
import type { Actions } from "../../state/actions";
import type { AppStore } from "../../state/appStore";
import { selectedProject, selectedTreeTarget, treeSelectionKind } from "../../state/selectors";
import type { TreeStore } from "../../state/treeStore";
import type { UiStore } from "../../state/uiStore";
import type { ProjectCard } from "../snapshotTypes";
import type { TreeClipboardAction, TreeClipboardTarget } from "../terminals/clipboardRouting";

/** ツリーのメニューが使う操作(Actions の一部)。 */
export type TreeMenuActions = Pick<
  Actions,
  | "openEntry"
  | "expandDir"
  | "revealEntry"
  | "insertEntryPath"
  | "copyEntryPath"
  | "cutEntries"
  | "copyEntries"
  | "pasteEntries"
  | "trashEntry"
  | "setShowHidden"
  | "collapseAllDirs"
>;

/** 操作を呼ぶのに要る、プロジェクトの値。 */
export interface TreeCommandScope {
  readonly projectId: ProjectCard["id"];
  /** 「パスをターミナルに挿入」の宛先(表示中のタブ。計画【仮定】16)。無ければ null。 */
  readonly terminalId: ProjectCard["activeTerminalId"];
  /** 隠しファイルを表示しているか(切替の項目で反対の値にする)。 */
  readonly showHidden: boolean;
  /** メニューを開いた場所(「開く」をファイルとフォルダで分ける)。 */
  readonly target: TreeMenuTarget;
}

/**
 * メニューの組み立て(buildTreeMenu)に渡す画面の値を、プロジェクトから作る。
 * 【仮定】表示中のタブの有無は写しのプロジェクトの activeTerminalId で決める(selectors.treeMenuContext と同じ意味。
 * ツリーの部品は写し全体を受け取らないため)。
 */
export function treeMenuContextOf(project: ProjectCard, shortcuts: ShortcutTable): TreeMenuContext {
  return { hasTab: project.activeTerminalId !== null, showHidden: project.showHidden, shortcuts };
}

/** 操作を呼ぶのに要るプロジェクトの値。 */
export function treeCommandScope(project: ProjectCard, target: TreeMenuTarget): TreeCommandScope {
  return { projectId: project.id, terminalId: project.activeTerminalId, showHidden: project.showHidden, target };
}

/**
 * 選ばれた項目の操作を呼ぶ。どの操作も失敗しない(actions が報告する)。
 * - 開く: ファイルは既定のアプリで開き、フォルダは開く(画面が開閉する。ルートは常に開いているので何もしない)
 * - 名称変更・新規ファイル・新規フォルダ: インライン編集を始める(確定は入力欄が actions.commitInlineEdit を呼ぶ)
 */
export function runTreeMenuCommand(
  actions: TreeMenuActions,
  ui: UiStore,
  scope: TreeCommandScope,
  command: TreeMenuCommand,
): Promise<void> {
  const { projectId } = scope;
  switch (command.type) {
    case "open":
      if (scope.target.kind === "folder") actions.expandDir(command.path);
      else if (scope.target.kind === "file") return actions.openEntry(projectId, command.path);
      return Promise.resolve();
    case "reveal":
      return actions.revealEntry(projectId, command.path);
    case "insert-path":
      if (scope.terminalId === null) return Promise.resolve();
      return actions.insertEntryPath(projectId, [command.path], scope.terminalId);
    case "copy-path":
      return actions.copyEntryPath(projectId, command.path, command.style);
    case "cut":
      return actions.cutEntries(projectId, [command.path]);
    case "copy":
      return actions.copyEntries(projectId, [command.path]);
    case "paste":
      return actions.pasteEntries(projectId, command.destination);
    case "rename":
      ui.getState().startInlineEdit({ mode: "rename", projectId, path: command.path });
      return Promise.resolve();
    case "trash":
      return actions.trashEntry(projectId, command.path);
    case "new-file":
      ui.getState().startInlineEdit({ mode: "create", projectId, parent: command.parent, kind: "file" });
      return Promise.resolve();
    case "new-folder":
      ui.getState().startInlineEdit({ mode: "create", projectId, parent: command.parent, kind: "folder" });
      return Promise.resolve();
    case "toggle-hidden":
      return actions.setShowHidden(projectId, !scope.showHidden);
    case "collapse-all":
      actions.collapseAllDirs();
      return Promise.resolve();
  }
}

/** ショートカット・クリップボードの入口が使う状態と操作。 */
export interface TreeCommandDeps {
  actions: TreeMenuActions;
  ui: UiStore;
  tree: TreeStore;
}

/**
 * ショートカットのコマンドの ID を、選んでいる項目の上のメニューの同じ項目として実行する(FR-024)。
 * 何も選んでいなければ余白(=ルート)のメニューから引く(⌘N はルートに作る)。
 * その場所のメニューに無い・無効の項目なら何もせず false。
 */
export function runTreeShortcut(deps: TreeCommandDeps, project: ProjectCard, id: ShortcutCommandId): boolean {
  const target = selectedTreeTarget(deps.tree.getState()) ?? { kind: "blank" };
  const command = treeShortcutCommand(id, target, treeMenuContextOf(project, deps.ui.getState().shortcuts));
  if (command === null) return false;
  void runTreeMenuCommand(deps.actions, deps.ui, treeCommandScope(project, target), command);
  return true;
}

/** ツリーのコピー・切り取り・ペーストを、表のコマンドの ID にする(受け口は「編集」の項目)。 */
const CLIPBOARD_COMMANDS: Readonly<Record<TreeClipboardAction, ShortcutCommandId>> = {
  "tree-copy": "edit.copy",
  "tree-cut": "edit.cut",
  "tree-paste": "edit.paste",
};

export interface TreeClipboardDeps extends TreeCommandDeps {
  /** 写し(選択中のプロジェクトを読む)。 */
  app: AppStore;
  /** フォーカスがツリーの名前の入力欄にあるか。 */
  isNameInputFocused: () => boolean;
  /**
   * いま実際にフォーカスがツリーの中にあるか(本物は document.activeElement で見る)。uiStore.treeFocused の印は、
   * フォーカスのある要素が取り除かれたときに WebKit が blur を出さず残ることがあるため、振り分けの時点で確かめ直す。
   */
  isTreeElementFocused: () => boolean;
}

/**
 * 文書のコピー・切り取り・ペーストのイベントの入口(clipboardRouting.ts)に渡す、ツリーの受け口。
 * 名前の入力中は name-input(入力欄の文字の操作に任せる)、ツリーにフォーカスがあれば(印と実際のフォーカスの両方)tree、
 * どちらでもなければ null
 * (ターミナルの振り分けに任せる)。
 */
export function createTreeClipboard(deps: TreeClipboardDeps): TreeClipboardTarget {
  return {
    focus(): ClipboardFocus | null {
      if (deps.isNameInputFocused()) return { kind: "name-input" };
      // 印と実際のフォーカスの両方がツリーにあるときだけ。印が残っていても、フォーカスがターミナルならターミナルの操作。
      if (!deps.ui.getState().treeFocused || !deps.isTreeElementFocused()) return null;
      return { kind: "tree", selection: treeSelectionKind(deps.tree.getState()) };
    },
    run(action) {
      const project = selectedProject(deps.app.getState().snapshot);
      if (project === null || project.missing) return;
      runTreeShortcut(deps, project, CLIPBOARD_COMMANDS[action]);
    },
  };
}
