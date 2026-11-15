// 画面の操作から Rust のコマンドを呼ぶ包み(設計書 §5.2・§6.1 の actions)。部品(T34c・T34d)はここだけを呼ぶ。
// 結果は状態の写しで届くので、ここでは写しを書き換えない。確認が要る操作は uiStore の確認を開き、確定で呼び直す。
// コマンドの失敗は reportError(診断。標準エラーの [arin:frontend-error])に 1 行で報告し、Promise は失敗させない。
// モジュールの読み込み時には作らない。main.tsx が生成関数で作り、props で渡す(設計書 §11)。
//
// ツリー(M3。T43a): 開閉・選択は TreeController(./treeStore.ts)へ渡す。ファイル操作の応答(FileOpOutcome)は次のように扱う
// (設計書 §7.1「ファイル操作」、計画【仮定】20):
// - done: 関わるフォルダを読み直し、select があればその項目を選ぶ(祖先を開く)
// - notFound: 「見つかりません」の知らせを出し、対象の親を読み直す(FR-036、AC-T24)
// - conflict: 衝突の確認(uiStore.treeConflict)を開く。resolveConflict で対処を付けて呼び直し、cancelConflict では呼ばない
// - invalidName・notAllowed・failed: 1 行の知らせを出す(文言に絶対パスを作らない。failed は Rust の文言をそのまま)

import type { ArinIpc } from "../ipc/client";
import type { ConflictResolution } from "../ipc/generated/ConflictResolution";
import type { FileOpOutcome } from "../ipc/generated/FileOpOutcome";
import type { InvalidNameReason } from "../ipc/generated/InvalidNameReason";
import type { NewEntryKind } from "../ipc/generated/NewEntryKind";
import type { NotAllowedReason } from "../ipc/generated/NotAllowedReason";
import type { PathKind } from "../ipc/generated/PathKind";
import type { ProjectColor } from "../ipc/generated/ProjectColor";
import type { ProjectId } from "../ipc/generated/ProjectId";
import type { RelPath } from "../ipc/generated/RelPath";
import type { TerminalId } from "../ipc/generated/TerminalId";
import { describeError } from "../logic/describeError";
import { nameOf, parentPath } from "../logic/tree/treePath";
import { selectionWithin, type TreeController, type TreeStore } from "./treeStore";
import type { RetryableFileOp, UiStore } from "./uiStore";

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
  | "setShowHidden"
  | "createEntry"
  | "renameEntry"
  | "moveEntry"
  | "copyEntry"
  | "trashEntry"
  | "copyEntries"
  | "cutEntries"
  | "pasteEntries"
  | "revealEntry"
  | "openEntry"
  | "copyEntryPath"
  | "insertEntryPath"
>;

export interface ActionsDeps {
  ipc: ActionsBackend;
  ui: UiStore;
  /** ツリーの状態(ゴミ箱に入れた項目の選択を外すのに読む)。 */
  tree: TreeStore;
  /** ツリーの開閉・選択・読み直し。 */
  treeController: TreeController;
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

