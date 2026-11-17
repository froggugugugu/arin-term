// src/editor/codemirror の外から CodeMirror を import する見本。
import { EditorView } from "@codemirror/view";
import { basicSetup } from "codemirror";
// src/ui から src/editor/codemirror への相対 import も違反
import { CodeMirrorEngine } from "../../editor/codemirror/CodeMirrorEngine";

export function Panel() {
  void EditorView;
  void basicSetup;
  void CodeMirrorEngine;
  return null;
}
