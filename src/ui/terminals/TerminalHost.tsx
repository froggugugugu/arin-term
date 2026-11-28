import { type MouseEvent, useEffect, useRef, useState } from "react";

import { createCoalescer } from "../../logic/coalesce";
import { buildTerminalMenu, type TerminalMenuCommand } from "../../logic/menu/terminalMenu";
import type { TerminalId } from "../../terminal/engine";
import type { TerminalManager, TerminalMenuContext } from "../../terminal/TerminalManager";
import { ContextMenu } from "../menu/ContextMenu";
import { TERMINAL_FOCUS_REQUEST, terminalDropZoneAttributes } from "../tree/dropZones";

/** 置き場所の大きさの変化がこの時間だけ止んだら、寸法を合わせ直す(ウィンドウのドラッグ中に PTY へ寸法を送り続けない)。 */
export const RESIZE_SETTLE_MS = 50;

interface TerminalHostProps {
  manager: TerminalManager;
  terminalId: TerminalId;
}

/**
 * ターミナルの置き場所(設計書 §9.1)。置き場所の要素だけを描き、サーフェスの要素の付け外しは TerminalManager が行う。
 * React はこの要素の中身を管理しない(子を描かない)ので、再描画や部品の付け外しでターミナルが作り直されない(ADR-003)。
 * ツリーのドラッグの落とし先の目印(data-drop-zone・data-terminal-id)を付け、落としたあとのフォーカスの求め
 * (TERMINAL_FOCUS_REQUEST)を受けてこのターミナルにフォーカスを移す(T43d。xterm.js の要素には手を入れない)。
 * 真ん中クリックは xterm.js より先に(捕捉の段階で)受け、持っている選択を貼る。右クリックはコピー・ペーストのメニューを出す
 * (どちらも、マウスを使うアプリが動いているあいだは Option を押していなければアプリに渡す。ADR-020)。
 */
export function TerminalHost({ manager, terminalId }: TerminalHostProps) {
  const slotRef = useRef<HTMLDivElement>(null);
  const [menu, setMenu] = useState<{
    readonly terminalId: TerminalId;
    readonly x: number;
    readonly y: number;
    readonly context: TerminalMenuContext;
  } | null>(null);

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
    const onFocusRequest = () => manager.focus(terminalId);
    slot.addEventListener(TERMINAL_FOCUS_REQUEST, onFocusRequest);
    // xterm.js の要素(この中)より先に受けるため、捕捉の段階で聞く。受けたら xterm.js へは伝えない。
    const onMouseDown = (event: globalThis.MouseEvent) => {
      if (event.button !== 1) return;
      if (!manager.handleMiddleClick(terminalId, event.altKey)) return;
      event.preventDefault();
      event.stopPropagation();
    };
    slot.addEventListener("mousedown", onMouseDown, true);
    // 右クリックをアプリに渡すとき(マウスを使うアプリが動いていて Option を押していない)は、xterm.js の右クリックの
    // 語の選択もさせない(マウスの報告のモードを見ずに語を選び、持っている選択を上書きしてしまう)。
    // ここで伝わりを止めると文書の標準のメニューの抑止(contextMenuGuard)にも届かないので、既定の動作もここで止める。
    const onContextMenuCapture = (event: globalThis.MouseEvent) => {
      if (manager.terminalMenuFor(terminalId, event.altKey) !== null) return;
      event.preventDefault();
      event.stopPropagation();
    };
    slot.addEventListener("contextmenu", onContextMenuCapture, true);
    return () => {
      slot.removeEventListener("contextmenu", onContextMenuCapture, true);
      slot.removeEventListener("mousedown", onMouseDown, true);
      slot.removeEventListener(TERMINAL_FOCUS_REQUEST, onFocusRequest);
      observer.disconnect();
      refit.cancel();
      detach();
    };
  }, [manager, terminalId]);

  // 右クリックの単語の選択(xterm.js が中の要素で行う)が済んでから届く。
  const onContextMenu = (event: MouseEvent<HTMLDivElement>) => {
    event.preventDefault();
    const context = manager.terminalMenuFor(terminalId, event.altKey);
    if (context === null) return;
    setMenu({ terminalId, x: event.clientX, y: event.clientY, context });
  };
  const closeMenu = (restoreFocus: boolean) => {
    setMenu(null);
    if (restoreFocus) manager.focus(terminalId);
  };
  const runMenu = (command: TerminalMenuCommand) => {
    if (menu === null) return;
    closeMenu(true);
    void manager.runTerminalMenu(menu.terminalId, command, menu.context);
  };
  // 同じ部品に別のターミナルが渡されたら、前のターミナルのメニューは出さない。
  const shownMenu = menu !== null && menu.terminalId === terminalId ? menu : null;

  return (
    <>
      <div
        ref={slotRef}
        className="terminal-host"
        data-testid="terminal-host"
        onContextMenu={onContextMenu}
        {...terminalDropZoneAttributes(terminalId)}
      />
      {shownMenu !== null && (
        <ContextMenu
          key={`${shownMenu.x}:${shownMenu.y}`}
          label="ターミナル"
          entries={buildTerminalMenu(shownMenu.context.selection !== "")}
          x={shownMenu.x}
          y={shownMenu.y}
          onClose={closeMenu}
          onSelect={runMenu}
        />
      )}
    </>
  );
}
