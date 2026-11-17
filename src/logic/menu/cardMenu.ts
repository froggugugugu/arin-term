// プロジェクトのカードの右クリックのメニュー(FR-003、FR-007、FR-059、FR-064、設計書 §9.1)。DOM を使わない純粋な関数。
//
// 画面(src/ui/menu)は、ここで組み立てた項目をそのまま映し、選ばれた項目の `command` を操作に渡す。
// src/logic は生成した型を import しないので、色の識別子はここで同じ値を定める(計画 M2 の【仮定】12。Rust の `ProjectColor`)。

/** プロジェクト色の識別子。この順がパレットの順。 */
export const PROJECT_COLOR_IDS = [
  "red",
  "orange",
  "yellow",
  "green",
  "teal",
  "blue",
  "purple",
  "pink",
] as const;

/** プロジェクト色の識別子。 */
export type ProjectColorId = (typeof PROJECT_COLOR_IDS)[number];

/** メニューに出す色の名前。色の値(ライト・ダーク)は src/ui/design/palette.ts が持つ。 */
const COLOR_LABELS: Readonly<Record<ProjectColorId, string>> = {
  red: "赤",
  orange: "オレンジ",
  yellow: "黄",
  green: "緑",
  teal: "青緑",
  blue: "青",
  purple: "紫",
  pink: "ピンク",
};

/** メニューを組み立てるのに要る、カードの値。 */
export interface CardMenuInput {
  /** ルートフォルダが見つからないか。 */
  readonly missing: boolean;
  /** いまの色。 */
  readonly color: ProjectColorId;
}

/** 選ばれたときの操作。対象のプロジェクトは、メニューを開いたカードのもの。 */
export type CardMenuCommand =
  | { readonly type: "copy-path" }
  | { readonly type: "set-color"; readonly color: ProjectColorId }
  | { readonly type: "relink" }
  | { readonly type: "remove" };

/** 色の下の 1 色。`checked` はいまの色の印。 */
export interface CardColorItem {
  readonly label: string;
  readonly checked: boolean;
  readonly command: { readonly type: "set-color"; readonly color: ProjectColorId };
}

/** メニューの 1 行。 */
export type CardMenuEntry =
  | { readonly kind: "command"; readonly label: string; readonly command: CardMenuCommand }
  | { readonly kind: "submenu"; readonly label: string; readonly items: readonly CardColorItem[] }
  | { readonly kind: "separator" };

const REMOVE: CardMenuEntry = { kind: "command", label: "一覧から削除", command: { type: "remove" } };
const SEPARATOR: CardMenuEntry = { kind: "separator" };

/**
 * カードの右クリックのメニューを組み立てる。
 * - ふつうのカード: 絶対パスをコピー / 色 ▸ 8 色 / 一覧から削除
 * - 見つからないカード: 再リンク… / 一覧から削除
 */
export function buildCardMenu(card: CardMenuInput): CardMenuEntry[] {
  if (card.missing) {
    return [{ kind: "command", label: "再リンク…", command: { type: "relink" } }, SEPARATOR, REMOVE];
  }
  const colors: CardColorItem[] = PROJECT_COLOR_IDS.map((color) => ({
    label: COLOR_LABELS[color],
    checked: color === card.color,
    command: { type: "set-color", color },
  }));
  return [
    { kind: "command", label: "絶対パスをコピー", command: { type: "copy-path" } },
    SEPARATOR,
    { kind: "submenu", label: "色", items: colors },
    SEPARATOR,
    REMOVE,
  ];
}
