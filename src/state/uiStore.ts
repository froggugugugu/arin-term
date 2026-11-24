// 表示だけの状態(設計書 §6.1 の uiStore)。開いている確認とメニューを持つ。Rust には送らない(§6.3)。
// モジュールの読み込み時には作らない。main.tsx が生成関数で作り、props で渡す(設計書 §11)。
//
// 確認ダイアログとメニューの部品(T34c・T34d)はここを読んで描き、閉じる・確定するは actions(./actions.ts)を呼ぶ。
// フォーカス位置のうち、ツリーにフォーカスがあるか(treeFocused)だけをここに持つ(M3。T43a)。ターミナルにフォーカスが
// あるかは、イベントが届いたときに TerminalManager がフォーカスのあるサーフェスを調べる(M2 のまま)。
//
// ツリーの部品(T43b〜T43d)が読む M3 の状態: メニュー(treeMenu)、インライン編集(inlineEdit)、衝突の確認(treeConflict)、
// 移動の確認(moveConfirmation)、ツリーの知らせ(treeNotice)、ドラッグ中(treeDragging)、ショートカットの表(shortcuts)。
// M4.5(T65e): 設定の知らせ(settingsNotice)。表と同じく、起動時と settings-changed が届いたときに startApp が置き換える。
// M4.6(T70a): 設定画面の開閉(settingsOpen)。⌘, とメニューの「設定…」(menu-command の "open-settings")で開く。
// M4.8(T83a。ADR-015 の 4・5・8): エディタのタブごとの表示の切替(editorViews)・帯(editorBanners)・プレビューの木
// (editorPreviews)、閉じる確認(unsavedClose)、保存の衝突の確認(saveConflict)、開いたあとのフォーカスの求め
// (pendingEditorFocus)。写しから消えたエディタのものは retainEditors で片付ける(editorSync.ts が呼ぶ)。
// メニュー・インライン編集・確認・ドラッグが開いている間は、ツリーの更新を保留する(FR-037。treeStore.ts がここを購読して
// UpdateGate の理由をそろえる。部品は保留を自分で立てない)。

import { createStore, type StoreApi } from "zustand/vanilla";

import type { MdBlock } from "../editor/markdownTypes";
import type { EditorId } from "../ipc/generated/EditorId";
import type { NewEntryKind } from "../ipc/generated/NewEntryKind";
import type { ProjectId } from "../ipc/generated/ProjectId";
import type { RelPath } from "../ipc/generated/RelPath";
import type { RunningTerminal } from "../ipc/generated/RunningTerminal";
import type { UnsavedEditor } from "../ipc/generated/UnsavedEditor";
import type { UpdateNotice } from "../ipc/generated/UpdateNotice";

export type { UpdateNotice };
import { DEFAULT_SHORTCUT_TABLE } from "../logic/keys/defaultShortcuts";
import type { ShortcutTable } from "../logic/keys/shortcutTable";
import type { TreeMenuTarget } from "../logic/menu/treeMenu";

/** 終了の確認(FR-063)。Rust の quit-requested イベントで開く。 */
export interface QuitConfirmation {
  /** 動作中のターミナル(プロジェクトの並び順・タブの順)。 */
  readonly running: readonly RunningTerminal[];
  /** 未保存のエディタ(プロジェクトの並び順・開いた順。M4.8)。 */
  readonly unsaved: readonly UnsavedEditor[];
}

/** プロジェクトの削除の確認(FR-003)。remove_project が needsConfirmation を返したときに開く。 */
export interface RemoveConfirmation {
  readonly projectId: ProjectId;
  /** そのプロジェクトの動作中のターミナル(タブの順)。 */
  readonly running: readonly RunningTerminal[];
  /** そのプロジェクトの未保存のエディタ(開いた順。M4.8)。 */
  readonly unsaved: readonly UnsavedEditor[];
}

/** エディタのタブの表示(ADR-015 の 8。「編集」「並べて」「プレビュー」)。 */
export type EditorViewMode = "edit" | "split" | "preview";

/** 表示の切替の既定(並べて)。 */
export const DEFAULT_EDITOR_VIEW: EditorViewMode = "split";

/**
 * エディタの上の帯(ADR-015 の 5)。
 * - `changed`: 未保存の変更があるあいだにほかのアプリで変わった(読み直していない)
 * - `missing`: ファイルが見つからない
 * - `failed`: 保存できなかった(読み取り専用・大きすぎる・Rust の文言)
 */
export type EditorBanner =
  | { readonly kind: "changed" }
  | { readonly kind: "missing" }
  | { readonly kind: "failed"; readonly message: string };

/** 未保存のタブを閉じる確認(AC-E07。「保存」「保存しない」「キャンセル」)。 */
export interface UnsavedClose {
  readonly editorId: EditorId;
}

