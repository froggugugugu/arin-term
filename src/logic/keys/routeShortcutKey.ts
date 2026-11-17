// 画面の keydown を、ショートカットの表とフォーカス位置でコマンドの ID にする(計画 T41b、【仮定】21・22)。
// DOM を使わない純粋な関数。
//
// keydown で受けるのは、表の受け口が `screen` の行だけ(ツリーのキー)。受け口が `menu-item` の行は OS がメニューの
// アクセラレータで受け、`edit-item` の行はコピー・切り取り・ペーストのイベント(./clipboard)で受けるので、ここでは扱わない
// (二重に動かさないため)。受け口を表の 1 行で替えれば、ここの振る舞いも替わる。

import type { FocusArea } from "./focus";
import type { KeyChord } from "./keyOwner";
import { findBinding, type ShortcutCommandId, type ShortcutTable } from "./shortcutTable";

const SCREEN_RECEIVERS = ["screen"] as const;

/**
 * 押されたキーを、フォーカス位置でコマンドの ID にする。受けないキーは null(画面の既定の動作に任せる)。
 * ツリーにフォーカスがあるときだけ受ける。名前の入力中・ターミナル・ほかの場所では受けない。
 */
export function routeShortcutKey(
  table: ShortcutTable,
  chord: KeyChord,
  focus: FocusArea,
): ShortcutCommandId | null {
  if (focus !== "tree") return null;
  return findBinding(table, chord, SCREEN_RECEIVERS)?.command ?? null;
}
