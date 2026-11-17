// 名称変更の入力欄で最初に選ぶ範囲(FR-026、【仮定】10、計画 T41a)。DOM を使わない純粋な関数。

import type { TreeEntryKind } from "./types";

/** 選ぶ範囲。位置は入力欄の `setSelectionRange` と同じ UTF-16 の単位。 */
export interface SelectionRange {
  readonly start: number;
  readonly end: number;
}

/**
 * ファイル(とファイルへのリンク・その他)は最後の `.` の前まで(`a.tar.gz` は `a.tar`)。
 * `.` で始まり他に `.` が無い名前(`.env`)、拡張子の無い名前、フォルダ(とフォルダへのリンク)は全体。
 */
export function renameSelectionRange(name: string, kind: TreeEntryKind): SelectionRange {
  const whole = { start: 0, end: name.length };
  if (kind === "directory" || kind === "directoryLink") {
    return whole;
  }
  const dot = name.lastIndexOf(".");
  return dot > 0 ? { start: 0, end: dot } : whole;
}
