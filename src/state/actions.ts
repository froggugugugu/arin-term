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
// - 応答が届いた時点で、そのプロジェクトのツリーがもう表示中でなければ(タイルのクリックなどで替えた)、確認も知らせも
//   開かずに捨てる(T47e。M3 の残り)。読み直しも表示中のツリーにしか効かないが、戻ったときに読み込み済みのフォルダを
//   すべて読み直すので、中身は追いつく(treeStore.ts)。知らせはそのプロジェクトに結び付け、替えたら閉じる
//
// 表示の操作(M4。T47e): 表示モードの切替・タイルへのフォーカス・ページ送り・拡大は、どれも Rust のコマンドを呼ぶだけ
// (応答は無く、結果は写しの display で届く)。フォーカス中のタイルへのフォーカスは呼ばない(写しを増やさない)。

import type { ArinIpc } from "../ipc/client";
import type { AppSnapshot } from "../ipc/generated/AppSnapshot";
import type { ConflictResolution } from "../ipc/generated/ConflictResolution";
import type { DisplayMode } from "../ipc/generated/DisplayMode";
import type { EditorId } from "../ipc/generated/EditorId";
import type { FileOpOutcome } from "../ipc/generated/FileOpOutcome";
import type { FontSizeTarget } from "../ipc/generated/FontSizeTarget";
import type { InvalidNameReason } from "../ipc/generated/InvalidNameReason";
import type { NewEntryKind } from "../ipc/generated/NewEntryKind";
import type { NotAllowedReason } from "../ipc/generated/NotAllowedReason";
import type { PathKind } from "../ipc/generated/PathKind";
import type { ProjectColor } from "../ipc/generated/ProjectColor";
import type { ProjectId } from "../ipc/generated/ProjectId";
import type { RelPath } from "../ipc/generated/RelPath";
import type { TerminalId } from "../ipc/generated/TerminalId";
import type { ThemeMode } from "../ipc/generated/ThemeMode";
import type { TranslucencyTarget } from "../ipc/generated/TranslucencyTarget";
import { describeError } from "../logic/describeError";
import { isMarkdownPath } from "../logic/editor/documentPath";
import { nameOf, parentPath } from "../logic/tree/treePath";
import type { AppStore } from "./appStore";
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
  | "moveProject"
  | "newTerminal"
  | "closeTerminal"
  | "selectTerminal"
  | "moveTerminalTab"
  | "moveEditorTab"
  | "closeTabs"
  | "setPaneSizes"
  | "setSidebarCollapsed"
  | "setDisplayMode"
  | "focusTerminal"
  | "nextTilePage"
  | "previousTilePage"
  | "toggleTileMaximize"
  | "stepFontSize"
  | "stepTranslucency"
  | "setTheme"
  | "resetAppearance"
  | "openSettingsFile"
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
  /** 写し(フォーカス中のタイルを読む。T47e)。 */
  app: AppStore;
  /** ツリーの状態(ゴミ箱に入れた項目の選択を外す・応答のプロジェクトが表示中かを確かめるのに読む)。 */
  tree: TreeStore;
  /** ツリーの開閉・選択・読み直し。 */
  treeController: TreeController;
  /** 失敗の報告先(ErrorReporter.report)。 */
  reportError: (message: string) => void;
  /**
   * エディタで開く(M4.8。./editorController.ts)。渡せば `.md`・`.markdown` の「開く」をここへ回す。
   * 渡さなければ(エディタを使わないテスト)今までどおり既定のアプリで開く。
   */
  editor?: {
    openDocument(projectId: ProjectId, path: RelPath): Promise<void>;
    /** 画面の部品が未保存か(タブをまとめて閉じるときに Rust へ知らせる。ADR-019)。 */
    isDirty?(id: EditorId): boolean;
  };
}

