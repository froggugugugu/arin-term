import { Terminal } from "@xterm/xterm";
import { sendInput } from "../../ipc/terminal";

export function attach(): void {
  const term = new Terminal();
  term.onData(sendInput);
}
