// src/editor/mermaid は mermaid を import してよい唯一の場所。本体は動的 import で読む。
import type { DiagramRenderer } from "../diagram";

export const load = () => import("mermaid");

export const renderer: DiagramRenderer | null = null;
