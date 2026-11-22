// src/editor/mermaid から src/ipc を import する見本。
export const load = () => import("mermaid");
import { saveDocument } from "../../ipc/client";

export const save = saveDocument;
