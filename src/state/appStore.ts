// Rust から届いた状態の写しを持つストア(設計書 §6.1)。画面は写しを書き換えず、届いたもので丸ごと置き換える。
// モジュールの読み込み時には作らない。main.tsx が生成関数で作り、props で渡す(設計書 §11、ADR-003)。

import { createStore, type StoreApi } from "zustand/vanilla";

import type { AppSnapshot } from "../ipc/generated/AppSnapshot";

export interface AppState {
  /** 最後に届いた写し。まだ届いていなければ null。 */
  readonly snapshot: AppSnapshot | null;
  /** 届いた写しで丸ごと置き換える。 */
  replaceSnapshot: (snapshot: AppSnapshot) => void;
}

export type AppStore = StoreApi<AppState>;

export function createAppStore(): AppStore {
  return createStore<AppState>()((set) => ({
    snapshot: null,
    replaceSnapshot: (snapshot) => {
      set({ snapshot });
    },
  }));
}
