import "./ContextMenu.css";

import { type KeyboardEvent, useEffect, useLayoutEffect, useRef, useState } from "react";

import { menuKeyAction, moveMenuFocus } from "../../logic/menu/menuNavigation";

/** 下の階層(色 ▸ など)の 1 項目。`checked` はいまの値の印(menuitemradio)。 */
export interface ContextMenuChoice<C> {
  readonly label: string;
  readonly checked: boolean;
  readonly command: C;
}

/** メニューの 1 行。形は src/logic/menu/cardMenu.ts の CardMenuEntry と同じ。 */
export type ContextMenuEntry<C> =
  | { readonly kind: "command"; readonly label: string; readonly command: C }
  | { readonly kind: "submenu"; readonly label: string; readonly items: readonly ContextMenuChoice<C>[] }
  | { readonly kind: "separator" };

interface ContextMenuProps<C> {
  /** メニューの名前(読み上げ用)。 */
  label: string;
  entries: readonly ContextMenuEntry<C>[];
  /** 開いた位置(画面の CSS ピクセル)。ウィンドウからはみ出すときは内側へずらす。 */
  x: number;
  y: number;
  /** 項目が選ばれた。メニューは呼び出し側が閉じる。 */
  onSelect: (command: C) => void;
  /** 閉じる。`restoreFocus` は、開いたもの(カードなど)にフォーカスを戻すか(Esc・Tab。外をクリックしたときは戻さない)。 */
  onClose: (restoreFocus: boolean) => void;
}

/** ウィンドウの端から空ける量。 */
const EDGE = 4;

/** 位置を、大きさ (w, h) の箱がウィンドウに収まるようにずらす。 */
function clampToViewport(x: number, y: number, w: number, h: number): { left: number; top: number } {
  return {
    left: Math.max(EDGE, Math.min(x, window.innerWidth - w - EDGE)),
    top: Math.max(EDGE, Math.min(y, window.innerHeight - h - EDGE)),
  };
}

/**
 * 画面内のコンテキストメニュー(設計書 §9.1・§1.3 #14)。項目は渡されたものをそのまま映す。
 * WAI-ARIA の menu の型: 開くと最初の項目にフォーカスし、上下の矢印・Home・End で動き(端で回り込む)、Enter・Space で選ぶ。
 * 下の階層は右の矢印・Enter・クリック・ポインタを載せると開き、左の矢印・Esc で閉じる。Esc・Tab でメニューを閉じる。
 * メニューの外を押す・ウィンドウの大きさが変わる・ウィンドウが前面でなくなると閉じる。
 */
