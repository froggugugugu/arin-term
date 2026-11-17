import { type KeyboardEvent, useEffect, useRef } from "react";

import { ariaKeyShortcuts, type ShortcutTable, shortcutLabel } from "../../logic/keys/shortcutTable";
import { moveMenuFocus } from "../../logic/menu/menuNavigation";
import { projectColorStyle } from "../design/palette";
import type { EditorTab, EditorTabId, ProjectColor, TerminalTab, TerminalTabId } from "../snapshotTypes";
import { CloseIcon, FileIcon, PlusIcon } from "../tree/icons";

interface TabBarProps {
  /** 選択中のプロジェクトのタブ(タブの順)。 */
  tabs: readonly TerminalTab[];
  /** 表示中のタブ。0 枚なら null。エディタを出しているとき(shownEditorId)はターミナルのタブは選ばれていない。 */
  activeId: TerminalTabId | null;
  /** 選択中のプロジェクトのエディタのタブ(開いた順。M4.8)。ターミナルのタブの後ろに並べる。 */
  editorTabs?: readonly EditorTab[];
  /** 出しているエディタ。無ければ null。 */
  shownEditorId?: EditorTabId | null;
  onSelectEditor?: (id: EditorTabId) => void;
  onCloseEditor?: (id: EditorTabId) => void;
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
  /** ショートカットの表(「+」と閉じるボタンのツールチップ・読み上げのキーを引く。部品にキーの文字列を書かない)。 */
  shortcuts: ShortcutTable;
  onSelect: (id: TerminalTabId) => void;
  onClose: (id: TerminalTabId) => void;
  onNew: () => void;
}

/** タブの要素の id(tabpanel の aria-labelledby から指す)。ID に空白が入っても 1 語になるようにする。 */
export function tabDomId(idBase: string, terminalId: TerminalTabId): string {
  return `${idBase}-tab-${encodeURIComponent(terminalId)}`;
}

const TERMINAL_PREFIX = "terminal:";
const EDITOR_PREFIX = "editor:";
/** タブの並びの中の目印(ターミナルとエディタの ID が重なっても区別する)。 */
const terminalKey = (id: TerminalTabId) => `${TERMINAL_PREFIX}${id}`;
const editorKey = (id: EditorTabId) => `${EDITOR_PREFIX}${id}`;

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
 * - 「+」のツールチップ(「新規ターミナル(⌘T)」)と読み上げのキー(aria-keyshortcuts)、閉じるボタンのツールチップのキーは
 *   表の terminal.new-tab・terminal.close-tab から引く(設定で替えると追従する。読めなければキーを出さない)
 * - エディタのタブ(M4.8。ADR-015 の 3・8)はターミナルのタブの後ろに開いた順で並ぶ。プロジェクト色の丸の代わりに文書の印を出し、
 *   未保存なら閉じるボタンの位置に「●」(マウスを載せると ×。macOS と同じ)。読み上げはタブの名前に「未保存」を足す。
 *   キーボードの移動・Delete はターミナルのタブと 1 つの並びとして扱う
 */
