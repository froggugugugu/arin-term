// 画面の操作から Rust のコマンドを呼ぶ包み(設計書 §5.2・§6.1 の actions)。部品(T34c・T34d)はここだけを呼ぶ。
// 結果は状態の写しで届くので、ここでは写しを書き換えない。確認が要る操作は uiStore の確認を開き、確定で呼び直す。
// コマンドの失敗は reportError(診断。標準エラーの [arin:frontend-error])に 1 行で報告し、Promise は失敗させない。
// モジュールの読み込み時には作らない。main.tsx が生成関数で作り、props で渡す(設計書 §11)。

import type { ArinIpc } from "../ipc/client";
import type { ProjectColor } from "../ipc/generated/ProjectColor";
import type { ProjectId } from "../ipc/generated/ProjectId";
import type { TerminalId } from "../ipc/generated/TerminalId";
import { describeError } from "../logic/describeError";
import type { UiStore } from "./uiStore";

/** actions が使う Rust の操作(ArinIpc の一部)。 */
export type ActionsBackend = Pick<
  ArinIpc,
  | "addProjectByDialog"
  | "selectProject"
  | "removeProject"
  | "relinkProject"
  | "setProjectColor"
  | "copyProjectPath"
  | "newTerminal"
  | "closeTerminal"
  | "selectTerminal"
  | "setPaneSizes"
  | "setSidebarCollapsed"
  | "confirmQuit"
>;

export interface ActionsDeps {
  ipc: ActionsBackend;
  ui: UiStore;
  /** 失敗の報告先(ErrorReporter.report)。 */
  reportError: (message: string) => void;
}

/** 画面の操作。どの Promise も失敗しない(失敗は報告済み)。 */
export interface Actions {
  /** フォルダ選択のダイアログ(Rust が開く)で選んだフォルダを登録する(FR-001)。 */
  addProjectByDialog(): Promise<void>;
  /** プロジェクトを選ぶ(FR-002)。 */
  selectProject(id: ProjectId): Promise<void>;
  /** プロジェクトを一覧から削除する(FR-003)。動作中があれば削除の確認を開く(削除はまだしない)。 */
  removeProject(id: ProjectId): Promise<void>;
  /** 開いている削除の確認で「削除」を選んだ。確認を閉じ、確認済みで削除する。確認が無ければ何もしない。 */
  confirmRemoveProject(): Promise<void>;
  /** 開いている削除の確認で「中止」を選んだ。確認を閉じるだけ。 */
  cancelRemoveProject(): void;
  /** 見つからないプロジェクトをつなぎ直す(FR-007。ダイアログは Rust が開く)。 */
  relinkProject(id: ProjectId): Promise<void>;
  /** プロジェクトの色を変える(FR-059)。 */
  setProjectColor(id: ProjectId, color: ProjectColor): Promise<void>;
  /** ルートの絶対パスをクリップボードに入れる(FR-064。選択は変えない)。 */
  copyProjectPath(id: ProjectId): Promise<void>;
  /** 選択中のプロジェクトに新しいタブを開く(FR-044。タブ 0 枚の「新規ターミナル」)。 */
  newTerminal(): Promise<void>;
  /** タブを閉じる。 */
  closeTerminal(id: TerminalId): Promise<void>;
  /** タブを表示中にする。 */
  selectTerminal(id: TerminalId): Promise<void>;
  /** 左ペインの幅とカード領域の高さを送る(FR-009。Rust が最小寸法で直して写しで返す)。 */
  setPaneSizes(sidebarWidth: number, cardAreaHeight: number): Promise<void>;
  /** 左ペインを折りたたむ・戻す(FR-009)。 */
  setSidebarCollapsed(collapsed: boolean): Promise<void>;
  /** 開いている終了の確認で「終了」を選んだ。確認を閉じて終了を確定する(FR-063)。確認が無ければ何もしない。 */
  confirmQuit(): Promise<void>;
  /** 開いている終了の確認で「取り消し」を選んだ。確認を閉じるだけ(Rust には何も送らない)。 */
  cancelQuit(): void;
}

export function createActions({ ipc, ui, reportError }: ActionsDeps): Actions {
  /** コマンドを呼び、失敗は what を付けて報告する。 */
  const run = async (what: string, request: () => Promise<unknown>): Promise<void> => {
    try {
      await request();
    } catch (cause) {
      reportError(`${what}: ${describeError(cause)}`);
    }
  };

  const remove = (id: ProjectId, confirmed: boolean) =>
    run("プロジェクトを一覧から削除できなかった", async () => {
      const outcome = await ipc.removeProject(id, confirmed);
      if (outcome.kind === "needsConfirmation") ui.getState().openRemoveConfirmation(id, outcome.running);
    });

  return {
    addProjectByDialog: () => run("フォルダを登録できなかった", () => ipc.addProjectByDialog()),
    selectProject: (id) => run("プロジェクトを選べなかった", () => ipc.selectProject(id)),
    removeProject: (id) => remove(id, false),
    confirmRemoveProject: async () => {
      const pending = ui.getState().removeConfirmation;
      if (pending === null) return;
      ui.getState().closeRemoveConfirmation();
      await remove(pending.projectId, true);
    },
    cancelRemoveProject: () => ui.getState().closeRemoveConfirmation(),
    relinkProject: (id) => run("フォルダをつなぎ直せなかった", () => ipc.relinkProject(id)),
    setProjectColor: (id, color) => run("プロジェクトの色を変えられなかった", () => ipc.setProjectColor(id, color)),
    copyProjectPath: (id) => run("パスをコピーできなかった", () => ipc.copyProjectPath(id)),
    newTerminal: () => run("新しいターミナルを開けなかった", () => ipc.newTerminal()),
    closeTerminal: (id) => run("ターミナルを閉じられなかった", () => ipc.closeTerminal(id)),
    selectTerminal: (id) => run("ターミナルを選べなかった", () => ipc.selectTerminal(id)),
    setPaneSizes: (sidebarWidth, cardAreaHeight) =>
      run("ペインの寸法を保存できなかった", () => ipc.setPaneSizes(sidebarWidth, cardAreaHeight)),
    setSidebarCollapsed: (collapsed) =>
      run("左ペインを折りたためなかった", () => ipc.setSidebarCollapsed(collapsed)),
    confirmQuit: async () => {
      if (ui.getState().quitConfirmation === null) return;
      ui.getState().closeQuitConfirmation();
      await run("アプリを終了できなかった", () => ipc.confirmQuit());
    },
    cancelQuit: () => ui.getState().closeQuitConfirmation(),
  };
}
