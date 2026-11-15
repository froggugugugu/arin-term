// ツリーの項目のドラッグ(FR-016、FR-029、FR-037、ADR-007 の (b)、計画 T43d)。
//
// HTML 標準のドラッグ&ドロップは使わず、pointer のイベントを src/logic/dnd の DragMachine につなぐ:
// - ツリーの pointerdown(行の上。左ボタンだけ、名前の入力中の行は除く)で press。以後の move・up・cancel は window で受ける
//   (しきい値を超える前にカーソルがツリーの外へ出ても取りこぼさない)
// - しきい値(4px)を超えたら drag-start: ツリーの要素に setPointerCapture(行は仮想スクロールで外れることがあるので、
//   押した行ではなくツリーの要素に付ける。【仮定】)、uiStore.setTreeDragging(true)(ツリーの更新を保留する。FR-037)
// - ドラッグ中は、カーソルの下の要素(document.elementFromPoint と data- 属性。./dropZones.ts)から落とし先を決め、
//   resolveDropTarget で落とせる先だけを強調する。⌥ は pointer と key のイベントの altKey で追う
// - 離したら release の結果(move・copy・insert・none)を onDrop に渡してから、後片付けをする
//
// 後片付け(stop)は 1 つの経路にまとめる: 離した・Esc・pointercancel・ポインタの捕捉を失った・ウィンドウのフォーカスを
// 失った・画面が隠れた・ボタンを離したのに up が届かなかった(移動のイベントの buttons が 0)・部品が外れた。どれでも、
// window の受け口を外し、捕捉を放し、強調と写しを消し、ドラッグ中だったら setTreeDragging(false) に戻す。
// ドラッグのあとに続いて届く click は 1 回だけ捨てる(行の選択や余白のクリックでの選択の解除にしない)。

import { type PointerEvent as ReactPointerEvent, type RefObject, useCallback, useEffect, useRef, useState } from "react";

import { DragMachine, type Point } from "../../logic/dnd/dragMachine";
import { type DropResult, resolveDropTarget } from "../../logic/dnd/dropTarget";
import { nameOf } from "../../logic/tree/treePath";
import { dropHitAt, type ResolvedHit } from "./dropZones";

/** ドラッグしている項目(写しに出す)。 */
export interface DragItem {
  readonly path: string;
  readonly name: string;
  readonly folder: boolean;
}

/** ドラッグ中の表示の状態(落とし先が変わったときだけ変わる。写しの位置は ref で直接動かす)。 */
export interface TreeDragView {
  readonly item: DragItem;
  /** 強調するツリーの中のフォルダ(ルートは空の文字列)。ツリーの中に落とせないときは null。 */
  readonly target: string | null;
  /** いま離すとコピーになるか(写しに「+」を付ける)。 */
  readonly copy: boolean;
}

interface TreeDragOptions {
  treeRef: RefObject<HTMLElement | null>;
  /** ドラッグが始まった(項目を選ぶ)。 */
  onDragStart: (path: string) => void;
  /** 離した。terminalElement は落とした先のターミナルの置き場所(insert のとき)。 */
  onDrop: (result: DropResult, terminalElement: HTMLElement | null) => void;
  setDragging: (dragging: boolean) => void;
}

/** ドラッグ中に html 要素に付ける印(カーソルを矢印に固定し、文字の選択を止める。TreeDrag.css)。 */
const DRAGGING_CLASS = "is-tree-dragging";
/**
 * カーソルから写しの左上までのずれ。写しをカーソルの右下に出し、カーソルの下の落とし先の名前を隠さない
 * (Finder は掴んだ位置に重ねるが、Finder の写しより不透明なので、落とし先が読めるようにずらす。【仮定】)。
 */
const GHOST_OFFSET: Point = { x: 10, y: 12 };

/** ドラッグのあとの click の扱い。 */
type ClickAfterDrag = "keep" | "swallow-now" | "swallow-after-release";

