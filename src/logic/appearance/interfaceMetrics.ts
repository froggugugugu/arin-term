// 画面の文字の大きさの基準値から、追従させるデザイントークンの値を決める(M4.6。ADR-013 の 4)。
// 純粋な関数だけを置く。当てるのは src/ui/design/applyInterfaceMetrics.ts、ツリーの仮想スクロールも
// 同じ関数の treeRowHeight を使う(CSS から読まないので、見た目と窓の計算がずれない)。
//
// 追従させるのは文字の大きさと、文字を入れる行の高さだけ。余白・角丸・線・アイコン・幅の上下限は固定(【要確認】5 の推奨)。
// ターミナルの中の文字はこの値に追従しない(ターミナルの大きさは別の値)。

/** 基準値の既定(tokens.css の --font-size-l)。 */
export const INTERFACE_BASE_SIZE = 13;

/**
 * 追従させるトークンと、既定(基準値 13)での値(px)。tokens.css の :root の値と同じ(テストで確かめる)。
 * 値は基準値に比例させ、整数の px に丸める。
 */
export const INTERFACE_TOKEN_DEFAULTS = {
  // 文字の大きさ
  "--font-size-s": 11,
  "--font-size-m": 12,
  "--font-size-l": 13,
  "--font-size-xl": 15,
  // 文字を入れる行の高さ
  "--header-height": 30,
  "--tab-bar-height": 28,
  "--tile-title-height": 24,
  "--sidebar-item-height": 28,
  "--sidebar-section-height": 26,
  "--button-height": 22,
  "--tree-row-height": 22,
  "--tree-toolbar-height": 26,
  "--menu-item-height": 22,
  // ツリーの名前の入力欄の高さ
  "--tree-input-height": 18,
} as const;

export type InterfaceToken = keyof typeof INTERFACE_TOKEN_DEFAULTS;

export interface InterfaceMetrics {
  /** CSS の変数の名前 → 値(整数の px)。 */
  readonly tokens: Readonly<Record<InterfaceToken, number>>;
  /** ツリーの 1 行の高さ(px)。tokens の --tree-row-height と同じ。 */
  readonly treeRowHeight: number;
}

/**
 * 基準値(画面の文字の大きさ。CSS ピクセル)から、追従させるトークンの値を返す。
 * 各値は `round(既定の値 × size / 13)`。範囲(11〜18)は Rust が留めるのでここでは留めない。
 * 数でない・0 以下の値は既定(13)として扱う。
 */
export function interfaceMetrics(size: number): InterfaceMetrics {
  const base = Number.isFinite(size) && size > 0 ? size : INTERFACE_BASE_SIZE;
  const tokens = {} as Record<InterfaceToken, number>;
  for (const token of Object.keys(INTERFACE_TOKEN_DEFAULTS) as InterfaceToken[]) {
    tokens[token] = Math.round((INTERFACE_TOKEN_DEFAULTS[token] * base) / INTERFACE_BASE_SIZE);
  }
  return { tokens, treeRowHeight: tokens["--tree-row-height"] };
}