export function TabBar({
  tabs,
  activeId,
  editorTabs = [],
  shownEditorId = null,
  onSelectEditor,
  onCloseEditor,
  projectName,
  color,
  idBase,
  panelId,
  canAdd,
  shortcuts,
  onSelect,
  onClose,
  onNew,
}: TabBarProps) {
  const newKey = shortcutLabel(shortcuts, "terminal.new-tab");
  const newAriaKey = ariaKeyShortcuts(shortcuts, "terminal.new-tab");
  const closeKey = shortcutLabel(shortcuts, "terminal.close-tab");
  const tabRefs = useRef(new Map<string, HTMLButtonElement>());
  // 表示中のタブ: エディタを出していればそのエディタ、無ければ表示中のターミナル。
  const selectedKey =
    shownEditorId !== null && editorTabs.some((t) => t.id === shownEditorId)
      ? editorKey(shownEditorId)
      : activeId === null
        ? null
        : terminalKey(activeId);
  const keys = [...tabs.map((t) => terminalKey(t.id)), ...editorTabs.map((t) => editorKey(t.id))];
  // Tab で止まるタブ: 表示中のタブ。表示中が並びに無ければ最初のタブ。
  const focusableKey = selectedKey !== null && keys.includes(selectedKey) ? selectedKey : (keys[0] ?? null);

  useEffect(() => {
    if (selectedKey === null) return;
    tabRefs.current.get(selectedKey)?.scrollIntoView({ block: "nearest", inline: "nearest" });
  }, [selectedKey]);

  const closeByKey = (key: string) => {
    if (key.startsWith(EDITOR_PREFIX)) onCloseEditor?.(key.slice(EDITOR_PREFIX.length));
    else onClose(key.slice(TERMINAL_PREFIX.length));
  };
  const refOf = (key: string) => (el: HTMLButtonElement | null) => {
    if (el === null) tabRefs.current.delete(key);
    else tabRefs.current.set(key, el);
  };

  const onListKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const ids = keys;
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
      closeByKey(id);
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
          const key = terminalKey(terminal.id);
          const selected = key === selectedKey;
          return (
            <div
              key={terminal.id}
              role="presentation"
              className={selected ? "tab-bar__item tab-bar__item--selected project-colored" : "tab-bar__item project-colored"}
              style={projectColorStyle(color)}
              data-testid="tab-item"
            >
              <button
                ref={refOf(key)}
                type="button"
                role="tab"
                id={tabDomId(idBase, terminal.id)}
                className="tab-bar__tab"
                aria-selected={selected}
                aria-controls={selected ? panelId : undefined}
                aria-keyshortcuts="Delete"
                tabIndex={key === focusableKey ? 0 : -1}
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
                title={closeKey === null ? "タブを閉じる" : `タブを閉じる(${closeKey})`}
                tabIndex={-1}
                onClick={() => onClose(terminal.id)}
              >
                <CloseIcon />
              </button>
            </div>
          );
        })}
        {editorTabs.map((editor) => {
          const key = editorKey(editor.id);
          const selected = key === selectedKey;
          const classes = ["tab-bar__item", "tab-bar__item--editor"];
          if (selected) classes.push("tab-bar__item--selected");
          if (editor.dirty) classes.push("tab-bar__item--dirty");
          return (
            <div key={key} role="presentation" className={classes.join(" ")} data-testid="editor-tab-item">
              <button
                ref={refOf(key)}
                type="button"
                role="tab"
                id={tabDomId(idBase, editor.id)}
                className="tab-bar__tab"
                aria-selected={selected}
                aria-controls={selected ? panelId : undefined}
                aria-keyshortcuts="Delete"
                aria-label={editor.dirty ? `${editor.label}(未保存)` : editor.label}
                tabIndex={key === focusableKey ? 0 : -1}
                title={`${projectName}: ${editor.path}`}
                onClick={() => {
                  if (!selected) onSelectEditor?.(editor.id);
                }}
              >
                <FileIcon className="tab-bar__doc" />
                <span className="tab-bar__label">{editor.label}</span>
              </button>
              <button
                type="button"
                className="tab-bar__close"
                aria-label={`「${editor.label}」を閉じる`}
                title={closeKey === null ? "タブを閉じる" : `タブを閉じる(${closeKey})`}
                tabIndex={-1}
                onClick={() => onCloseEditor?.(editor.id)}
              >
                {editor.dirty && (
                  <span className="tab-bar__dirty" aria-hidden="true" data-testid="editor-tab-dirty">
                    ●
                  </span>
                )}
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
        aria-keyshortcuts={newAriaKey ?? undefined}
        title={newKey === null ? "新規ターミナル" : `新規ターミナル(${newKey})`}
        disabled={!canAdd}
        onClick={onNew}
      >
        <PlusIcon />
      </button>
    </div>
  );
}