/**
 * 保存の衝突の確認(AC-E05)。`changed` は外で変わっていた(「上書き保存」「読み直す」「キャンセル」)、
 * `missing` はファイルが無くなっていた(「作る」「キャンセル」)。
 */
export interface SaveConflict {
  readonly editorId: EditorId;
  readonly kind: "changed" | "missing";
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
  openQuitConfirmation: (running: readonly RunningTerminal[], unsaved?: readonly UnsavedEditor[]) => void;
  /** 終了の確認を閉じる。 */
  closeQuitConfirmation: () => void;
  /** 削除の確認を開く。開いていれば置き換える。 */
  openRemoveConfirmation: (
    projectId: ProjectId,
    running: readonly RunningTerminal[],
    unsaved?: readonly UnsavedEditor[],
  ) => void;
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
   * ショートカットの表。起動時と設定が変わったと届いたときに Rust から get_shortcuts で受け取ったもの。届くまでと、
   * 起動時の取得に失敗したときは既定の表の見本(src/logic/keys/defaultShortcuts.ts)。取り直しに失敗したときは今の表のまま。
   */
  readonly shortcuts: ShortcutTable;
  setShortcuts: (table: ShortcutTable) => void;
  /**
   * 設定の問題の知らせ(get_settings_notice の 1 行)。無ければ null。状態ファイルの知らせ(写しの notice)とは別に持ち、
   * 別に閉じる。起動時と、設定が変わったと届いたときに置き換える。
   */
  readonly settingsNotice: string | null;
  /** 設定の知らせを閉じたか。違う文の知らせ(または null)が届くと戻る(同じ文が届き直しても閉じたまま)。 */
  readonly settingsNoticeDismissed: boolean;
  setSettingsNotice: (notice: string | null) => void;
  /** 設定の知らせを閉じる。 */
  dismissSettingsNotice: () => void;
  /** 新しい版の確認の知らせ(ADR-018。get_update_notice と update-changed)。閉じたら null。 */
  readonly updateNotice: UpdateNotice | null;
  setUpdateNotice: (notice: UpdateNotice | null) => void;
  /** 設定画面(M4.6。ADR-013 の 5)が開いているか。 */
  readonly settingsOpen: boolean;
  /**
   * 設定画面を開く(⌘, とメニューの「設定…」)。開いていれば 1 つのまま(状態を変えない)。ほかの確認(終了・削除・衝突・
   * 移動)が開いていれば開かない(モーダルを 2 枚重ねない)。開くときは右クリックのメニューを閉じる。
   * 設定画面を開いているあいだに届いた終了の確認は止めずに上に重ねる(openQuitConfirmation は settingsOpen を見ない)。
   */
  openSettings: () => void;
  /** 設定画面を閉じる。 */
  closeSettings: () => void;
  /**
   * 表示中のツリーが替わった(別のプロジェクト、または無くなった)ときの片付け。projectId 以外のプロジェクトの
   * メニュー・インライン編集・衝突の確認・移動の確認・ツリーの知らせを閉じ、ドラッグ中とフォーカスの印を戻す(treeStore が呼ぶ)。
   * 部品が外れても状態は残り、更新の保留が解けない・別のプロジェクトに確認が出る、を防ぐ。
   */
  resetTreeUi: (projectId: ProjectId | null) => void;

  // ── エディタ(M4.8) ──
  /** タブごとの表示の切替(保存しない)。無いタブは既定(editorViewMode で引く)。 */
  readonly editorViews: ReadonlyMap<EditorId, EditorViewMode>;
  setEditorView: (id: EditorId, mode: EditorViewMode) => void;
  /** タブごとの帯。 */
  readonly editorBanners: ReadonlyMap<EditorId, EditorBanner>;
  /** 帯を出す・置き換える。null で消す(無ければ何もしない)。 */
  setEditorBanner: (id: EditorId, banner: EditorBanner | null) => void;
  /** タブごとのいちばん新しいプレビューの木(EditorManager の onPreview から)。 */
  readonly editorPreviews: ReadonlyMap<EditorId, readonly MdBlock[]>;
  setEditorPreview: (id: EditorId, blocks: readonly MdBlock[]) => void;
  /** 開いたエディタへフォーカスを移す求め(ツリーから開いたとき)。置き場所が受け取ったら消す。 */
  readonly pendingEditorFocus: EditorId | null;
  requestEditorFocus: (id: EditorId) => void;
  /** 求めが id のものなら消す。 */
  consumeEditorFocus: (id: EditorId) => void;
  /** 開いている閉じる確認。無ければ null。 */
  readonly unsavedClose: UnsavedClose | null;
  /** 閉じる確認を開く。開いていれば置き換える。 */
  openUnsavedClose: (id: EditorId) => void;
  closeUnsavedClose: () => void;
  /** 開いている保存の衝突の確認。無ければ null。 */
  readonly saveConflict: SaveConflict | null;
  openSaveConflict: (conflict: SaveConflict) => void;
  closeSaveConflict: () => void;
  /** ids に無いエディタの切替・帯・プレビュー・確認・フォーカスの求めを片付ける(タブを閉じた)。 */
  retainEditors: (ids: ReadonlySet<EditorId>) => void;
}