/** 画面の操作。どの Promise も失敗しない(失敗は報告済み)。 */
export interface Actions {
  /** フォルダ選択のダイアログ(Rust が開く)で選んだフォルダを登録する(FR-001)。 */
  addProjectByDialog(): Promise<void>;
  /** プロジェクトを選ぶ(FR-002)。 */
  selectProject(id: ProjectId): Promise<void>;
  /** プロジェクトを toIndex(動かしたあとの位置)へ動かす(カードのドラッグ。ADR-019)。 */
  moveProject(id: ProjectId, toIndex: number): Promise<void>;
  /** ターミナルのタブを同じプロジェクトのターミナルの中で動かす(タブのドラッグ。ADR-019)。 */
  moveTerminalTab(id: TerminalId, toIndex: number): Promise<void>;
  /** エディタのタブを同じプロジェクトのエディタの中で動かす(タブのドラッグ。ADR-019)。 */
  moveEditorTab(id: EditorId, toIndex: number): Promise<void>;
  /**
   * タブをまとめて閉じる(タブの右クリックのメニュー。ADR-019)。動作中のターミナルか未保存のエディタがあれば、
   * 閉じずに確認を開く。どちらも無ければ確認せずに閉じる。何も挙げなければ何もしない。
   */
  closeTabs(projectId: ProjectId, terminalIds: readonly TerminalId[], editorIds: readonly EditorId[]): Promise<void>;
  /** 開いているタブをまとめて閉じる確認で「閉じる」を選んだ。確認を閉じ、確認済みで閉じる。確認が無ければ何もしない。 */
  confirmCloseTabs(): Promise<void>;
  /** 開いているタブをまとめて閉じる確認で「キャンセル」を選んだ。確認を閉じるだけ。 */
  cancelCloseTabs(): void;
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
  newTerminal(projectId?: ProjectId): Promise<void>;
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

  // ── 表示(M4。T47e)。結果は写しの display で届く ──
  /** 表示モードをタブ表示・タイル表示にする(FR-048。表示モードの切替のボタン)。 */
  setDisplayMode(mode: DisplayMode): Promise<void>;
  /**
   * タイルをクリックした・タイルに入力のフォーカスが入った: そのターミナルにフォーカスする(FR-053。そのプロジェクトが
   * 選ばれ、そのタブが表示中になる)。もうフォーカス中(写しの display.focusedTerminalId)なら何も呼ばない。
   * 同じタイルを求めたあと、写しがまだ届いていなければ送り直さない(失敗したら送り直せる)。
   */
  focusTile(id: TerminalId): Promise<void>;
  /** 次のページへ(FR-052。端では Rust が何もしない)。 */
  nextTilePage(): Promise<void>;
  /** 前のページへ(FR-052)。 */
  previousTilePage(): Promise<void>;
  /**
   * タイルを拡大する・元に戻す(FR-054)。タイトルバーのダブルクリックは対象のタイルの ID を渡す(そのタイルにフォーカス
   * してから拡大する)。元に戻すボタン・メニューは渡さない(フォーカス中のタイル)。
   */
  toggleTileMaximize(terminalId?: TerminalId): Promise<void>;

