// src/editor(抽象)でも CodeMirror の部品は直接使わない。動的 import も数える。
import { vim } from "@replit/codemirror-vim";

export const load = () => import("@codemirror/state");
export const mode = vim;
