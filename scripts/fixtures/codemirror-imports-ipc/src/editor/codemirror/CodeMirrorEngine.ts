// src/editor/codemirror から src/ipc を import する見本。
import { EditorView } from "@codemirror/view";
import { saveDocument } from "../../ipc/client";

export const view = EditorView;
export const save = saveDocument;
