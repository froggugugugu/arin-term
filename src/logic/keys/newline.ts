// ターミナルの Shift+Enter の改行(M4.7。ADR-014 の 5)。DOM を使わない純粋な関数。
//
// Shift+Enter(ほかの修飾キーなし、IME の変換中でない)は ESC + CR を送る。Claude Code・Codex などの入力欄は、これを
// 「改行」と読む(iTerm2・VS Code の `/terminal-setup` が入れる設定と同じ並び)。素の zsh(既定のキーの割り当て)では、実行されずに改行が入る(複数行の入力になる)。
// 表には載せない(ターミナルに渡す文字の決まりで、コマンドではない)。⌘⇧Enter(拡大)など修飾キーつきは表の扱いのまま。

/** Shift+Enter で送る文字(ESC + CR)。 */
export const NEWLINE_INPUT = "\x1b\r";

/** 判定に使うキーの事象の部分(KeyboardEvent はこの形を満たす)。 */
export interface NewlineKeyEvent {
  readonly type: string;
  readonly key: string;
  readonly shiftKey: boolean;
  readonly metaKey: boolean;
  readonly ctrlKey: boolean;
  readonly altKey: boolean;
  readonly isComposing: boolean;
  /** IME の変換中の keydown は 229 になる(WebKit は isComposing を立てないことがある)。 */
  readonly keyCode?: number;
}

/** IME の変換中か。 */
function composing(event: NewlineKeyEvent): boolean {
  return event.isComposing || event.keyCode === 229;
}

/**
 * Shift だけを押した Enter か(keydown・keypress・keyup のどれでも)。IME の変換中は違う。
 * 当たるキーは xterm.js に処理させない(keypress で CR を送らせないため)。
 */
export function isShiftEnter(event: NewlineKeyEvent): boolean {
  return (
    event.key === "Enter" &&
    event.shiftKey &&
    !event.metaKey &&
    !event.ctrlKey &&
    !event.altKey &&
    !composing(event)
  );
}

/** ターミナルへ送る文字。Shift+Enter の keydown なら ESC + CR、ほかは null(xterm.js の既定のまま)。 */
export function newlineInput(event: NewlineKeyEvent): string | null {
  return event.type === "keydown" && isShiftEnter(event) ? NEWLINE_INPUT : null;
}
