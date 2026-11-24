// 並べ替えのドラッグ(プロジェクトのカードとタブ表示のタブ。ADR-019)。
//
// ツリーのドラッグ(src/ui/tree/useTreeDrag.ts、ADR-007 の (b))と同じく、HTML 標準のドラッグ&ドロップは使わず pointer の
// イベントで作る。並べ替えの規則(落とす位置・印の場所)は src/logic/dnd/reorder.ts。
//
// - 項目の要素に data-reorder-group(並べ替えの組。同じ組の中だけで動く)と data-reorder-id を付け、並びの要素の
//   pointerdown にこの onPointerDown を渡す。data-reorder-ignore の中(閉じるボタンなど)からは始めない
// - 左ボタンだけ。⌃ 付きのクリック(右クリックのメニュー)では始めない。しきい値(4px)を超えるまではクリックのまま
// - ドラッグ中は、掴んだ項目を浮かせてポインタに付けて並びの向きにだけ動かし(data-reorder-dragging)、落とす位置に
//   隙間が空くようにあいだの項目を掴んだ項目の大きさぶんずらす(data-reorder-shift。macOS のタブ・一覧と同じ)。
//   落とす位置は、ドラッグを始めたときの各項目の中心(並びのスクロールの量を足し引きする)で決める(ずらした位置では決めない)
// - 離したら、位置が変わるときだけ onReorder(組, ID, 動かしたあとの位置)を呼ぶ。結果は写しの並びで届く
// - 後片付けは 1 つの経路(離した・Esc・pointercancel・捕捉を失った・ウィンドウのフォーカスを失った・画面が隠れた・
//   ボタンを離したのに up が届かなかった・部品が外れた)。ドラッグのあとに届く click は 1 回だけ捨てる(タブやカードを選ばない)

import "./ReorderDrag.css";

import { type PointerEvent as ReactPointerEvent, useCallback, useEffect, useRef } from "react";

import { DRAG_THRESHOLD_PX } from "../../logic/dnd/dragMachine";
import { reorderIndex, shiftOf } from "../../logic/dnd/reorder";

export interface ReorderDragOptions {
  /** 並びの向き(縦の一覧は y、横のタブは x)。 */
  axis: "x" | "y";
  /** 位置が変わるように離した。toIndex は組の中で動かしたあとの位置。 */
  onReorder: (group: string, id: string, toIndex: number) => void;
}

/** ドラッグ中に html 要素に付ける印(カーソルを矢印に固定し、文字の選択を止める。ReorderDrag.css)。 */
const DRAGGING_CLASS = "is-reorder-dragging";

