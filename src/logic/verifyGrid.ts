// 9 枚の検証画面(T22)の「1 枚だけ表示」と「9 枚表示に戻す」の状態の計算。DOM も React も使わない。
//
// M1 の検証用(設計書 §2.2「9 枚の簡易確認」「非表示のインスタンス」)。M4 のタイル表示ではなく、T49 で外す。
// ターミナルの ID はここでは文字列として扱う(src/logic は生成した型を import しない)。

/** 表示の状態。`all` は全部を表示、`single` は 1 枚だけを表示して残りを隠す。 */
export type GridView = { readonly mode: "all" } | { readonly mode: "single"; readonly id: string };

/** 全部を表示している状態(最初の状態)。 */
export const SHOW_ALL: GridView = { mode: "all" };

/** `id` の 1 枚だけを表示する状態。一覧に無い ID なら全部を表示する(隠したまま戻せなくならないように)。 */
export function showOnly(ids: readonly string[], id: string): GridView {
  return ids.includes(id) ? { mode: "single", id } : SHOW_ALL;
}

/** `view` で `id` を表示するか。 */
export function isShown(view: GridView, id: string): boolean {
  return view.mode === "all" || view.id === id;
}

/** 表示を切り替える 1 件。 */
export interface VisibilityChange {
  readonly id: string;
  readonly visible: boolean;
}

/**
 * `from` から `to` に移るときに、表示を切り替えるターミナルの一覧(変わらないものは含めない)。
 * 隠すものを先、表示するものを後に並べる(WebGL の描画コンテキストを先に返してから作り直すため。同時に持てる数に上限がある)。
 * それぞれの中は `ids` の順。
 */
export function visibilityChanges(
  ids: readonly string[],
  from: GridView,
  to: GridView,
): VisibilityChange[] {
  const hide: VisibilityChange[] = [];
  const show: VisibilityChange[] = [];
  for (const id of ids) {
    const before = isShown(from, id);
    const after = isShown(to, id);
    if (before === after) continue;
    (after ? show : hide).push({ id, visible: after });
  }
  return [...hide, ...show];
}
