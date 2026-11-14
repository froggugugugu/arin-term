// src/terminal/xterm は @xterm/* を import してよい唯一の場所。
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";

// 抽象(一つ上の src/terminal)への相対 import は許す。
import type { TerminalEngine } from "../engine";

export class XtermEngine implements TerminalEngine {
  private readonly term = new Terminal();
  private readonly fit = new FitAddon();

  write(data: string): void {
    this.term.write(data);
    this.fit.fit();
  }
}
