import { useState } from "react";

import type { PaneSizes } from "../../logic/panes/paneLayout";
import type { Actions } from "../../state/actions";

/** ペインの寸法の扱い(FR-009)。保存値は写しの `layout`。 */
export interface PaneSizesControl {
  /** いま描く寸法(保存値に、送ったが写しがまだ届いていない値と、ドラッグ中の値を重ねたもの)。 */
  readonly sizes: PaneSizes;
  /** ドラッグ中の値を描く(送らない)。null で取り消す。 */
  preview(sizes: Partial<Pick<PaneSizes, "sidebarWidth" | "cardAreaHeight">> | null): void;
  /** 左ペインの幅とカード領域の高さを送る(離したとき・キーで動かしたとき)。 */
  commit(sidebarWidth: number, cardAreaHeight: number): void;
  /** 左ペインを折りたたむ・戻す。 */
  setCollapsed(collapsed: boolean): void;
}

interface Pending {
  /** 送ったときの写しの寸法。次の写しが届いたら(別のものになったら)送った値は使わない。 */
  readonly base: PaneSizes;
  readonly sizes: Partial<PaneSizes>;
}

/**
 * 写しの寸法と、送った値・ドラッグ中の値を重ねる。送った値は Rust が最小寸法で直して写しで返すので、
 * 次の写しが届くまでのあいだだけ使う(離した瞬間に元の位置へ戻って見えないようにするため)。
 */
export function usePaneSizes(layout: PaneSizes, actions: Pick<Actions, "setPaneSizes" | "setSidebarCollapsed">): PaneSizesControl {
  const [pending, setPending] = useState<Pending | null>(null);
  const [dragging, setDragging] = useState<Partial<PaneSizes> | null>(null);
  const sent = pending !== null && pending.base === layout ? pending.sizes : {};
  const saved: PaneSizes = { ...layout, ...sent };
  const sizes: PaneSizes = dragging === null ? saved : { ...saved, ...dragging };

  const send = (next: Partial<PaneSizes>) => {
    setDragging(null);
    setPending({ base: layout, sizes: { ...sent, ...next } });
  };

  return {
    sizes,
    preview: setDragging,
    commit: (sidebarWidth, cardAreaHeight) => {
      send({ sidebarWidth, cardAreaHeight });
      void actions.setPaneSizes(sidebarWidth, cardAreaHeight);
    },
    setCollapsed: (collapsed) => {
      send({ sidebarCollapsed: collapsed });
      void actions.setSidebarCollapsed(collapsed);
    },
  };
}
