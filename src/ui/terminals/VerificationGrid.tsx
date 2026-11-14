import "./VerificationGrid.css";

import { useEffect, useRef, useState } from "react";

import { type GridView, isShown, SHOW_ALL, showOnly, visibilityChanges } from "../../logic/verifyGrid";
import type { AppState } from "../../state/appStore";
import { describeTerminalStatus } from "../../state/terminalStatus";
import type { TerminalManager } from "../../terminal/TerminalManager";
import { RendererLabel } from "../shell/RendererLabel";
import { TerminalHost } from "./TerminalHost";

type TerminalItem = NonNullable<AppState["snapshot"]>["terminals"][number];

interface VerificationGridProps {
  terminals: TerminalManager;
  /** 写しにあるターミナル(9 枚の検証画面では 9 枚)。 */
  items: readonly TerminalItem[];
}

/**
 * 9 枚の検証画面(T22。設計書 §2.2「9 枚の簡易確認」「非表示のインスタンス」、NFR-009 の簡易確認)。
 * **M1 の検証用で、M4 のタイル表示ではない。M4(T49)で外す。**
 *
 * - 写しにあるターミナルを 3 × 3 に並べる。9 枚とも最初は表示した状態で作り、それぞれのマスの大きさで接続する
 * - 各マスの見出し: ターミナルの番号(押すとそのマスにフォーカス)、そのマスがいま描いている方式(dom / webgl)、
 *   終了・起動の失敗の表示、「1 枚だけ表示」(表示中は「9 枚表示に戻す」)のボタン
 * - 「1 枚だけ表示」: 選んだ 1 枚を全面に出し、残りを TerminalManager.setVisible(false) で隠す(描画の資源を手放し、
 *   出力の取り込みは続ける)。戻すと setVisible(true) で表示し、それぞれのマスの大きさに合わせ直す
 * - 隠している間は寸法を送らない(置き場所の大きさが 0 になるので、サーフェスの fit が何もしない)
 */
export function VerificationGrid({ terminals, items }: VerificationGridProps) {
  const [view, setView] = useState<GridView>(SHOW_ALL);
  const applied = useRef<GridView>(SHOW_ALL);
  const ids = items.map((item) => item.id);
  const idsKey = ids.join("\n");

  // 描き終えてから(マスの表示・非表示が画面に反映されてから)サーフェスの表示を切り替える。
  // 表示に戻すときの寸法合わせが、戻したマスの大きさを測れるようにするため。
  useEffect(() => {
    const list = idsKey === "" ? [] : idsKey.split("\n");
    for (const change of visibilityChanges(list, applied.current, view)) {
      terminals.setVisible(change.id, change.visible);
    }
    applied.current = view;
    if (view.mode === "single") terminals.focus(view.id);
  }, [terminals, idsKey, view]);

  return (
    <section className="verify-grid" data-testid="verify-grid" data-mode={view.mode} aria-label="ターミナル(検証画面)">
      {items.map((item, index) => {
        const shown = isShown(view, item.id);
        const status = describeTerminalStatus(item.status);
        const number = index + 1;
        return (
          <div
            key={item.id}
            className="verify-cell"
            data-testid="verify-cell"
            data-terminal-id={item.id}
            hidden={!shown}
          >
            <div className="verify-cell__header">
              <button
                type="button"
                className="verify-cell__number"
                data-testid="verify-cell-focus"
                title="このマスにフォーカス"
                onClick={() => terminals.focus(item.id)}
              >
                {number}
              </button>
              <RendererLabel terminals={terminals} terminalId={item.id} />
              {status !== null && (
                <p role="status" className="verify-cell__status" data-testid="terminal-status">
                  {status}
                </p>
              )}
              <button
                type="button"
                className="verify-cell__toggle"
                data-testid="verify-cell-toggle"
                onClick={() => setView(view.mode === "single" ? SHOW_ALL : showOnly(ids, item.id))}
              >
                {view.mode === "single" ? "9 枚表示に戻す" : "1 枚だけ表示"}
              </button>
            </div>
            <TerminalHost manager={terminals} terminalId={item.id} />
          </div>
        );
      })}
    </section>
  );
}
