// src/terminal(抽象)は src/ipc を使ってよい。
import { ping } from "../ipc/client";

export interface TerminalEngine {
  write(data: string): void;
}

export const connect = ping;
