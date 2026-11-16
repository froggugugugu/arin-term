// src/editor/codemirror は CodeMirror・lezer・Vim を import してよい唯一の場所。
import { EditorView, basicSetup } from "codemirror";
import { EditorState } from "@codemirror/state";
import { parser } from "@lezer/markdown";
import { vim } from "@replit/codemirror-vim";

// 抽象(一つ上の src/editor)への相対 import は許す。
import type { EditorSurface } from "../engine";

export class CodeMirrorEngine implements EditorSurface {
  private readonly view = new EditorView({ state: EditorState.create({ extensions: [basicSetup, vim()] }) });

  text(): string {
    void parser;
    return this.view.state.doc.toString();
  }
}
