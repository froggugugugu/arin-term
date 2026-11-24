// 並べ替えのドラッグの落とし先(カードとタブ。ADR-019)。DOM を使わない純粋な関数。
//
// 画面(src/ui/shell/useReorderDrag.ts)は、ドラッグを始めたときの各項目の中心(縦の一覧なら y、横のタブなら x)を渡す。返す位置は「動かしたあとの位置」で、Rust の move_project・move_terminal_tab・
// move_editor_tab の toIndex にそのまま渡す。

/**
 * 動かしたあとの位置。掴んだ項目(from)を除いた項目のうち、中心がポインタより手前にあるものの数。
 * centers は並びの順の各項目の中心(掴んだ項目のぶんも含む。使わない)。
 */
export function reorderIndex(centers: readonly number[], from: number, pointer: number): number {
  let index = 0;
  centers.forEach((center, i) => {
    if (i !== from && center < pointer) index += 1;
  });
  return index;
}

/**
 * ドラッグ中に、掴んだ項目のぶんだけずらす向き(落とす位置に隙間を空ける。macOS のタブ・一覧と同じ)。
 * 並びの index 番目の項目について、前へずらすなら -1、後ろへずらすなら 1、動かさないなら 0。掴んだ項目自身は 0。
 */
export function shiftOf(index: number, from: number, to: number): -1 | 0 | 1 {
  if (index === from) return 0;
  if (from < to && index > from && index <= to) return -1;
  if (to < from && index >= to && index < from) return 1;
  return 0;
}
