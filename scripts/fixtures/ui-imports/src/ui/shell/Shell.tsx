// 相対パスで src/ipc と src/terminal/xterm を越境する見本。
import { ping } from "../../ipc/client";
import { XtermEngine } from "../../terminal/xterm/XtermEngine";
export { helper } from "../../ipc";
// 抽象(src/terminal)は許す
import type { TerminalEngine } from "../../terminal/engine";

export function Shell(props: { engine: TerminalEngine }) {
  void ping;
  void XtermEngine;
  return props.engine ? null : null;
}
