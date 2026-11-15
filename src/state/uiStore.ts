// 表示だけの状態(設計書 §6.1 の uiStore)。開いている確認とメニューを持つ。Rust には送らない(§6.3)。
// モジュールの読み込み時には作らない。main.tsx が生成関数で作り、props で渡す(設計書 §11)。
//
// 確認ダイアログとメニューの部品(T34c・T34d)はここを読んで描き、閉じる・確定するは actions(./actions.ts)を呼ぶ。
// フォーカス位置(ターミナルかそれ以外か)はここに持たない。コピー・ペーストと「すべてを選択」は、
// イベントが届いたときに TerminalManager がフォーカスのあるサーフェスを調べて振り分ける(M2 の範囲ではそれで足りる)。

import { createStore, type StoreApi } from "zustand/vanilla";

import type { ProjectId } from "../ipc/generated/ProjectId";
import type { RunningTerminal } from "../ipc/generated/RunningTerminal";

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
}

export type UiStore = StoreApi<UiState>;

export function createUiStore(): UiStore {
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
  }));
}
