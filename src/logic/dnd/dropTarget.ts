// ドロップ先の判定(FR-016、FR-029、設計書 §5.1 の `resolveDropTarget`、ADR-007 の (b))。DOM を使わない純粋な関数。
//
// 画面(src/ui/tree。計画 T43d)は、離した位置の要素の `data-` 属性からドロップ先(`DropHit`)を作って渡す。
// パスはルート基準の相対パス(区切りは `/`、空の文字列はルート)。

/** ドラッグしている項目。ルートはドラッグできない(FR-035)。 */
export interface DragSource {
  /** ルート基準の相対パス(空でない)。 */
  readonly path: string;
}

/**
 * 離した位置にあるドロップ先。
 * - `folder`: フォルダの行(【仮定】フォルダへのリンクの行は展開しないので `file` として渡す)
 * - `file`: ファイルの行(その親のフォルダに落とす)
 * - `root`: ツリーのヘッダ(ルート)
 * - `terminal`: ターミナルの置き場所(パスの挿入)
 */
export type DropHit =
  | { readonly kind: "folder"; readonly path: string }
  | { readonly kind: "file"; readonly path: string }
  | { readonly kind: "root" }
  | { readonly kind: "terminal"; readonly terminalId: string };

/**
 * 落としたときの操作。
 * - `move`・`copy`: `source` を `destination`(フォルダの相対パス)の中へ移す・写す。⌥ を押して離すとコピー
 * - `insert`: `source` のパスをターミナルへ挿入する
 * - `none`: 何もしない(`no-target`: 落とし先が無い、`self-or-descendant`: 自分自身・子孫、`same-parent`: 同じ親のフォルダ)
 */
export type DropResult =
  | { readonly type: "move"; readonly source: string; readonly destination: string }
  | { readonly type: "copy"; readonly source: string; readonly destination: string }
  | { readonly type: "insert"; readonly source: string; readonly terminalId: string }
  | { readonly type: "none"; readonly reason: "no-target" | "self-or-descendant" | "same-parent" };

function parentOf(path: string): string {
  const slash = path.lastIndexOf("/");
  return slash < 0 ? "" : path.slice(0, slash);
}

/** `path` が `ancestor` 自身かその子孫か(要素ごとに比べる。`src` と `src2` は別)。 */
function isSelfOrDescendant(path: string, ancestor: string): boolean {
  return path === ancestor || path.startsWith(`${ancestor}/`);
}

/**
 * ドラッグした項目を `hit` に落としたときの操作を決める。
 * フォルダの行はそのフォルダ、ファイルの行はその親、ヘッダはルート、ターミナルの置き場所は挿入。
 * 自分自身と子孫には落とせない。同じ親のフォルダに落としても何もしない(【仮定】⌥ のコピーでも。複製は M6)。
 */
export function resolveDropTarget(source: DragSource, hit: DropHit | null, altKey: boolean): DropResult {
  if (hit === null) return { type: "none", reason: "no-target" };
  if (hit.kind === "terminal") return { type: "insert", source: source.path, terminalId: hit.terminalId };
  const destination = hit.kind === "folder" ? hit.path : hit.kind === "file" ? parentOf(hit.path) : "";
  if (isSelfOrDescendant(destination, source.path)) return { type: "none", reason: "self-or-descendant" };
  if (destination === parentOf(source.path)) return { type: "none", reason: "same-parent" };
  return { type: altKey ? "copy" : "move", source: source.path, destination };
}
