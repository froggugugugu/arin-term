// 表示だけの状態(設計書 §6.1 の uiStore)。開いている確認とメニューを持つ。Rust には送らない(§6.3)。
// モジュールの読み込み時には作らない。main.tsx が生成関数で作り、props で渡す(設計書 §11)。
//
// 確認ダイアログとメニューの部品(T34c・T34d)はここを読んで描き、閉じる・確定するは actions(./actions.ts)を呼ぶ。
// フォーカス位置のうち、ツリーにフォーカスがあるか(treeFocused)だけをここに持つ(M3。T43a)。ターミナルにフォーカスが
// あるかは、イベントが届いたときに TerminalManager がフォーカスのあるサーフェスを調べる(M2 のまま)。
//
// ツリーの部品(T43b〜T43d)が読む M3 の状態: メニュー(treeMenu)、インライン編集(inlineEdit)、衝突の確認(treeConflict)、
// 移動の確認(moveConfirmation)、ツリーの知らせ(treeNotice)、ドラッグ中(treeDragging)、ショートカットの表(shortcuts)。
// メニュー・インライン編集・確認・ドラッグが開いている間は、ツリーの更新を保留する(FR-037。treeStore.ts がここを購読して
// UpdateGate の理由をそろえる。部品は保留を自分で立てない)。

import { createStore, type StoreApi } from "zustand/vanilla";

import type { NewEntryKind } from "../ipc/generated/NewEntryKind";
import type { ProjectId } from "../ipc/generated/ProjectId";
import type { RelPath } from "../ipc/generated/RelPath";
import type { RunningTerminal } from "../ipc/generated/RunningTerminal";
import { DEFAULT_SHORTCUT_TABLE } from "../logic/keys/defaultShortcuts";
import type { ShortcutTable } from "../logic/keys/shortcutTable";
import type { TreeMenuTarget } from "../logic/menu/treeMenu";

/** 終了の確認(FR-063)。Rust の quit-requested イベントで開く。 */
export interface QuitConfirmation {
  /** 動作中のターミナル(プロジェクトの並び順・タブの順)。 */
  readonly running: readonly RunningTerminal[];
}

/** プロジェクトの削除の確認(FR-003)。remove_project が needsConfirmation を返したときに開く。 */
export interface RemoveConfirmation {
  readonly projectId: ProjectId;
  /** そのプロジェクトの動作中のターミナル(タブの順)。 */
  readonly running: readonly RunningTerminal[];
}

/** カードの右クリックのメニュー(項目は src/logic/menu/cardMenu.ts が決める)。位置は画面の CSS ピクセル。 */
export interface CardMenu {
  readonly projectId: ProjectId;
  readonly x: number;
  readonly y: number;
}

/** ツリーの右クリックのメニュー(項目は src/logic/menu/treeMenu.ts が決める)。位置は画面の CSS ピクセル。 */
export interface TreeMenuState {
  readonly projectId: ProjectId;
  readonly target: TreeMenuTarget;
  readonly x: number;
  readonly y: number;
}

/**
 * ツリーのインライン編集(FR-025・FR-026)。
 * - `create`: フォルダ parent(ルートは空の文字列)に kind の項目を作る名前の入力
 * - `rename`: 項目 path の名前の入力(選択範囲は src/logic/tree/renameSelection.ts)
 */
export type InlineEdit =
  | { readonly mode: "create"; readonly projectId: ProjectId; readonly parent: RelPath; readonly kind: NewEntryKind }
  | { readonly mode: "rename"; readonly projectId: ProjectId; readonly path: RelPath };

/**
 * 衝突したあとに対処を付けて呼び直すファイル操作(FR-030)。actions が作り、衝突の確認に載せる。
 * パスはどれもルート基準の相対パス(ルートは空の文字列)。
 */
export type RetryableFileOp =
  | { readonly op: "create"; readonly projectId: ProjectId; readonly parent: RelPath; readonly name: string; readonly kind: NewEntryKind }
  | { readonly op: "rename"; readonly projectId: ProjectId; readonly path: RelPath; readonly newName: string }
  | { readonly op: "move"; readonly projectId: ProjectId; readonly source: RelPath; readonly destDir: RelPath }
  | { readonly op: "copy"; readonly projectId: ProjectId; readonly source: RelPath; readonly destDir: RelPath }
  | { readonly op: "paste"; readonly projectId: ProjectId; readonly destDir: RelPath };

