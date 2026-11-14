import { useEffect, useRef } from "react";

import { createCoalescer } from "../../logic/coalesce";
import type { TerminalId } from "../../terminal/engine";
import type { TerminalManager } from "../../terminal/TerminalManager";

/** 置き場所の大きさの変化がこの時間だけ止んだら、寸法を合わせ直す(ウィンドウのドラッグ中に PTY へ寸法を送り続けない)。 */
export const RESIZE_SETTLE_MS = 50;

interface TerminalHostProps {
  manager: TerminalManager;
  terminalId: TerminalId;
}

/**
 * ターミナルの置き場所(設計書 §9.1)。置き場所の要素だけを描き、サーフェスの要素の付け外しは TerminalManager が行う。
 * React はこの要素の中身を管理しない(子を描かない)ので、再描画や部品の付け外しでターミナルが作り直されない(ADR-003)。
 */
export function TerminalHost({ manager, terminalId }: TerminalHostProps) {
  const slotRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const slot = slotRef.current;
    if (slot === null) return;
    const detach = manager.attach(terminalId, slot);
    const refit = createCoalescer(
      () => manager.fit(terminalId),
      (run) => {
        const timer = setTimeout(run, RESIZE_SETTLE_MS);
        return () => clearTimeout(timer);
      },
    );
    const observer = new ResizeObserver(() => refit.request());
    observer.observe(slot);
    return () => {
      observer.disconnect();
      refit.cancel();
      detach();
    };
  }, [manager, terminalId]);

  return <div ref={slotRef} className="terminal-host" data-testid="terminal-host" />;
}
