// ターミナルにフォーカスがあるときのキーの受け手(設計書 §9.1、PRD C-04、FR-043、FR-044)。DOM を使わない純粋な関数。
//
// アプリのショートカットはメニューバーの項目のアクセラレータで受ける(計画 M2 の決定 (b))。
// ここで `app` にしたキーは xterm.js に処理させず、メニューの項目と画面のイベントに任せる。

/** キーの押し下げのうち、振り分けに使う部分(KeyboardEvent はこの形を満たす)。 */
export interface KeyChord {
  readonly key: string;
  readonly metaKey: boolean;
  readonly ctrlKey: boolean;
  readonly altKey: boolean;
  readonly shiftKey: boolean;
}

/**
 * キーの受け手。
 * - `app`: ターミナル(xterm.js)は処理しない。メニューの項目と画面のイベントに任せる
 * - `terminal`: xterm.js に任せる(Ctrl の組み合わせは制御文字、Option は xterm.js の既定のまま)
 */
export type KeyOwner = "app" | "terminal";

/** ⌘ だけを押したときにアプリが受けるキー: コピー・ペースト・すべてを選択・新規タブ・タブを閉じる。 */
const COMMAND_KEYS: ReadonlySet<string> = new Set(["c", "v", "a", "t", "w"]);

/**
 * ⌘⇧ を押したときにアプリが受けるキー: 前のタブ・次のタブ。
 * ⇧ を押した `key` は配列によって「[」「]」のままのことも「{」「}」になることもあるので、両方を受ける。
 */
const COMMAND_SHIFT_KEYS: ReadonlySet<string> = new Set(["[", "]", "{", "}"]);

/** ターミナルにフォーカスがあるときに、押されたキーを誰が受けるか。 */
export function terminalKeyOwner(chord: KeyChord): KeyOwner {
  if (!chord.metaKey || chord.ctrlKey || chord.altKey) return "terminal";
  const keys = chord.shiftKey ? COMMAND_SHIFT_KEYS : COMMAND_KEYS;
  return keys.has(chord.key.toLowerCase()) ? "app" : "terminal";
}
