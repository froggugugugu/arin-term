// エディタにフォーカスがあるときのキーの受け手(ADR-015 の 6、AC-E09)。DOM を使わない純粋な関数。
// ターミナルの terminalKeyOwner(./keyOwner)と同じ考え: どのキーをアプリに任せるかは、ショートカットの表から決める
// (キーを直書きしない)。
//   - 受け口が `menu-item`・`edit-item` の行のキー → `app`(CodeMirror は処理しない。メニューと画面のコピー・ペーストのイベントへ)
//   - `editor.save` の行のキー → `save`
//   - それ以外(受け口が `screen` のツリーのキー、⌘Z・Esc、ふつうの文字を含む) → `editor`(CodeMirror。通常の keymap か Vim)
// 保存のキーがメニューの項目のキーと重なったら、メニューを先にする(⌘W などをエディタが奪わない)。

import type { KeyChord } from "./keyOwner";
import { findBinding, type ShortcutTable } from "./shortcutTable";

/** エディタの上のキーの受け手。 */
export type EditorKeyOwner = "app" | "save" | "editor";

const APP_RECEIVERS = ["menu-item", "edit-item"] as const;
const SCREEN_RECEIVERS = ["screen"] as const;

/** エディタにフォーカスがあるときに、押されたキーを誰が受けるか。 */
export function editorKeyOwner(table: ShortcutTable, chord: KeyChord): EditorKeyOwner {
  if (findBinding(table, chord, APP_RECEIVERS) !== null) return "app";
  const save = table.filter((binding) => binding.command === "editor.save");
  return findBinding(save, chord, SCREEN_RECEIVERS) === null ? "editor" : "save";
}
