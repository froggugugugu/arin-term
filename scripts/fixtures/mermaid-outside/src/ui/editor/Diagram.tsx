// src/editor/mermaid の外から mermaid を import する見本。
import mermaid from "mermaid";
// src/ui から src/editor/mermaid への相対 import も違反
import { load } from "../../editor/mermaid/MermaidRenderer";

export function Diagram() {
  void mermaid;
  void load;
  return null;
}