interface Session {
  cancel: () => void;
}

export function useTreeDrag({ treeRef, onDragStart, onDrop, setDragging }: TreeDragOptions) {
  const [view, setView] = useState<TreeDragView | null>(null);
  const ghostRef = useRef<HTMLDivElement>(null);
  /** 写しの左上(画面の CSS ピクセル)。描き直しでも位置が戻らないよう、描くときにもここを読む。 */
  const ghostPositionRef = useRef<Point>({ x: 0, y: 0 });
  const sessionRef = useRef<Session | null>(null);
  const callbacksRef = useRef({ onDragStart, onDrop, setDragging });
  callbacksRef.current = { onDragStart, onDrop, setDragging };

  // 部品が外れたら(プロジェクトの切替など)取り消す。
  useEffect(() => () => sessionRef.current?.cancel(), []);

  const onPointerDown = useCallback(
    (event: ReactPointerEvent<HTMLElement>) => {
      if (event.button !== 0 || event.ctrlKey || !event.isPrimary || sessionRef.current !== null) return;
      const tree = treeRef.current;
      const target = event.target as Element;
      if (tree === null || target.closest("[data-tree-name-input]") !== null) return;
      const rowElement = target.closest<HTMLElement>('[data-testid="tree-row"]');
      if (rowElement === null || !tree.contains(rowElement) || rowElement.classList.contains("tree-row--editing")) return;
      const path = rowElement.dataset.path ?? "";
      if (path === "") return;
      const kind = rowElement.dataset.kind;
      const item: DragItem = { path, name: nameOf(path), folder: kind === "directory" || kind === "directoryLink" };

      const pointerId = event.pointerId;
      const machine = new DragMachine();
      machine.press({ path }, { x: event.clientX, y: event.clientY });

      let dragging = false;
      let captured = false;
      let alt = event.altKey;
      let last: Point = { x: event.clientX, y: event.clientY };
      let hit: ResolvedHit | null = null;
      let activeTerminal: HTMLElement | null = null;

      const placeGhost = () => {
        const position = { x: last.x + GHOST_OFFSET.x, y: last.y + GHOST_OFFSET.y };
        ghostPositionRef.current = position;
        const ghost = ghostRef.current;
        if (ghost !== null) ghost.style.transform = `translate3d(${position.x}px, ${position.y}px, 0)`;
      };

      /** 今の落とし先と ⌥ から、強調と写しの印を決める(落とせない先では強調しない)。 */
      const show = () => {
        const result: DropResult =
          hit === null ? { type: "none", reason: "no-target" } : resolveDropTarget({ path }, hit.hit, alt);
        const folder = result.type === "move" || result.type === "copy" ? result.destination : null;
        const terminal = result.type === "insert" ? (hit?.terminalElement ?? null) : null;
        if (terminal !== activeTerminal) {
          activeTerminal?.removeAttribute("data-drop-active");
          terminal?.setAttribute("data-drop-active", "true");
          activeTerminal = terminal;
        }
        const copy = result.type === "copy";
        setView((prev) => (prev !== null && prev.target === folder && prev.copy === copy ? prev : { item, target: folder, copy }));
      };

      /** 後片付け(どの終わり方でもここを通る)。 */
      const stop = (click: ClickAfterDrag) => {
        window.removeEventListener("pointermove", onMove, true);
        window.removeEventListener("pointerup", onUp, true);
        window.removeEventListener("pointercancel", onCancelEvent, true);
        window.removeEventListener("keydown", onKeyDown, true);
        window.removeEventListener("keyup", onKeyUp, true);
        window.removeEventListener("blur", onCancelEvent);
        window.removeEventListener("selectstart", onSelectStart, true);
        document.removeEventListener("visibilitychange", onVisibility);
        tree.removeEventListener("lostpointercapture", onLostCapture);
        if (captured && tree.hasPointerCapture(pointerId)) tree.releasePointerCapture(pointerId);
        activeTerminal?.removeAttribute("data-drop-active");
        activeTerminal = null;
        sessionRef.current = null;
        if (!dragging) return;
        dragging = false;
        document.documentElement.classList.remove(DRAGGING_CLASS);
        setView(null);
        callbacksRef.current.setDragging(false);
        if (click === "swallow-now") swallowNextClick();
        else if (click === "swallow-after-release") swallowClickAfterRelease();
      };

      const cancel = () => {
        machine.cancel();
        // ボタンはまだ押されていることが多い。離したときの click を捨てる(余白のクリックとして選択を外さない)。
        stop("swallow-after-release");
      };

      const onMove = (e: PointerEvent) => {
        if (e.pointerId !== pointerId) return;
        // ボタンを離したのに up が届かなかった(ウィンドウの外で離したなど)。
        if ((e.buttons & 1) === 0) {
          cancel();
          return;
        }
        last = { x: e.clientX, y: e.clientY };
        alt = e.altKey;
        if (machine.move(last).type === "drag-start") {
          dragging = true;
          try {
            tree.setPointerCapture(pointerId);
            captured = true;
          } catch {
            // 捕捉できなくても window で受けている。
          }
          document.documentElement.classList.add(DRAGGING_CLASS);
          window.getSelection()?.removeAllRanges();
          callbacksRef.current.setDragging(true);
          callbacksRef.current.onDragStart(path);
        }
        if (!machine.isDragging) return;
        placeGhost();
        hit = dropHitAt(last.x, last.y, tree);
        show();
      };

      const onUp = (e: PointerEvent) => {
        if (e.pointerId !== pointerId) return;
        if (!dragging) {
          // しきい値を超えなかった。クリックとして画面の既定の動き(行の onClick)に任せる。
          machine.release(null, false);
          stop("keep");
          return;
        }
        const resolved = dropHitAt(e.clientX, e.clientY, tree);
        const outcome = machine.release(resolved?.hit ?? null, e.altKey);
        // 先に操作を始めてから保留を解く(移動の確認が開いてから。確認の表示中も保留の理由。計画【仮定】18)。
        if (outcome.type === "drop") callbacksRef.current.onDrop(outcome.result, resolved?.terminalElement ?? null);
        stop("swallow-now");
      };

      const onCancelEvent = () => cancel();
      const onLostCapture = () => {
        if (machine.isDragging) cancel();
      };
      const onVisibility = () => {
        if (document.visibilityState === "hidden") cancel();
      };
      const onSelectStart = (e: Event) => e.preventDefault();
      const onKeyDown = (e: KeyboardEvent) => {
        if (e.key === "Escape") {
          if (!dragging) return;
          e.preventDefault();
          e.stopPropagation();
          cancel();
          return;
        }
        if (e.key === "Alt" && dragging) {
          alt = true;
          show();
        }
      };
      const onKeyUp = (e: KeyboardEvent) => {
        if (e.key === "Alt" && dragging) {
          alt = false;
          show();
        }
      };

      window.addEventListener("pointermove", onMove, true);
      window.addEventListener("pointerup", onUp, true);
      window.addEventListener("pointercancel", onCancelEvent, true);
      window.addEventListener("keydown", onKeyDown, true);
      window.addEventListener("keyup", onKeyUp, true);
      window.addEventListener("blur", onCancelEvent);
      window.addEventListener("selectstart", onSelectStart, true);
      document.addEventListener("visibilitychange", onVisibility);
      tree.addEventListener("lostpointercapture", onLostCapture);
      sessionRef.current = { cancel };
    },
    [treeRef],
  );

  return { view, ghostRef, ghostPositionRef, onPointerDown };
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

/**
 * 取り消したあと、ボタンを離したときの click を 1 回だけ捨てる。離す前に次の押下が来たら(ウィンドウの外で離した
 * などで up が届かなかった)何もしない(利用者の次のクリックを捨てない)。
 */
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
