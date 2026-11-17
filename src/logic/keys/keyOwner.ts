// ターミナルにフォーカスがあるときのキーの受け手(設計書 §9.1、PRD C-04、FR-043、FR-044)。DOM を使わない純粋な関数。
//
// アプリのショートカットはメニューバーの項目のアクセラレータと、「編集」の項目(画面のコピー・切り取り・ペーストのイベント)で受ける
// (計画 M2 の決定 (b)、M3 の【仮定】21)。ここで `app` にしたキーは xterm.js に処理させず、メニューの項目と画面のイベントに任せる。
// どのキーを `app` にするかは、ショートカットの表(./shortcutTable)の受け口が `menu-item`・`edit-item` の行から決める
// (キーを直書きしない。前提 (e))。受け口が `screen` の行(ツリーのキー)はターミナルでは使わないので xterm.js に任せる。

import { findBinding, type PressedKey, type ShortcutTable } from "./shortcutTable";

/** キーの押し下げのうち、振り分けに使う部分(KeyboardEvent はこの形を満たす)。`code` は ⌥ を含むキーを比べるときに使う。 */
export type KeyChord = PressedKey;

/**
 * キーの受け手。
 * - `app`: ターミナル(xterm.js)は処理しない。メニューの項目と画面のイベントに任せる
 * - `terminal`: xterm.js に任せる(Ctrl の組み合わせは制御文字、Option は xterm.js の既定のまま)
 */
export type KeyOwner = "app" | "terminal";

/** ターミナルの上で、アプリが受けるキーの受け口。 */
const APP_RECEIVERS = ["menu-item", "edit-item"] as const;

/** ターミナルにフォーカスがあるときに、押されたキーを誰が受けるか。 */
export function terminalKeyOwner(table: ShortcutTable, chord: KeyChord): KeyOwner {
  return findBinding(table, chord, APP_RECEIVERS) === null ? "terminal" : "app";
}
