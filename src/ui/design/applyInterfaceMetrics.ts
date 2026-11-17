// 画面の文字の大きさ(M4.6。ADR-013 の 4)を画面に当てる 1 か所。
// - CSS: :root の変数を CSSOM の setProperty で上書きする(HTML の差し込みは使わない)。tokens.css の値は既定のまま残し、
//   写しが届く前と、当てられなかったときの値になる
// - ツリーの仮想スクロール: 同じ値を React の文脈で配る(FileTreeView は行の高さを CSS から読まない)

import { createContext } from "react";

import {
  INTERFACE_BASE_SIZE,
  type InterfaceMetrics,
  type InterfaceToken,
  interfaceMetrics,
} from "../../logic/appearance/interfaceMetrics";

/** 変数を書く先(document.documentElement はこの形を満たす)。 */
export interface StyleTarget {
  readonly style: { setProperty(name: string, value: string): void };
}

/** 追従させるトークンの値を、変数を書く先(:root)に px で書く。 */
export function applyInterfaceMetrics(root: StyleTarget, metrics: InterfaceMetrics): void {
  for (const [token, px] of Object.entries(metrics.tokens) as [InterfaceToken, number][]) {
    root.style.setProperty(token, `${px}px`);
  }
}

/** いまの画面の大きさの値(AppShell が写しから配る。配られていなければ既定の 13 の値)。 */
export const InterfaceMetricsContext = createContext<InterfaceMetrics>(interfaceMetrics(INTERFACE_BASE_SIZE));