export function useReorderDrag({ axis, onReorder }: ReorderDragOptions) {
  const onReorderRef = useRef(onReorder);
  onReorderRef.current = onReorder;
  const cancelRef = useRef<(() => void) | null>(null);

  // 部品が外れたら取り消す。
  useEffect(() => () => cancelRef.current?.(), []);

  return useCallback(
    (event: ReactPointerEvent<HTMLElement>) => {
      if (event.button !== 0 || event.ctrlKey || !event.isPrimary || cancelRef.current !== null) return;
      const target = event.target as Element;
      if (target.closest("[data-reorder-ignore]") !== null) return;
      const container = event.currentTarget;
      const item = target.closest<HTMLElement>("[data-reorder-id]");
      if (item === null || !container.contains(item)) return;
      const group = item.dataset.reorderGroup ?? "";
      const id = item.dataset.reorderId ?? "";
      const members = [...container.querySelectorAll<HTMLElement>("[data-reorder-id]")].filter(
        (el) => el.dataset.reorderGroup === group,
      );
      const ids = members.map((el) => el.dataset.reorderId ?? "");
      const from = ids.indexOf(id);
      if (from < 0 || members.length < 2) return;

      const pointerId = event.pointerId;
      const coord = (e: { clientX: number; clientY: number }) => (axis === "x" ? e.clientX : e.clientY);
      const origin = { x: event.clientX, y: event.clientY };
      let dragging = false;
      let captured = false;
      let to = from;
      /** ドラッグを始めたときの各項目の中心と、掴んだ項目の大きさ(並びの向き)。 */
      let startCenters: number[] = [];
      let size = 0;
      const scroll = () => (axis === "x" ? container.scrollLeft : container.scrollTop);
      let startScroll = 0;

      const measure = () => {
        startScroll = scroll();
        startCenters = members.map((el) => {
          const rect = el.getBoundingClientRect();
          return axis === "x" ? rect.left + rect.width / 2 : rect.top + rect.height / 2;
        });
        const rect = item.getBoundingClientRect();
        size = axis === "x" ? rect.width : rect.height;
      };

      const translate = (offset: number) =>
        axis === "x" ? `translate3d(${offset}px, 0, 0)` : `translate3d(0, ${offset}px, 0)`;

      /** あいだの項目をずらして、落とす位置に隙間を空ける。 */
      const shift = () => {
        members.forEach((el, index) => {
          if (el === item) return;
          const direction = shiftOf(index, from, to);
          if (direction === 0) {
            el.removeAttribute("data-reorder-shift");
            el.style.removeProperty("transform");
          } else {
            el.setAttribute("data-reorder-shift", String(direction));
            el.style.transform = translate(direction * size);
          }
        });
      };

      const stop = (swallow: "now" | "after-release" | "keep") => {
        window.removeEventListener("pointermove", onMove, true);
        window.removeEventListener("pointerup", onUp, true);
        window.removeEventListener("pointercancel", onCancelEvent, true);
        window.removeEventListener("keydown", onKeyDown, true);
        window.removeEventListener("blur", onCancelEvent);
        window.removeEventListener("selectstart", onSelectStart, true);
        document.removeEventListener("visibilitychange", onVisibility);
        container.removeEventListener("lostpointercapture", onLostCapture);
        if (captured && container.hasPointerCapture(pointerId)) container.releasePointerCapture(pointerId);
        cancelRef.current = null;
        if (!dragging) return;
        dragging = false;
        // 先に印を外して戻りの動きを止めてから、ずらしを消す(離した位置から元の位置へ流れて見えないように)。
        document.documentElement.classList.remove(DRAGGING_CLASS);
        container.removeAttribute("data-reorder-to");
        for (const el of members) {
          el.removeAttribute("data-reorder-shift");
          el.removeAttribute("data-reorder-dragging");
          el.style.removeProperty("transform");
        }
        if (swallow === "now") swallowNextClick();
        else if (swallow === "after-release") swallowClickAfterRelease();
      };

      const cancel = () => stop("after-release");

      const onMove = (e: PointerEvent) => {
        if (e.pointerId !== pointerId) return;
        if ((e.buttons & 1) === 0) {
          cancel();
          return;
        }
        if (!dragging) {
          if (Math.hypot(e.clientX - origin.x, e.clientY - origin.y) <= DRAG_THRESHOLD_PX) return;
          dragging = true;
          try {
            container.setPointerCapture(pointerId);
            captured = true;
          } catch {
            // 捕捉できなくても window で受けている。
          }
          measure();
          document.documentElement.classList.add(DRAGGING_CLASS);
          window.getSelection()?.removeAllRanges();
          item.setAttribute("data-reorder-dragging", "true");
        }
        const delta = coord(e) - (axis === "x" ? origin.x : origin.y);
        item.style.transform = translate(delta);
        // 並びがスクロールしたら、そのぶん中心も動いている。
        const scrolled = scroll() - startScroll;
        const next = reorderIndex(
          startCenters.map((c) => c - scrolled),
          from,
          coord(e),
        );
        if (next !== to || !container.hasAttribute("data-reorder-to")) {
          to = next;
          container.setAttribute("data-reorder-to", String(to));
          shift();
        }
      };

      const onUp = (e: PointerEvent) => {
        if (e.pointerId !== pointerId) return;
        if (!dragging) {
          stop("keep");
          return;
        }
        const moved = to !== from;
        const toIndex = to;
        stop("now");
        if (moved) onReorderRef.current(group, id, toIndex);
      };

      const onCancelEvent = () => cancel();
      const onLostCapture = () => {
        if (dragging) cancel();
      };
      const onVisibility = () => {
        if (document.visibilityState === "hidden") cancel();
      };
      const onSelectStart = (e: Event) => {
        if (dragging) e.preventDefault();
      };
      const onKeyDown = (e: KeyboardEvent) => {
        if (e.key !== "Escape" || !dragging) return;
        e.preventDefault();
        e.stopPropagation();
        cancel();
      };

      window.addEventListener("pointermove", onMove, true);
      window.addEventListener("pointerup", onUp, true);
      window.addEventListener("pointercancel", onCancelEvent, true);
      window.addEventListener("keydown", onKeyDown, true);
      window.addEventListener("blur", onCancelEvent);
      window.addEventListener("selectstart", onSelectStart, true);
      document.addEventListener("visibilitychange", onVisibility);
      container.addEventListener("lostpointercapture", onLostCapture);
      cancelRef.current = cancel;
    },
    [axis],
  );
}

/** ドラッグを終えた直後に届く click を 1 回だけ捨てる(同じ処理の中で届かなければ捨てる口を外す)。 */
function swallowNextClick(): void {
  const swallow = (event: MouseEvent) => {
    event.stopPropagation();
    event.preventDefault();
    window.removeEventListener("click", swallow, true);
  };
  window.addEventListener("click", swallow, true);
  setTimeout(() => window.removeEventListener("click", swallow, true), 0);
}

/** 取り消したあと、ボタンを離したときの click を 1 回だけ捨てる(離す前に次の押下が来たら何もしない)。 */
function swallowClickAfterRelease(): void {
  const cleanup = () => {
    window.removeEventListener("pointerup", onUp, true);
    window.removeEventListener("pointerdown", cleanup, true);
  };
  const onUp = () => {
    cleanup();
    swallowNextClick();
  };
  window.addEventListener("pointerup", onUp, true);
  window.addEventListener("pointerdown", cleanup, true);
}
