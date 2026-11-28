// ターミナルの真ん中クリックと右クリックを、Arin が受けるかアプリに渡すかを決める(ADR-020 の 2)。DOM を使わない純粋な関数。
//
// マウスを使うアプリ(vim の mouse=a、htop など。xterm.js のマウスの報告のモード)が動いているあいだはアプリに渡す。
// Option を押しながらなら Arin が受ける(Option を押しながらのドラッグで選べるのと同じ考え方)。

/** クリックしたときの様子。 */
export interface TerminalMouseState {
  /** マウスの報告のモードが有効か(アプリがマウスを使っている)。 */
  readonly appOwnsMouse: boolean;
  /** Option を押していたか。 */
  readonly altKey: boolean;
}

/** Arin が受けるか(false ならアプリ・xterm.js に任せる)。 */
export function arinTakesMouse(state: TerminalMouseState): boolean {
  return !state.appOwnsMouse || state.altKey;
}

/**
 * 真ん中クリックで何をするか。
 * - `paste`: 持っている選択を貼る(持っていなければ何も貼らないが、クリックは Arin が受ける)
 * - `pass`: 何もしない(xterm.js・アプリに任せる)
 */
export function middleClickAction(state: TerminalMouseState & { readonly enabled: boolean }): "paste" | "pass" {
  return state.enabled && arinTakesMouse(state) ? "paste" : "pass";
}
