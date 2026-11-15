import { type KeyboardEvent, useEffect, useRef } from "react";

import { moveMenuFocus } from "../../logic/menu/menuNavigation";
import { projectColorStyle } from "../design/palette";
import type { ProjectColor, TerminalTab, TerminalTabId } from "../snapshotTypes";
import { CloseIcon, PlusIcon } from "../tree/icons";

interface TabBarProps {
  /** 選択中のプロジェクトのタブ(タブの順)。 */
  tabs: readonly TerminalTab[];
  /** 表示中のタブ。0 枚なら null。 */
  activeId: TerminalTabId | null;
  /** プロジェクトの表示名(ツールチップの「プロジェクト名: タブ名」)。 */
  projectName: string;
  /** プロジェクト色(タブの着色。FR-060)。 */
  color: ProjectColor;
  /** 要素の id の頭(useId)。タブの id は tabDomId で作る。 */
  idBase: string;
  /** ターミナルの置き場所(tabpanel)の id。 */
  panelId: string;
  /** 新しいタブを開けるか(見つからないプロジェクトでは開けない。設計書 §8)。 */
  canAdd: boolean;
  onSelect: (id: TerminalTabId) => void;
  onClose: (id: TerminalTabId) => void;
  onNew: () => void;
}

/** タブの要素の id(tabpanel の aria-labelledby から指す)。ID に空白が入っても 1 語になるようにする。 */
export function tabDomId(idBase: string, terminalId: TerminalTabId): string {
  return `${idBase}-tab-${encodeURIComponent(terminalId)}`;
}

/** 左右の矢印・Home・End をメニューの移動のキーに読み替える(タブは横に並ぶ)。 */
const MOVE_KEYS: Readonly<Record<string, string>> = {
  ArrowRight: "ArrowDown",
  ArrowLeft: "ArrowUp",
  Home: "Home",
  End: "End",
};

/**
 * タブバー(FR-044・FR-060・FR-061。設計書 §8「タブ表示」、§9.1 の Presentational)。
 * 選択中のプロジェクトのタブを順に並べ、右端に「+」(新規ターミナル)を置く。値とコールバックだけを受け取る。
 *
 * - 見た目は macOS のタブにならう: タブの間は細い区切り線、表示中のタブだけ明るい地に浮かせ、ほかは控えめな文字の色。
 *   各タブの名前の前に、カードと同じプロジェクト色の小さな丸い印を出す(FR-060。地は塗らない)
 * - タブは WAI-ARIA の tablist / tab。表示中は aria-selected、明るい地と太字で分かる(色だけに頼らない)。タブ名の文字を必ず出す
 * - 閉じるボタンは、表示中のタブとマウスを載せたタブだけに見せる(macOS のタブと同じ。見えないときも押せる場所は変えない)
 * - キーボード: 表示中のタブだけが Tab で止まり、左右の矢印・Home・End でタブを移る(端で反対側に回る)。
 *   Enter・Space で選ぶ(手動の切替。移るたびに Rust へ選択を送らない)。Delete・Backspace でそのタブを閉じる
 * - 閉じるボタンはマウス用で Tab では止まらない(キーボードでは Delete か ⌘W)。読み上げの名前は「「タブ名」を閉じる」
 * - タブが多いときはタブの列だけが横にスクロールし、「+」は常に見える。表示中のタブが変わると、見える位置までスクロールする
 */
export function TabBar({
  tabs,
  activeId,
  projectName,
  color,
  idBase,
  panelId,
  canAdd,
  onSelect,
  onClose,
  onNew,
}: TabBarProps) {
  const tabRefs = useRef(new Map<TerminalTabId, HTMLButtonElement>());
  // Tab で止まるタブ: 表示中のタブ。表示中が並びに無ければ最初のタブ。
  const focusableId = tabs.some((t) => t.id === activeId) ? activeId : (tabs[0]?.id ?? null);

  useEffect(() => {
    if (activeId === null) return;
    tabRefs.current.get(activeId)?.scrollIntoView({ block: "nearest", inline: "nearest" });
  }, [activeId]);

  const onListKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const ids = tabs.map((t) => t.id);
    const current = ids.findIndex((id) => tabRefs.current.get(id) === document.activeElement);
    if (current < 0) return;
    if (event.key === "Delete" || event.key === "Backspace") {
      event.preventDefault();
      const id = ids[current];
      if (id === undefined) return;
      // 閉じるタブのボタンは写しで消えるので、先に隣のタブへフォーカスを移しておく(消えたあと body に落ちないように)。
      // 表示中のタブを閉じたときは、TerminalArea が次に表示するターミナルへ移し直す。
      const neighbor = ids[current + 1] ?? ids[current - 1];
      if (neighbor !== undefined) tabRefs.current.get(neighbor)?.focus();
      onClose(id);
      return;
    }
    const moveKey = MOVE_KEYS[event.key];
    if (moveKey === undefined) return;
    const next = moveMenuFocus(current, ids.length, moveKey);
    if (next === null) return;
    event.preventDefault();
    const id = ids[next];
    if (id !== undefined) tabRefs.current.get(id)?.focus();
  };

  return (
    <div className="tab-bar" data-testid="tab-bar">
      <div
        role="tablist"
        aria-label="ターミナルのタブ"
        aria-orientation="horizontal"
        className="tab-bar__list"
        onKeyDown={onListKeyDown}
      >
        {tabs.map((terminal) => {
          const selected = terminal.id === activeId;
          return (
            <div
              key={terminal.id}
              role="presentation"
              className={selected ? "tab-bar__item tab-bar__item--selected project-colored" : "tab-bar__item project-colored"}
              style={projectColorStyle(color)}
              data-testid="tab-item"
            >
              <button
                ref={(el) => {
                  if (el === null) tabRefs.current.delete(terminal.id);
                  else tabRefs.current.set(terminal.id, el);
                }}
                type="button"
                role="tab"
                id={tabDomId(idBase, terminal.id)}
                className="tab-bar__tab"
                aria-selected={selected}
                aria-controls={selected ? panelId : undefined}
                aria-keyshortcuts="Delete"
                tabIndex={terminal.id === focusableId ? 0 : -1}
                title={`${projectName}: ${terminal.label}`}
                onClick={() => {
                  if (!selected) onSelect(terminal.id);
                }}
              >
                <span className="project-dot tab-bar__dot" aria-hidden="true" />
                <span className="tab-bar__label">{terminal.label}</span>
              </button>
              <button
                type="button"
                className="tab-bar__close"
                aria-label={`「${terminal.label}」を閉じる`}
                title="タブを閉じる(⌘W)"
                tabIndex={-1}
                onClick={() => onClose(terminal.id)}
              >
                <CloseIcon />
              </button>
            </div>
          );
        })}
      </div>
      <button
        type="button"
        className="tab-bar__new"
        aria-label="新規ターミナル"
        aria-keyshortcuts="Meta+T"
        title="新規ターミナル(⌘T)"
        disabled={!canAdd}
        onClick={onNew}
      >
        <PlusIcon />
      </button>
    </div>
  );
}