/** 衝突の確認(置き換える・両方残す・中止。FR-030)。ファイル操作が conflict を返したときに開く。 */
export interface TreeConflict {
  /** 呼び直す操作。 */
  readonly request: RetryableFileOp;
  /** 衝突した名前(Rust が返したもの。最初の 1 件の名前で、絶対パスではない)。 */
  readonly name: string;
  /** 衝突した項目の数(Rust が返したもの。1 以上)。2 以上なら確認の文面に数を出す。 */
  readonly total: number;
}

/** 移動の確認(FR-029・C-05)。ツリー内のドロップでの移動にだけ出す(計画【仮定】12)。 */
export interface MoveConfirmation {
  readonly projectId: ProjectId;
  readonly source: RelPath;
  readonly destDir: RelPath;
}

/** ツリーの 1 行の知らせ(見つかりません など)。同じ文でも出し直せるよう、出すたびに番号を変える。 */
export interface TreeNotice {
  readonly id: number;
  readonly message: string;
  /**
   * 知らせの対象のプロジェクト(T47e)。表示中のツリーが別のプロジェクトに替わったら閉じる(resetTreeUi)。
   * null はプロジェクトに結び付けない知らせ(替わっても閉じない)。
   */
  readonly projectId: ProjectId | null;
}

export interface UiState {
  /** 開いている終了の確認。無ければ null。 */
  readonly quitConfirmation: QuitConfirmation | null;
  /** 開いている削除の確認。無ければ null。 */
  readonly removeConfirmation: RemoveConfirmation | null;
  /** 開いているカードのメニュー。無ければ null。 */
  readonly cardMenu: CardMenu | null;
  /**
   * 終了の確認を開く。開いていれば 1 つのまま、動作中の一覧を新しいものに置き換える
   * (⌘Q を続けて押したなどで同じ要求が複数回届いても、確認は 1 つ)。
   */
  openQuitConfirmation: (running: readonly RunningTerminal[]) => void;
  /** 終了の確認を閉じる。 */
  closeQuitConfirmation: () => void;
  /** 削除の確認を開く。開いていれば置き換える。 */
  openRemoveConfirmation: (projectId: ProjectId, running: readonly RunningTerminal[]) => void;
  /** 削除の確認を閉じる。 */
  closeRemoveConfirmation: () => void;
  /** カードのメニューを開く。開いていれば置き換える。 */
  openCardMenu: (projectId: ProjectId, position: { readonly x: number; readonly y: number }) => void;
  /** カードのメニューを閉じる。 */
  closeCardMenu: () => void;
  /**
   * 画面で閉じた知らせ(写しの `notice`)の文。無ければ null。Rust に「閉じた」を伝えるコマンドは無いので、
   * 閉じたことは画面だけが覚え、同じ文の知らせを出さない(知らせは保存されないので、次の起動では出ない)。
   */
  readonly dismissedNotice: string | null;
  /** 知らせを閉じる。 */
  dismissNotice: (notice: string) => void;

  /** 開いているツリーのメニュー。無ければ null。 */
  readonly treeMenu: TreeMenuState | null;
  /** ツリーのメニューを開く。開いていれば置き換える。 */
  openTreeMenu: (
    projectId: ProjectId,
    target: TreeMenuTarget,
    position: { readonly x: number; readonly y: number },
  ) => void;
  /** ツリーのメニューを閉じる。 */
  closeTreeMenu: () => void;
  /** 開いているインライン編集。無ければ null。 */
  readonly inlineEdit: InlineEdit | null;
  /** インライン編集を始める。編集中なら置き換える(前の入力は取り消し)。 */
  startInlineEdit: (edit: InlineEdit) => void;
  /** インライン編集を閉じる(確定は actions.commitInlineEdit、取り消しはこれだけ)。 */
  closeInlineEdit: () => void;
  /** 開いている衝突の確認。無ければ null。 */
  readonly treeConflict: TreeConflict | null;
  /** 衝突の確認を開く。開いていれば置き換える。 */
  openTreeConflict: (conflict: TreeConflict) => void;
  /** 衝突の確認を閉じる。 */
  closeTreeConflict: () => void;
  /** 開いている移動の確認。無ければ null。 */
  readonly moveConfirmation: MoveConfirmation | null;
  /** 移動の確認を開く。開いていれば置き換える。 */
  openMoveConfirmation: (confirmation: MoveConfirmation) => void;
  /** 移動の確認を閉じる。 */
  closeMoveConfirmation: () => void;
  /** 出しているツリーの知らせ。無ければ null。 */
  readonly treeNotice: TreeNotice | null;
  /** ツリーの知らせを出す。出ていれば置き換える。projectId はその知らせの対象(省略は null)。 */
  showTreeNotice: (message: string, projectId?: ProjectId | null) => void;
  /** ツリーの知らせを閉じる。 */
  dismissTreeNotice: () => void;
  /** ツリーの項目をドラッグ中か(T43d が立てる。更新を保留する)。 */
  readonly treeDragging: boolean;
  setTreeDragging: (dragging: boolean) => void;
  /** ツリー(行やヘッダ。名前の入力欄を除く)にフォーカスがあるか。キーとコピー・ペーストの振り分けに使う(C-04)。 */
  readonly treeFocused: boolean;
  setTreeFocused: (focused: boolean) => void;
  /**
   * ショートカットの表。起動時に Rust から get_shortcuts で受け取ったもの。届くまでと、取得に失敗したときは
   * 既定の表の見本(src/logic/keys/defaultShortcuts.ts)。
   */
  readonly shortcuts: ShortcutTable;
  setShortcuts: (table: ShortcutTable) => void;
  /**
   * 表示中のツリーが替わった(別のプロジェクト、または無くなった)ときの片付け。projectId 以外のプロジェクトの
   * メニュー・インライン編集・衝突の確認・移動の確認・ツリーの知らせを閉じ、ドラッグ中とフォーカスの印を戻す(treeStore が呼ぶ)。
   * 部品が外れても状態は残り、更新の保留が解けない・別のプロジェクトに確認が出る、を防ぐ。
   */
  resetTreeUi: (projectId: ProjectId | null) => void;
}

