// プロジェクト色の値(設計書 §9.2・§9.3、計画 M2 の【仮定】12)。Rust が持つのは色の識別子だけで、値はここが持つ。
//
// 1 色につきライト用とダーク用の 2 組を持つ。各組は次の 3 つ。
// - tint: 薄く着色する地(カードのタイトル部、ツリーのヘッダ、タブ。FR-060)
// - ink: tint の上の文字(コントラスト比 4.5 以上。FR-062)
// - accent: 小さな目印(カードの左の帯など。tint に対してコントラスト比 3 以上)
// 部品は projectColorStyle の CSS の変数を要素に付け、tokens.css の .project-tinted が OS の外観で使い分ける。
// ターミナル本文の背景には使わない(FR-062)。値は M6(T56)で両モードを人間が確かめて直してよい。

import type { CSSProperties } from "react";

import type { ProjectColorId } from "../../logic/menu/cardMenu";

/** 1 つの外観での色の組。値は `#rrggbb`。 */
export interface ProjectColorValues {
  readonly tint: string;
  readonly ink: string;
  readonly accent: string;
}

/** 1 色ぶんのライト・ダークの値。 */
export interface ProjectColorEntry {
  readonly light: ProjectColorValues;
  readonly dark: ProjectColorValues;
}

const LIGHT_INK = "#1d1d1f";
const DARK_INK = "#f5f5f7";

/** パレット(識別子の順 = パレットの順)。 */
export const PROJECT_PALETTE: Readonly<Record<ProjectColorId, ProjectColorEntry>> = {
  red: {
    light: { tint: "#fbdcd9", ink: LIGHT_INK, accent: "#c4001a" },
    dark: { tint: "#5a2b29", ink: DARK_INK, accent: "#ff8a82" },
  },
  orange: {
    light: { tint: "#fde4cc", ink: LIGHT_INK, accent: "#b33c00" },
    dark: { tint: "#5a3a1d", ink: DARK_INK, accent: "#ffb666" },
  },
  yellow: {
    light: { tint: "#f9eeb8", ink: LIGHT_INK, accent: "#8a5a00" },
    dark: { tint: "#4f4316", ink: DARK_INK, accent: "#f5d24a" },
  },
  green: {
    light: { tint: "#d5efd3", ink: LIGHT_INK, accent: "#1f7a35" },
    dark: { tint: "#244529", ink: DARK_INK, accent: "#6bdc85" },
  },
  teal: {
    light: { tint: "#cfeceb", ink: LIGHT_INK, accent: "#0b6f6a" },
    dark: { tint: "#1f4444", ink: DARK_INK, accent: "#5fd7d0" },
  },
  blue: {
    light: { tint: "#d8e6fa", ink: LIGHT_INK, accent: "#0a50c8" },
    dark: { tint: "#233a5c", ink: DARK_INK, accent: "#7fb5ff" },
  },
  purple: {
    light: { tint: "#e8ddf6", ink: LIGHT_INK, accent: "#7a3fa3" },
    dark: { tint: "#3d2d59", ink: DARK_INK, accent: "#cd9cff" },
  },
  pink: {
    light: { tint: "#f9dde9", ink: LIGHT_INK, accent: "#b8124a" },
    dark: { tint: "#562a42", ink: DARK_INK, accent: "#ff8ab3" },
  },
};

/** 要素に付ける CSS の変数(React の style にそのまま渡せる)。 */
export type ProjectColorStyle = CSSProperties &
  Readonly<Record<`--project-${"tint" | "ink" | "accent"}-${"light" | "dark"}`, string>>;

/** 色の識別子から、要素に付ける CSS の変数を作る。 */
export function projectColorStyle(color: ProjectColorId): ProjectColorStyle {
  const { light, dark } = PROJECT_PALETTE[color];
  return {
    "--project-tint-light": light.tint,
    "--project-ink-light": light.ink,
    "--project-accent-light": light.accent,
    "--project-tint-dark": dark.tint,
    "--project-ink-dark": dark.ink,
    "--project-accent-dark": dark.accent,
  };
}

/** `#rrggbb` の相対輝度(WCAG 2)。 */
function relativeLuminance(hex: string): number {
  const channel = (offset: number) => {
    const value = Number.parseInt(hex.slice(offset, offset + 2), 16) / 255;
    return value <= 0.03928 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * channel(1) + 0.7152 * channel(3) + 0.0722 * channel(5);
}

/** 2 色のコントラスト比(WCAG 2。1〜21)。 */
export function contrastRatio(a: string, b: string): number {
  const [hi, lo] = [relativeLuminance(a), relativeLuminance(b)].sort((x, y) => y - x) as [number, number];
  return (hi + 0.05) / (lo + 0.05);
}
