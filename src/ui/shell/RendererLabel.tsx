import { useCallback, useSyncExternalStore } from "react";

import type { TerminalId } from "../../terminal/engine";
import type { TerminalManager } from "../../terminal/TerminalManager";

interface RendererLabelProps {
  terminals: TerminalManager;
  terminalId: TerminalId;
}

/**
 * いまどちらの描画方式(dom / webgl)で描いているかを小さく出す(T21)。
 * サーフェスが無ければ購読のときに作る(写しにあるターミナルにだけ使う)。
 * WebGL を指定したのに DOM に戻って動いていたことに、比べる人が気づけるようにするための 検証用の表示。M4 で外す(ADR-001)。
 */
export function RendererLabel({ terminals, terminalId }: RendererLabelProps) {
  const subscribe = useCallback(
    (onChange: () => void) => terminals.onRendererChange(terminalId, onChange),
    [terminals, terminalId],
  );
  // サーフェスは購読(描画のあと)で作られるので、最初の描画では undefined のことがある。
  const renderer = useSyncExternalStore(subscribe, () => terminals.rendererOf(terminalId));
  if (renderer === undefined) return null;
  return (
    <p className="app-shell__renderer" data-testid="terminal-renderer" title="ターミナルの描画方式(M1 の検証用)">
      {renderer}
    </p>
  );
}