  // ── 設定画面(M4.6・M4.7。ADR-013 の 2・5、ADR-014 の 1・3)。結果は写しの fontSizes・translucency で届く。開閉は uiStore ──
  /** 文字の大きさを delta だけ動かす(ステッパー。今の値を足した数ではなく差分を送る)。 */
  stepFontSize(target: FontSizeTarget, delta: number): Promise<void>;
  /** 透過を delta(% の差分)だけ動かす(ステッパー。今の値を足した数ではなく差分を送る)。 */
  stepTranslucency(target: TranslucencyTarget, delta: number): Promise<void>;
  /** テーマの選び方(システム・ライト・ダーク)を替える(ADR-016)。結果は写しの theme で届く。 */
  setTheme(theme: ThemeMode): Promise<void>;
  /** 文字の大きさ 2 つと透過 2 つを既定に戻す(確認を挟まない)。 */
  resetAppearance(): Promise<void>;
  /** 設定ファイルを開く(設定画面のボタン。メニューの項目は Rust が直接開く)。 */
  openSettingsFile(): Promise<void>;

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
  /**
   * 開く(FR-018。ファイルだけ。ダブルクリック・⌘O・メニューの「開く」)。`.md`・`.markdown` はエディタのタブで開き
   * (M4.8。ADR-015 の 4)、ほかは既定のアプリで開く。
   */
  openEntry(projectId: ProjectId, path: RelPath): Promise<void>;
  /** 既定のアプリで開く(拡張子によらない。`.md` の右クリックの「既定のアプリで開く」。M4.8)。 */
  openEntryExternally(projectId: ProjectId, path: RelPath): Promise<void>;
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

export function createActions({ ipc, ui, app, tree, treeController, reportError, editor }: ActionsDeps): Actions {
  /** そのプロジェクトのツリーが表示中か(替えたあとに届いた応答で、確認や知らせを開かないため。T47e)。 */
  const isShown = (projectId: ProjectId) => tree.getState().activeProjectId === projectId;
  /** 送ったタイルへのフォーカスの求めと、送ったときの写し(focusTile が同じ求めを続けて送らないため)。 */
  let pendingFocus: { id: TerminalId; snapshot: AppSnapshot | null } | null = null;

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
      if (outcome.kind === "needsConfirmation") {
        ui.getState().openRemoveConfirmation(id, outcome.running, outcome.unsaved);
      }
    });

  /**
   * タブをまとめて閉じる。画面が未保存と知っているエディタも一緒に送る(Rust の印は遅れて届くことがある)。
   * confirmed は本人が確認した一覧(初めは無し)。確認が要れば確認を開く(確認のあとに増えていれば開き直す)。
   */
  const closeTabs = (
    projectId: ProjectId,
    terminalIds: readonly TerminalId[],
    editorIds: readonly EditorId[],
    confirmed: { terminals: readonly TerminalId[]; editors: readonly EditorId[] } | null,
  ) =>
    run("タブを閉じられなかった", async () => {
      if (terminalIds.length === 0 && editorIds.length === 0) return;
      const outcome = await ipc.closeTabs(projectId, {
        terminalIds: [...terminalIds],
        editorIds: [...editorIds],
        dirtyEditorIds: editorIds.filter((id) => editor?.isDirty?.(id) === true),
        confirmedTerminalIds: [...(confirmed?.terminals ?? [])],
        confirmedEditorIds: [...(confirmed?.editors ?? [])],
      });
      if (outcome.kind === "needsConfirmation") {
        ui.getState().openCloseTabsConfirmation({
          projectId,
          terminalIds,
          editorIds,
          running: outcome.running,
          unsaved: outcome.unsaved,
        });
      }
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
    // 読み直し・選択は表示中のツリーにしか効かない(treeController が確かめる)。確認と知らせはここで確かめる。
    const notice = (message: string) => {
      if (isShown(projectId)) ui.getState().showTreeNotice(message, projectId);
    };
    switch (outcome.kind) {
      case "done":
        treeController.refresh(projectId, outcome.select === null ? reload : [...reload, parentOf(outcome.select)]);
        if (outcome.select !== null) treeController.reveal(projectId, outcome.select);
        return;
      case "notFound":
        notice(notFoundMessage(target));
        treeController.refresh(projectId, [parentOf(target)]);
        return;
      case "conflict":
        if (retry === undefined) notice(`「${outcome.name}」と同じ名前の項目があります`);
        else if (isShown(projectId)) {
          ui.getState().openTreeConflict({ request: retry, name: outcome.name, total: outcome.total });
        }
        return;
      case "invalidName":
        notice(INVALID_NAME[outcome.reason]);
        return;
      case "notAllowed":
        notice(NOT_ALLOWED[outcome.reason]);
        return;
      case "failed":
        notice(outcome.message);
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
    moveProject: (id, toIndex) => run("プロジェクトの並びを変えられなかった", () => ipc.moveProject(id, toIndex)),
    moveTerminalTab: (id, toIndex) => run("タブの並びを変えられなかった", () => ipc.moveTerminalTab(id, toIndex)),
    moveEditorTab: (id, toIndex) => run("タブの並びを変えられなかった", () => ipc.moveEditorTab(id, toIndex)),
    closeTabs: (projectId, terminalIds, editorIds) => closeTabs(projectId, terminalIds, editorIds, null),
    confirmCloseTabs: async () => {
      const pending = ui.getState().closeTabsConfirmation;
      if (pending === null) return;
      ui.getState().closeCloseTabsConfirmation();
      // 確認に出した一覧だけを確認済みとして送る(確認のあいだに増えたものは Rust がもう一度確認を求める)
      await closeTabs(pending.projectId, pending.terminalIds, pending.editorIds, {
        terminals: pending.running.map((t) => t.terminalId),
        editors: pending.unsaved.map((e) => e.editorId),
      });
    },
    cancelCloseTabs: () => ui.getState().closeCloseTabsConfirmation(),
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
    newTerminal: (projectId) => run("新しいターミナルを開けなかった", () => ipc.newTerminal(projectId)),
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

    setDisplayMode: (mode) => run("表示モードを切り替えられなかった", () => ipc.setDisplayMode(mode)),
    focusTile: async (id) => {
      const snapshot = app.getState().snapshot;
      if (snapshot?.display.focusedTerminalId === id) return;
      // 同じタイルを求めたあと、まだ写しが届いていなければ送り直さない(押下のあとに xterm.js が入力欄へ移すフォーカス、
      // ドロップのあとのフォーカスでも呼ばれる。M4 のレビューの S2)。写しが届けば(どの写しでも)次は送る。
      if (pendingFocus !== null && pendingFocus.id === id && pendingFocus.snapshot === snapshot) return;
      pendingFocus = { id, snapshot };
      await run("タイルにフォーカスできなかった", async () => {
        try {
          await ipc.focusTerminal(id);
        } catch (cause) {
          // 失敗したら、同じタイルをもう一度求められるようにする。
          if (pendingFocus?.id === id) pendingFocus = null;
          throw cause;
        }
      });
    },
    nextTilePage: () => run("次のページに送れなかった", () => ipc.nextTilePage()),
    previousTilePage: () => run("前のページに戻せなかった", () => ipc.previousTilePage()),
    toggleTileMaximize: (terminalId) =>
      run("タイルを拡大・元に戻せなかった", () => ipc.toggleTileMaximize(terminalId)),

    stepFontSize: (target, delta) => run("文字の大きさを変えられなかった", () => ipc.stepFontSize(target, delta)),
    stepTranslucency: (target, delta) => run("透過を変えられなかった", () => ipc.stepTranslucency(target, delta)),
    setTheme: (theme) => run("テーマを変えられなかった", () => ipc.setTheme(theme)),
    resetAppearance: () => run("見た目を既定に戻せなかった", () => ipc.resetAppearance()),
    openSettingsFile: () => run("設定ファイルを開けなかった", () => ipc.openSettingsFile()),

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
      // 表示中でないプロジェクトの確認は開かない(開いても resetTreeUi が閉じる対象。T47e)。
      if (isShown(projectId)) ui.getState().openMoveConfirmation({ projectId, source, destDir });
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
      editor !== undefined && isMarkdownPath(path)
        ? editor.openDocument(projectId, path)
        : runSimple("開けなかった", projectId, path, [], () => ipc.openEntry(projectId, path)),
    openEntryExternally: (projectId, path) =>
      runSimple("開けなかった", projectId, path, [], () => ipc.openEntry(projectId, path)),
    copyEntryPath: (projectId, path, kind) =>
      runSimple("パスをコピーできなかった", projectId, path, [], () => ipc.copyEntryPath(projectId, path, kind)),
    insertEntryPath: (projectId, paths, terminalId) =>
      runSimple("パスを挿入できなかった", projectId, paths[0] ?? "", [], () =>
        ipc.insertEntryPath(projectId, paths, terminalId),
      ),
  };
}