/** そのタブの表示の切替(持っていなければ既定の「並べて」)。 */
export function editorViewMode(state: Pick<UiState, "editorViews">, id: EditorId): EditorViewMode {
  return state.editorViews.get(id) ?? DEFAULT_EDITOR_VIEW;
}

/** id を除いた写し(無ければ元のまま)。 */
function without<V>(map: ReadonlyMap<EditorId, V>, keep: ReadonlySet<EditorId>): ReadonlyMap<EditorId, V> {
  if ([...map.keys()].every((id) => keep.has(id))) return map;
  return new Map([...map].filter(([id]) => keep.has(id)));
}

export type UiStore = StoreApi<UiState>;

export function createUiStore(): UiStore {
  let noticeId = 0;
  return createStore<UiState>()((set) => ({
    quitConfirmation: null,
    removeConfirmation: null,
    cardMenu: null,
    openQuitConfirmation: (running, unsaved = []) => set({ quitConfirmation: { running, unsaved } }),
    closeQuitConfirmation: () => set({ quitConfirmation: null }),
    openRemoveConfirmation: (projectId, running, unsaved = []) =>
      set({ removeConfirmation: { projectId, running, unsaved } }),
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
    settingsNotice: null,
    settingsNoticeDismissed: false,
    setSettingsNotice: (notice) =>
      set((s) => (s.settingsNotice === notice ? s : { settingsNotice: notice, settingsNoticeDismissed: false })),
    dismissSettingsNotice: () => set({ settingsNoticeDismissed: true }),
    updateNotice: null,
    setUpdateNotice: (notice) => set({ updateNotice: notice }),
    settingsOpen: false,
    openSettings: () =>
      set((s) => {
        const blocked =
          s.quitConfirmation !== null ||
          s.removeConfirmation !== null ||
          s.treeConflict !== null ||
          s.moveConfirmation !== null ||
          s.unsavedClose !== null ||
          s.saveConflict !== null;
        if (s.settingsOpen || blocked) return s;
        return { settingsOpen: true, cardMenu: null, treeMenu: null };
      }),
    closeSettings: () => set((s) => (s.settingsOpen ? { settingsOpen: false } : s)),
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
    editorViews: new Map(),
    setEditorView: (id, mode) =>
      set((s) => (s.editorViews.get(id) === mode ? s : { editorViews: new Map(s.editorViews).set(id, mode) })),
    editorBanners: new Map(),
    setEditorBanner: (id, banner) =>
      set((s) => {
        if (banner === null) {
          if (!s.editorBanners.has(id)) return s;
          const next = new Map(s.editorBanners);
          next.delete(id);
          return { editorBanners: next };
        }
        return { editorBanners: new Map(s.editorBanners).set(id, banner) };
      }),
    editorPreviews: new Map(),
    setEditorPreview: (id, blocks) => set((s) => ({ editorPreviews: new Map(s.editorPreviews).set(id, blocks) })),
    pendingEditorFocus: null,
    requestEditorFocus: (id) => set({ pendingEditorFocus: id }),
    consumeEditorFocus: (id) => set((s) => (s.pendingEditorFocus === id ? { pendingEditorFocus: null } : s)),
    unsavedClose: null,
    openUnsavedClose: (id) => set({ unsavedClose: { editorId: id } }),
    closeUnsavedClose: () => set((s) => (s.unsavedClose === null ? s : { unsavedClose: null })),
    saveConflict: null,
    openSaveConflict: (conflict) => set({ saveConflict: conflict }),
    closeSaveConflict: () => set((s) => (s.saveConflict === null ? s : { saveConflict: null })),
    retainEditors: (ids) =>
      set((s) => {
        const patch: { -readonly [K in keyof UiState]?: UiState[K] } = {};
        const views = without(s.editorViews, ids);
        if (views !== s.editorViews) patch.editorViews = views;
        const banners = without(s.editorBanners, ids);
        if (banners !== s.editorBanners) patch.editorBanners = banners;
        const previews = without(s.editorPreviews, ids);
        if (previews !== s.editorPreviews) patch.editorPreviews = previews;
        if (s.unsavedClose !== null && !ids.has(s.unsavedClose.editorId)) patch.unsavedClose = null;
        if (s.saveConflict !== null && !ids.has(s.saveConflict.editorId)) patch.saveConflict = null;
        if (s.pendingEditorFocus !== null && !ids.has(s.pendingEditorFocus)) patch.pendingEditorFocus = null;
        return Object.keys(patch).length === 0 ? s : patch;
      }),
  }));
}
