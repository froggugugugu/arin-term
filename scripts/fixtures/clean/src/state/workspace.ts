// src/state は src/ipc を使ってよい。
import { ping } from "../ipc/client";

export function useWorkspace(): string {
  // 自前の関数名に fetch を含んでも、呼び出しが fetch( でなければ数えない
  const prefetchCount = 0;
  void ping;
  return `count=${prefetchCount}`;
}
