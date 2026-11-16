import { useEffect, useRef } from "react";
import { useStore } from "zustand";

import type { EditorId } from "../../editor/engine";
import type { EditorManager } from "../../editor/EditorManager";
import type { UiStore } from "../../state/uiStore";

interface EditorHostProps {
  editors: EditorManager;
  ui: UiStore;
  editorId: EditorId;
  /** プレビューだけの表示で隠しているか(部品は外さない。フォーカスの求めは受け取るだけにする)。 */
  hidden: boolean;
}

/**
 * エディタの部品の置き場所(ADR-015 の 1。src/ui/terminals/TerminalHost.tsx にならう)。置き場所の要素だけを描き、
 * 部品の要素の付け外しは EditorManager が行う(React は中身を管理しない。タブを替えても取り消しの履歴とカーソルが残る)。
 * 開いたあとのフォーカスの求め(uiStore の pendingEditorFocus)がこのエディタなら、次の描画のフレームでフォーカスを移して
 * 求めを消す(ツリーから開いたとき・確認を閉じたとき。TerminalArea の宛先の切替と同じく、WebKit が外し直さないようフレームを待つ)。
 */
export function EditorHost({ editors, ui, editorId, hidden }: EditorHostProps) {
  const slotRef = useRef<HTMLDivElement>(null);
  const pending = useStore(ui, (s) => s.pendingEditorFocus);

  useEffect(() => {
    const slot = slotRef.current;
    if (slot === null) return;
    return editors.attach(editorId, slot);
  }, [editors, editorId]);

  useEffect(() => {
    if (pending !== editorId) return;
    if (hidden) {
      ui.getState().consumeEditorFocus(editorId);
      return;
    }
    const frame = requestAnimationFrame(() => {
      // 確認ダイアログが開いていれば、そちらの操作の最中なので移さない(閉じたときに求め直される)
      if (document.querySelector("dialog[open]") !== null) return;
      editors.focus(editorId);
      ui.getState().consumeEditorFocus(editorId);
    });
    return () => cancelAnimationFrame(frame);
  }, [editors, ui, editorId, pending, hidden]);

  return <div ref={slotRef} className="editor-host" data-testid="editor-host" hidden={hidden} />;
}
