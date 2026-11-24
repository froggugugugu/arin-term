// src/editor(抽象)。Mermaid の描画は動的 import で読む(相対パスなので許す)。
export interface DiagramRenderer {
  render(source: string): Promise<string>;
}

export const loadDiagrams = () => import("./mermaid/MermaidRenderer");
