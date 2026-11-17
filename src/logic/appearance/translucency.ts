// 透過の値(% の整数。0 は不透明)から、地の色の不透明度を決める(M4.7。ADR-014 の 2)。
// 純粋な関数だけを置く。当てるのは src/ui/design/applyTranslucency.ts(interfaceMetrics と同じ分け方)。
//
// 色の RGB は tokens.css が持ち、ここは不透明度だけを決める。tokens.css の色は `rgba(…, var(--…-alpha))` で受ける。
// - ウィンドウの地(--color-window-bg): ライトは 1 − t/100、ダークは 1 − t/100 − 0.05(0 で留める)
// - ターミナルの置き場所の地(--color-terminal-bg。黒): 1 − t/100
// 範囲・刻みの規則は Rust(arin-core の layout/translucency.rs)が持つ。ここでは 0〜100 の外へ出さないことだけを守る。

/** 既定の透過(Rust の `DEFAULT_WINDOW_TRANSLUCENCY`・`DEFAULT_TERMINAL_TRANSLUCENCY`)。写しが届く前に使う。 */
export const TRANSLUCENCY_DEFAULTS = { window: 45, terminal: 20 } as const;

/** ダークの地をライトより少しだけ濃くする差(M4.6 までのライト 0.55・ダーク 0.5 の差)。 */
const DARK_WINDOW_OFFSET = 0.05;

/**
 * 当てる変数と、既定(ウィンドウ 45・ターミナル 20)での値。tokens.css の :root の値と同じ(テストで確かめる)。
 */
export const TRANSLUCENCY_TOKEN_DEFAULTS = {
  "--window-bg-alpha-light": 0.55,
  "--window-bg-alpha-dark": 0.5,
  "--terminal-bg-alpha": 0.8,
} as const;

export type TranslucencyToken = keyof typeof TRANSLUCENCY_TOKEN_DEFAULTS;

/** CSS の変数の名前 → 不透明度(0〜1。小数 2 桁)。 */
export type TranslucencyTokens = Readonly<Record<TranslucencyToken, number>>;

/** 透過の値を 0〜100 に収める(数でなければ fallback)。 */
function percent(value: number, fallback: number): number {
  const t = Number.isFinite(value) ? value : fallback;
  return Math.max(0, Math.min(100, t));
}

/** 不透明度を 0〜1 に留め、小数 2 桁に丸める(0.1 + 0.2 のような端数を CSS に書かない)。 */
function alpha(value: number): number {
  return Math.round(Math.max(0, Math.min(1, value)) * 100) / 100;
}

/** ウィンドウとターミナルの透過(%)から、当てる不透明度を返す。 */
export function translucencyTokens(windowTranslucency: number, terminalTranslucency: number): TranslucencyTokens {
  const windowOpacity = 1 - percent(windowTranslucency, TRANSLUCENCY_DEFAULTS.window) / 100;
  const terminalOpacity = 1 - percent(terminalTranslucency, TRANSLUCENCY_DEFAULTS.terminal) / 100;
  return {
    "--window-bg-alpha-light": alpha(windowOpacity),
    "--window-bg-alpha-dark": alpha(windowOpacity - DARK_WINDOW_OFFSET),
    "--terminal-bg-alpha": alpha(terminalOpacity),
  };
}
