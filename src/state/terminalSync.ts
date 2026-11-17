// 写しをターミナルの管理(TerminalManager)へ伝える。main.tsx がつなぐ。
// State は src/terminal に依存しない(設計書 §3.1)ので、伝える先は形(TerminalSyncTarget)で受け取る。
//
// いまの写しと、写しが届くたびに、次の順で伝える。
// 1. 全ターミナルの状態(終わったシェルへ入力と寸法を送らないため)
// 2. 写しにあるターミナルの一覧(無くなったもの=タブを閉じた・シェルが終わってタブが閉じたものを破棄するため)
// 3. 見えるターミナルの並び(タブ表示と拡大中は 0〜1 枚、タイル表示は表示中のページのタイル)と、
//    隠して出力の取り込みだけを続けるもの(FR-042・FR-052、NFR-009。selectors.ts の presentation)
// その前に、ターミナルの文字の大きさが前に伝えたものと違えば伝える(M4.6。ADR-013 の 3。
// present で隠したまま作るサーフェスも新しい大きさで作るため)。

import type { TerminalId } from "../ipc/generated/TerminalId";
import type { TerminalStatus } from "../ipc/generated/TerminalStatus";
import type { AppStore } from "./appStore";
import { presentation } from "./selectors";
import type { AppSnapshot } from "../ipc/generated/AppSnapshot";

/** 伝える先(TerminalManager はこの形を満たす)。 */
export interface TerminalSyncTarget {
  setStatus(id: TerminalId, status: TerminalStatus): void;
  retain(ids: Iterable<TerminalId>): void;
  present(shown: readonly TerminalId[], background: readonly TerminalId[]): void;
  /** 全ターミナルの文字の大きさ(CSS ピクセル)。 */
  setFontSize(px: number): void;
}

/** 写しをターミナルの管理へ伝え続ける。外す関数を返す。写しがまだ無ければ届くまで何もしない。 */
export function syncTerminals(store: AppStore, target: TerminalSyncTarget): () => void {
  let forwardedFontSize: number | null = null;
  const forward = (snapshot: AppSnapshot | null) => {
    if (snapshot === null) return;
    const fontSize = snapshot.fontSizes.terminal.value;
    if (fontSize !== forwardedFontSize) {
      forwardedFontSize = fontSize;
      target.setFontSize(fontSize);
    }
    for (const terminal of snapshot.terminals) target.setStatus(terminal.id, terminal.status);
    target.retain(snapshot.terminals.map((t) => t.id));
    const { shown, background } = presentation(snapshot);
    target.present(shown, background);
  };
  forward(store.getState().snapshot);
  return store.subscribe((state, previous) => {
    if (state.snapshot !== previous.snapshot) forward(state.snapshot);
  });
}
