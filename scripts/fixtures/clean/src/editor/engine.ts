// src/editor(抽象)。CodeMirror の部品は動的 import で読む(相対パスなので許す)。
export interface EditorSurface {
  text(): string;
}

export const loadEngine = () => import("./codemirror/CodeMirrorEngine");
