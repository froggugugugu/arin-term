// プレビューの図(Mermaid)の抽象(ADR-017)。このファイルは mermaid を import しない(境界検査が見る)。
// 本物は src/editor/mermaid/MermaidRenderer.ts(EditorManager が動的 import で読む)。

/**
 * 図を描いた結果。成功ならライト用・ダーク用の 2 つの SVG の文字列(`<img>` で出す。画面に直接は入れない)。
 * 失敗なら理由の文字(文として出すだけ)。
 */
export type DiagramResult = { ok: true; light: string; dark: string } | { ok: false; message: string };

/** 図を描くもの。 */
export interface DiagramRenderer {
  render(source: string): Promise<DiagramResult>;
}

/** コードの塊の言語名(フェンスの info)が Mermaid か。最初の語だけを見て、大文字・小文字は区別しない。 */
export function isMermaidInfo(info: string): boolean {
  const word = info.trim().split(/\s+/, 1)[0] ?? "";
  return word.toLowerCase() === "mermaid";
}
