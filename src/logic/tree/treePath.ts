// ツリーの中の位置を表す、ルート基準の相対パスの文字列の操作。
//
// 区切りは `/`、ルートは空の文字列(Rust の `RelPath` と同じ形。計画 T38a)。
// 届いた値の検査(`..`・空の要素など)は Rust 側が行う。ここでは検査済みの値を受け取る前提。

/** フォルダ `dir` の子 `name` の相対パス。 */
export function joinPath(dir: string, name: string): string {
  return dir === "" ? name : `${dir}/${name}`;
}

/** 親のフォルダの相対パス。ルートの親は無い(`null`)。 */
export function parentPath(path: string): string | null {
  if (path === "") {
    return null;
  }
  const slash = path.lastIndexOf("/");
  return slash < 0 ? "" : path.slice(0, slash);
}

/** 最後の要素の名前。ルートは空の文字列。 */
export function nameOf(path: string): string {
  return path.slice(path.lastIndexOf("/") + 1);
}

/** `path` が `ancestor` 自身かその子孫か。要素の単位で比べる(`src2` は `src` の子孫ではない)。 */
export function isSameOrDescendant(path: string, ancestor: string): boolean {
  return ancestor === "" || path === ancestor || path.startsWith(`${ancestor}/`);
}
