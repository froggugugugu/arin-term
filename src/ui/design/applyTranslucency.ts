// 透過(M4.7。ADR-014 の 2)を画面に当てる 1 か所。:root の不透明度の変数を CSSOM の setProperty で上書きする
// (HTML の差し込みは使わない)。tokens.css の値は既定のまま残し、写しが届く前と、当てられなかったときの値になる。
// 色(RGB)は tokens.css が外観ごとに持ち、ここは不透明度だけを書く。xterm.js には何も送らない(地は CSS の変数だけ)。

import type { TranslucencyToken, TranslucencyTokens } from "../../logic/appearance/translucency";
import type { StyleTarget } from "./applyInterfaceMetrics";

/** 不透明度の変数を、変数を書く先(:root)に単位なしの数で書く。 */
export function applyTranslucency(root: StyleTarget, tokens: TranslucencyTokens): void {
  for (const [token, opacity] of Object.entries(tokens) as [TranslucencyToken, number][]) {
    root.style.setProperty(token, String(opacity));
  }
}