  // ── ツリー(M3)。開閉・選択は表示中のプロジェクトのツリーに効く ──
  /** フォルダを開く(未読込なら中身を求める。FR-013)。 */
  expandDir(path: RelPath): void;
  /** フォルダを閉じる。 */
  collapseDir(path: RelPath): void;
  /** 開閉を切り替える(開閉の印のクリック、フォルダのダブルクリック)。 */
  toggleDir(path: RelPath): void;
  /** すべて折りたたむ。 */
  collapseAllDirs(): void;
  /** 項目を選ぶ(空の文字列はルート、null は選択を外す)。 */
  selectTreePath(path: RelPath | null): void;
  /** 隠しファイル表示を切り替え、済んだら読み込み済みのフォルダを読み直す(FR-017)。 */
  setShowHidden(projectId: ProjectId, showHidden: boolean): Promise<void>;
  /**
   * 開いているインライン編集を name で確定する(新規作成か名称変更)。入力を閉じてからコマンドを呼ぶ。
   * 編集が無ければ何もしない。取り消しは uiStore.closeInlineEdit。
   */
  commitInlineEdit(name: string): Promise<void>;
  /** フォルダ parent に空のファイルかフォルダを作る(FR-025)。済んだら作った項目を選ぶ。 */
  createEntry(projectId: ProjectId, parent: RelPath, name: string, kind: NewEntryKind): Promise<void>;
  /** 名称変更(FR-026)。 */
  renameEntry(projectId: ProjectId, path: RelPath, newName: string): Promise<void>;
  /**
   * ツリー内のドロップ(FR-029)。copy(⌥ を押していた)ならすぐコピーし、そうでなければ移動の確認を開く
   * (移動はまだしない。【仮定】12)。
   */
  dropEntry(projectId: ProjectId, source: RelPath, destDir: RelPath, copy: boolean): Promise<void>;
  /** 移動の確認で「移動」を選んだ。確認を閉じて移動する。確認が無ければ何もしない。 */
  confirmMove(): Promise<void>;
  /** 移動の確認で「中止」を選んだ。確認を閉じるだけ。 */
  cancelMove(): void;
  /** 項目を移す(確認なし)。 */
  moveEntry(projectId: ProjectId, source: RelPath, destDir: RelPath): Promise<void>;
  /** 項目を写す(確認なし)。 */
  copyEntry(projectId: ProjectId, source: RelPath, destDir: RelPath): Promise<void>;
  /** ゴミ箱に入れる(FR-028)。選んでいたら選択を外す。 */
  trashEntry(projectId: ProjectId, path: RelPath): Promise<void>;
  /** 項目をコピーする(ペーストボードにファイルの URL。FR-027)。 */
  copyEntries(projectId: ProjectId, paths: RelPath[]): Promise<void>;
  /** 項目を切り取る(FR-027)。 */
  cutEntries(projectId: ProjectId, paths: RelPath[]): Promise<void>;
  /** ペーストボードのファイルをフォルダ destDir へ置く(FR-027)。 */
  pasteEntries(projectId: ProjectId, destDir: RelPath): Promise<void>;
  /** 衝突の確認で選んだ対処を付けて呼び直す(FR-030)。確認を閉じてから呼ぶ。確認が無ければ何もしない。 */
  resolveConflict(resolution: ConflictResolution): Promise<void>;
  /** 衝突の確認で「中止」を選んだ。確認を閉じるだけ(呼び直さない)。 */
  cancelConflict(): void;
  /** Finder で表示(FR-018)。 */
  revealEntry(projectId: ProjectId, path: RelPath): Promise<void>;
  /** 既定のアプリで開く(FR-018。ファイルだけ)。 */
  openEntry(projectId: ProjectId, path: RelPath): Promise<void>;
  /** パスをコピー(FR-015。Rust がクリップボードに書く)。 */
  copyEntryPath(projectId: ProjectId, path: RelPath, kind: PathKind): Promise<void>;
  /** パスをターミナル terminalId に挿入する(FR-016・FR-024。Enter は送らない)。 */
  insertEntryPath(projectId: ProjectId, paths: RelPath[], terminalId: TerminalId): Promise<void>;
}

/** 名前が正しくない理由の文言。 */
const INVALID_NAME: Readonly<Record<InvalidNameReason, string>> = {
  empty: "名前を入力してください",
  containsSlash: "名前に「/」は使えません",
  containsNul: "名前に使えない文字が含まれています",
  dotName: "「.」と「..」は名前に使えません",
  tooLong: "名前が長すぎます",
};

/** 対象にできない理由の文言。 */
const NOT_ALLOWED: Readonly<Record<NotAllowedReason, string>> = {
  outsideRoot: "プロジェクトのフォルダの外は扱えません",
  rootItself: "プロジェクトのフォルダ自体は対象にできません",
  intoDescendant: "フォルダをその中へは移せません",
};

