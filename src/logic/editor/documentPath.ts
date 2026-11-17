// エディタで開くファイルの見分け(ADR-015 の 5)。DOM を使わない純粋な関数。
// Rust の `arin_core::editor::is_markdown_path` と同じ規則(拡張子 `.md`・`.markdown`、大文字小文字を問わない)。
// 最後の決定は Rust がする(違えば open_document が not-allowed を返す)。ここは「開く」をエディタへ回すかの見分けだけ。

/** ルート基準の相対パスの名前が `.md`・`.markdown` で終わるか(`.md` のような点で始まる名前だけのものは除く)。 */
export function isMarkdownPath(path: string): boolean {
  const name = path.slice(path.lastIndexOf("/") + 1);
  const dot = name.lastIndexOf(".");
  if (dot <= 0) return false;
  const extension = name.slice(dot + 1).toLowerCase();
  return extension === "md" || extension === "markdown";
}
