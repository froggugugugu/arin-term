// 違反の無い見本。コメントの中の innerHTML や fetch( や import "../ipc/client" は数えない。
/*
 * import { Terminal } from "@xterm/xterm";
 * el.innerHTML = name; new WebSocket("ws://example");
 */
import { useState } from "react";

import type { TerminalEngine } from "../terminal/engine";
import { useWorkspace } from "../state/workspace";

const pattern = /[/*]+x/; // 正規表現リテラルの中の /* でコメント扱いを始めない。innerHTML
const url = "http://example.com/path"; // 文字列の中の // でコメント扱いを始めない

export function App(props: { engine?: TerminalEngine }) {
  const [label] = useState(`${url}`);
  const ws = useWorkspace();
  return (
    <main title={label} data-x={String(pattern.test(ws))}>
      {props.engine ? "あり" : "なし"}
    </main>
  );
}