/** 見つからない項目の知らせ(名前は 1 つの名前だけを出す。ルートは「フォルダ」)。 */
export function notFoundMessage(path: RelPath): string {
  return path === "" ? "フォルダが見つかりません" : `「${nameOf(path)}」が見つかりません`;
}

/** 親のフォルダ(ルートの親はルートとして扱う)。 */
const parentOf = (path: RelPath): RelPath => parentPath(path) ?? "";

export function createActions({ ipc, ui, tree, treeController, reportError }: ActionsDeps): Actions {
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

  /**
   * ファイル操作の応答を扱う。
   * target は「見つからない」のときに親を読み直す項目、reload は済んだときに読み直すフォルダ。
   */
  const settle = (
    projectId: ProjectId,
    outcome: FileOpOutcome,
    { target, reload, retry }: { target: RelPath; reload: readonly RelPath[]; retry?: RetryableFileOp },
  ) => {
    switch (outcome.kind) {
      case "done":
        treeController.refresh(projectId, outcome.select === null ? reload : [...reload, parentOf(outcome.select)]);
        if (outcome.select !== null) treeController.reveal(projectId, outcome.select);
        return;
      case "notFound":
        ui.getState().showTreeNotice(notFoundMessage(target));
        treeController.refresh(projectId, [parentOf(target)]);
        return;
      case "conflict":
        if (retry !== undefined) {
          ui.getState().openTreeConflict({ request: retry, name: outcome.name, total: outcome.total });
        }
        else ui.getState().showTreeNotice(`「${outcome.name}」と同じ名前の項目があります`);
        return;
      case "invalidName":
        ui.getState().showTreeNotice(INVALID_NAME[outcome.reason]);
        return;
      case "notAllowed":
        ui.getState().showTreeNotice(NOT_ALLOWED[outcome.reason]);
        return;
      case "failed":
        ui.getState().showTreeNotice(outcome.message);
        return;
    }
  };

  /** 衝突の対処を付けて(または付けずに)呼び直せるファイル操作を実行する。 */
  const runRetryable = (request: RetryableFileOp, resolution?: ConflictResolution) => {
    const { projectId } = request;
    switch (request.op) {
      case "create":
        return run("項目を作れなかった", async () => {
          const outcome = await ipc.createEntry(projectId, request.parent, request.name, request.kind, resolution);
          settle(projectId, outcome, { target: request.parent, reload: [request.parent], retry: request });
        });
      case "rename":
        return run("名前を変えられなかった", async () => {
          const outcome = await ipc.renameEntry(projectId, request.path, request.newName, resolution);
          settle(projectId, outcome, { target: request.path, reload: [parentOf(request.path)], retry: request });
        });
      case "move":
        return run("項目を移せなかった", async () => {
          const outcome = await ipc.moveEntry(projectId, request.source, request.destDir, resolution);
          settle(projectId, outcome, {
            target: request.source,
            reload: [parentOf(request.source), request.destDir],
            retry: request,
          });
        });
      case "copy":
        return run("項目をコピーできなかった", async () => {
          const outcome = await ipc.copyEntry(projectId, request.source, request.destDir, resolution);
          settle(projectId, outcome, { target: request.source, reload: [request.destDir], retry: request });
        });
      case "paste":
        return run("ペーストできなかった", async () => {
          const outcome = await ipc.pasteEntries(projectId, request.destDir, resolution);
          settle(projectId, outcome, { target: request.destDir, reload: [request.destDir], retry: request });
        });
    }
  };

  /** 呼び直しの無いファイル操作(衝突を返さないもの)。 */
  const runSimple = (
    what: string,
    projectId: ProjectId,
    target: RelPath,
    reload: readonly RelPath[],
    request: () => Promise<FileOpOutcome>,
  ) =>
    run(what, async () => {
      settle(projectId, await request(), { target, reload });
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

    expandDir: (path) => treeController.expand(path),
    collapseDir: (path) => treeController.collapse(path),
    toggleDir: (path) => treeController.toggle(path),
    collapseAllDirs: () => treeController.collapseAll(),
    selectTreePath: (path) => treeController.select(path),
    setShowHidden: (projectId, showHidden) =>
      run("隠しファイルの表示を切り替えられなかった", async () => {
        await ipc.setShowHidden(projectId, showHidden);
        treeController.reloadAll(projectId);
      }),
    commitInlineEdit: async (name) => {
      const edit = ui.getState().inlineEdit;
      if (edit === null) return;
      ui.getState().closeInlineEdit();
      if (edit.mode === "create") {
        await runRetryable({ op: "create", projectId: edit.projectId, parent: edit.parent, name, kind: edit.kind });
      } else {
        await runRetryable({ op: "rename", projectId: edit.projectId, path: edit.path, newName: name });
      }
    },
    createEntry: (projectId, parent, name, kind) => runRetryable({ op: "create", projectId, parent, name, kind }),
    renameEntry: (projectId, path, newName) => runRetryable({ op: "rename", projectId, path, newName }),
    dropEntry: async (projectId, source, destDir, copy) => {
      if (copy) {
        await runRetryable({ op: "copy", projectId, source, destDir });
        return;
      }
      ui.getState().openMoveConfirmation({ projectId, source, destDir });
    },
    confirmMove: async () => {
      const pending = ui.getState().moveConfirmation;
      if (pending === null) return;
      ui.getState().closeMoveConfirmation();
      await runRetryable({ op: "move", ...pending });
    },
    cancelMove: () => ui.getState().closeMoveConfirmation(),
    moveEntry: (projectId, source, destDir) => runRetryable({ op: "move", projectId, source, destDir }),
    copyEntry: (projectId, source, destDir) => runRetryable({ op: "copy", projectId, source, destDir }),
    trashEntry: (projectId, path) =>
      runSimple("ゴミ箱に入れられなかった", projectId, path, [parentOf(path)], async () => {
        const outcome = await ipc.trashEntry(projectId, path);
        const model = tree.getState().trees.get(projectId);
        if (outcome.kind === "done" && model !== undefined && selectionWithin(model, path)) {
          if (tree.getState().activeProjectId === projectId) treeController.select(null);
        }
        return outcome;
      }),
    copyEntries: (projectId, paths) =>
      runSimple("コピーできなかった", projectId, paths[0] ?? "", [], () => ipc.copyEntries(projectId, paths)),
    cutEntries: (projectId, paths) =>
      runSimple("切り取れなかった", projectId, paths[0] ?? "", [], () => ipc.cutEntries(projectId, paths)),
    pasteEntries: (projectId, destDir) => runRetryable({ op: "paste", projectId, destDir }),
    resolveConflict: async (resolution) => {
      const pending = ui.getState().treeConflict;
      if (pending === null) return;
      ui.getState().closeTreeConflict();
      await runRetryable(pending.request, resolution);
    },
    cancelConflict: () => ui.getState().closeTreeConflict(),
    revealEntry: (projectId, path) =>
      runSimple("Finder で表示できなかった", projectId, path, [], () => ipc.revealEntry(projectId, path)),
    openEntry: (projectId, path) =>
      runSimple("開けなかった", projectId, path, [], () => ipc.openEntry(projectId, path)),
    copyEntryPath: (projectId, path, kind) =>
      runSimple("パスをコピーできなかった", projectId, path, [], () => ipc.copyEntryPath(projectId, path, kind)),
    insertEntryPath: (projectId, paths, terminalId) =>
      runSimple("パスを挿入できなかった", projectId, paths[0] ?? "", [], () =>
        ipc.insertEntryPath(projectId, paths, terminalId),
      ),
  };
}