export function ContextMenu<C>({ label, entries, x, y, onSelect, onClose }: ContextMenuProps<C>) {
  const rootRef = useRef<HTMLDivElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const submenuRef = useRef<HTMLDivElement>(null);
  const itemRefs = useRef<(HTMLButtonElement | null)[]>([]);
  const choiceRefs = useRef<(HTMLButtonElement | null)[]>([]);
  // 選べる項目(区切りを除く)。フォーカスはこの並びの番号で持つ。
  const items = entries.filter((e): e is Exclude<ContextMenuEntry<C>, { kind: "separator" }> => e.kind !== "separator");
  const [focused, setFocused] = useState(0);
  /** 開いている下の階層(items の番号)と、その中のフォーカス。 */
  const [submenu, setSubmenu] = useState<{ index: number; focused: number | null } | null>(null);
  const [position, setPosition] = useState<{ left: number; top: number } | null>(null);
  const [submenuPosition, setSubmenuPosition] = useState<{ left: number; top: number } | null>(null);

  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;

  // 大きさを測ってから、ウィンドウに収まる位置に置く(測るまでは見えない位置で描く)。
  useLayoutEffect(() => {
    const menu = menuRef.current;
    if (menu === null) return;
    const rect = menu.getBoundingClientRect();
    setPosition(clampToViewport(x, y, rect.width, rect.height));
  }, [x, y]);

  // 下の階層は親の項目の右に、入らなければ左に置く。
  const submenuIndex = submenu?.index ?? null;
  useLayoutEffect(() => {
    if (submenuIndex === null || position === null) {
      setSubmenuPosition(null);
      return;
    }
    const sub = submenuRef.current;
    const parent = itemRefs.current[submenuIndex];
    const menu = menuRef.current;
    if (sub === null || parent === null || parent === undefined || menu === null) return;
    const subRect = sub.getBoundingClientRect();
    const menuRect = menu.getBoundingClientRect();
    const parentRect = parent.getBoundingClientRect();
    const right = menuRect.right - 2;
    const left = right + subRect.width + EDGE <= window.innerWidth ? right : menuRect.left - subRect.width + 2;
    setSubmenuPosition(clampToViewport(left, parentRect.top - 4, subRect.width, subRect.height));
  }, [submenuIndex, position]);

  // フォーカスを、いまの項目に移す。
  const submenuFocused = submenu?.focused ?? null;
  useEffect(() => {
    if (position === null) return;
    if (submenuFocused !== null && submenuPosition !== null) {
      choiceRefs.current[submenuFocused]?.focus();
    } else if (submenuFocused === null) {
      itemRefs.current[focused]?.focus();
    }
  }, [focused, submenuFocused, position, submenuPosition]);

  // 外を押す・大きさが変わる・前面でなくなると閉じる。
  useEffect(() => {
    const onPointerDown = (event: Event) => {
      if (event.target instanceof Node && rootRef.current?.contains(event.target)) return;
      onCloseRef.current(false);
    };
    const close = () => onCloseRef.current(false);
    document.addEventListener("pointerdown", onPointerDown, true);
    window.addEventListener("resize", close);
    window.addEventListener("blur", close);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown, true);
      window.removeEventListener("resize", close);
      window.removeEventListener("blur", close);
    };
  }, []);

  const openSubmenu = (index: number, focusInside: boolean) => {
    const entry = items[index];
    if (entry?.kind !== "submenu") return;
    const checked = entry.items.findIndex((item) => item.checked);
    setFocused(index);
    setSubmenu({ index, focused: focusInside ? Math.max(0, checked) : null });
  };

  const activate = (index: number) => {
    const entry = items[index];
    if (entry === undefined) return;
    if (entry.kind === "submenu") openSubmenu(index, true);
    else onSelect(entry.command);
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const subFocused = submenu?.focused ?? null;
    const inSubmenu = submenu !== null && subFocused !== null;
    const subEntry = submenu === null ? undefined : items[submenu.index];
    const subItems = subEntry?.kind === "submenu" ? subEntry.items : [];
    const moved = inSubmenu
      ? moveMenuFocus(subFocused, subItems.length, event.key)
      : moveMenuFocus(focused, items.length, event.key);
    if (moved !== null) {
      event.preventDefault();
      if (submenu !== null && inSubmenu) setSubmenu({ index: submenu.index, focused: moved });
      else {
        setFocused(moved);
        setSubmenu(null);
      }
      return;
    }
    const action = menuKeyAction(event.key, { inSubmenu });
    if (action === "none") return;
    event.preventDefault();
    event.stopPropagation();
    switch (action) {
      case "activate":
        if (inSubmenu) {
          const choice = subItems[subFocused];
          if (choice !== undefined) onSelect(choice.command);
        } else activate(focused);
        return;
      case "open-submenu":
        openSubmenu(focused, true);
        return;
      case "close-submenu":
        setSubmenu(null);
        return;
      case "close":
        onClose(true);
        return;
    }
  };

  const hiddenStyle = { left: 0, top: 0, visibility: "hidden" } as const;
  const openEntry = submenu === null ? undefined : items[submenu.index];
  let itemIndex = -1;

  return (
    <div ref={rootRef} className="context-menu-root" onKeyDown={onKeyDown} onContextMenu={(e) => e.preventDefault()}>
      <div
        ref={menuRef}
        role="menu"
        aria-label={label}
        className="context-menu"
        data-testid="context-menu"
        style={position ?? hiddenStyle}
      >
        {entries.map((entry, entryIndex) => {
          if (entry.kind === "separator") {
            return <div key={`sep-${entryIndex}`} role="separator" className="context-menu__separator" />;
          }
          itemIndex++;
          const index = itemIndex;
          const isSubmenu = entry.kind === "submenu";
          return (
            <button
              key={entry.label}
              ref={(el) => {
                itemRefs.current[index] = el;
              }}
              type="button"
              role="menuitem"
              tabIndex={index === focused ? 0 : -1}
              className={isSubmenu ? "context-menu__item context-menu__item--submenu" : "context-menu__item"}
              aria-haspopup={isSubmenu ? "menu" : undefined}
              aria-expanded={isSubmenu ? submenu?.index === index : undefined}
              onPointerEnter={() => {
                if (isSubmenu) openSubmenu(index, false);
                else {
                  setFocused(index);
                  setSubmenu(null);
                }
              }}
              onClick={() => activate(index)}
            >
              <span className="context-menu__label">{entry.label}</span>
            </button>
          );
        })}
      </div>
      {openEntry?.kind === "submenu" && (
        <div
          ref={submenuRef}
          role="menu"
          aria-label={openEntry.label}
          className="context-menu context-menu--submenu"
          style={submenuPosition ?? hiddenStyle}
        >
          {openEntry.items.map((choice, index) => (
            <button
              key={choice.label}
              ref={(el) => {
                choiceRefs.current[index] = el;
              }}
              type="button"
              role="menuitemradio"
              aria-checked={choice.checked}
              tabIndex={submenu?.focused === index ? 0 : -1}
              className="context-menu__item"
              onPointerEnter={() => setSubmenu((s) => (s === null ? s : { ...s, focused: index }))}
              onClick={() => onSelect(choice.command)}
            >
              <span className="context-menu__check" aria-hidden="true">
                {choice.checked ? "✓" : ""}
              </span>
              <span className="context-menu__label">{choice.label}</span>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