export type UiStore = StoreApi<UiState>;

export function createUiStore(): UiStore {
  let noticeId = 0;
  return createStore<UiState>()((set) => ({
    quitConfirmation: null,
    removeConfirmation: null,
    cardMenu: null,
    openQuitConfirmation: (running) => set({ quitConfirmation: { running } }),
    closeQuitConfirmation: () => set({ quitConfirmation: null }),
    openRemoveConfirmation: (projectId, running) => set({ removeConfirmation: { projectId, running } }),
    closeRemoveConfirmation: () => set({ removeConfirmation: null }),
    openCardMenu: (projectId, { x, y }) => set({ cardMenu: { projectId, x, y } }),
    closeCardMenu: () => set({ cardMenu: null }),
    dismissedNotice: null,
    dismissNotice: (notice) => set({ dismissedNotice: notice }),
    treeMenu: null,
    openTreeMenu: (projectId, target, { x, y }) => set({ treeMenu: { projectId, target, x, y } }),
    closeTreeMenu: () => set({ treeMenu: null }),
    inlineEdit: null,
    startInlineEdit: (edit) => set({ inlineEdit: edit }),
    closeInlineEdit: () => set({ inlineEdit: null }),
    treeConflict: null,
    openTreeConflict: (conflict) => set({ treeConflict: conflict }),
    closeTreeConflict: () => set({ treeConflict: null }),
    moveConfirmation: null,
    openMoveConfirmation: (confirmation) => set({ moveConfirmation: confirmation }),
    closeMoveConfirmation: () => set({ moveConfirmation: null }),
    treeNotice: null,
    showTreeNotice: (message, projectId = null) => {
      noticeId += 1;
      set({ treeNotice: { id: noticeId, message, projectId } });
    },
    dismissTreeNotice: () => set({ treeNotice: null }),
    treeDragging: false,
    setTreeDragging: (dragging) => set({ treeDragging: dragging }),
    treeFocused: false,
    setTreeFocused: (focused) => set({ treeFocused: focused }),
    shortcuts: DEFAULT_SHORTCUT_TABLE,
    setShortcuts: (table) => set({ shortcuts: table }),
    resetTreeUi: (projectId) =>
      set((s) => {
        const other = (owner: ProjectId | undefined) => owner !== undefined && owner !== projectId;
        const patch: { -readonly [K in keyof UiState]?: UiState[K] } = {};
        if (other(s.treeMenu?.projectId)) patch.treeMenu = null;
        if (other(s.inlineEdit?.projectId)) patch.inlineEdit = null;
        if (other(s.treeConflict?.request.projectId)) patch.treeConflict = null;
        if (other(s.moveConfirmation?.projectId)) patch.moveConfirmation = null;
        if (other(s.treeNotice?.projectId ?? undefined)) patch.treeNotice = null;
        if (s.treeDragging) patch.treeDragging = false;
        if (s.treeFocused) patch.treeFocused = false;
        return Object.keys(patch).length === 0 ? s : patch;
      }),
  }));
}
