import "./SettingsDialog.css";

import { type KeyboardEvent, useEffect, useId, useRef } from "react";
import { useStore } from "zustand";

import { findBinding } from "../../logic/keys/shortcutTable";
import type { Actions } from "../../state/actions";
import type { AppStore } from "../../state/appStore";
import type { UiStore } from "../../state/uiStore";
import type { Snapshot } from "../snapshotTypes";
import { FontSizeStepper } from "./FontSizeStepper";

interface SettingsDialogProps {
  store: AppStore;
  /** 開閉(settingsOpen)とショートカットの表(⌘W のキーを引く)。 */
  ui: UiStore;
  actions: Pick<Actions, "stepFontSize" | "resetFontSizes" | "openSettingsFile">;
}

/** Tab で回る要素(▲▼ のボタンは tabindex="-1" なので入らない。無効のボタンも入らない)。 */
const FOCUSABLE = "button:not([disabled]):not([tabindex='-1']), [tabindex]:not([tabindex='-1'])";

/** ⌘W を受けたかを見る受け口(メニューの項目の行。キーの文字列は表から引く)。 */
const MENU_RECEIVERS = ["menu-item"] as const;

/**
 * 設定画面(M4.6。ADR-013 の 5)。⌘, とメニューの「設定…」で開く画面内のシート。uiStore の settingsOpen が真のあいだだけ描く。
 * 文字の大きさ(ターミナル・画面)のステッパーと、「設定ファイルを開く…」、「既定に戻す」・「完了」を持つ。値は写しから読み、
 * 押すたびにすぐ反映する(「適用」は無い)。背景は暗くしない(変えた大きさを後ろで見ながら選べる。【要確認】4)。
 * 開くとターミナルのステッパーにフォーカスし、Tab・Shift+Tab は中を回る。Esc・Return(ボタン以外の上)・「完了」・
 * ⌘W(表の terminal.close-tab のキー。画面の keydown は取り消す)で閉じ、開く前のフォーカスへ戻す。
 */
export function SettingsDialog({ store, ui, actions }: SettingsDialogProps) {
  const open = useStore(ui, (s) => s.settingsOpen);
  const fontSizes = useStore(store, (s) => s.snapshot?.fontSizes ?? null);
  if (!open || fontSizes === null) return null;
  return <SettingsSheet fontSizes={fontSizes} ui={ui} actions={actions} />;
}

interface SettingsSheetProps {
  fontSizes: Snapshot["fontSizes"];
  ui: UiStore;
  actions: SettingsDialogProps["actions"];
}

function SettingsSheet({ fontSizes, ui, actions }: SettingsSheetProps) {
  const dialogRef = useRef<HTMLDialogElement>(null);
  const doneRef = useRef<HTMLButtonElement>(null);
  const titleId = useId();
  const fontGroupId = useId();
  const shortcutGroupId = useId();
  const close = () => ui.getState().closeSettings();
  const { terminal, interface: screen } = fontSizes;
  const allDefault = terminal.value === terminal.defaultValue && screen.value === screen.defaultValue;

  useEffect(() => {
    const dialog = dialogRef.current;
    if (dialog === null) return;
    const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    if (!dialog.open) dialog.showModal();
    dialog.querySelector<HTMLElement>("[role=spinbutton]")?.focus();
    // Esc は <dialog> の cancel にもなる(フォーカスが中に無いとき)。閉じるのは状態で行う。
    const onCancel = (event: Event) => {
      event.preventDefault();
      ui.getState().closeSettings();
    };
    dialog.addEventListener("cancel", onCancel);
    return () => {
      dialog.removeEventListener("cancel", onCancel);
      if (dialog.open) dialog.close();
      if (previous?.isConnected) previous.focus();
    };
  }, [ui]);

  // 「既定に戻す」が無効になったとき、フォーカスが行き場を失っていれば「完了」へ移す(設定画面の外へ落とさない)。
  // WebKit はボタンをマウスで押してもボタンにフォーカスせず、<dialog> 自身に置くので、それも行き場が無いとみなす。
  useEffect(() => {
    if (!allDefault) return;
    const active = document.activeElement;
    const lost =
      active === null ||
      active === document.body ||
      active === dialogRef.current ||
      (active instanceof HTMLButtonElement && active.disabled);
    if (lost) doneRef.current?.focus();
  }, [allDefault]);

  const onKeyDown = (event: KeyboardEvent<HTMLDialogElement>) => {
    if (event.key === "Escape") {
      event.preventDefault();
      close();
      return;
    }
    if (findBinding(ui.getState().shortcuts, event, MENU_RECEIVERS)?.command === "terminal.close-tab") {
      // 設定画面を開いているあいだの ⌘W は設定画面だけを閉じる(【要確認】3)。
      event.preventDefault();
      close();
      return;
    }
    if (event.key === "Enter" && !event.metaKey && !event.ctrlKey && !event.altKey) {
      // ボタンの上の Return はそのボタンを押す(既定の動作)。それ以外では「完了」と同じ。
      if (event.target instanceof HTMLButtonElement) return;
      event.preventDefault();
      close();
      return;
    }
    if (event.key !== "Tab") return;
    // Tab の移動は自分で行う(macOS の既定では Tab がボタンに止まらないため。端で反対側に回る)。
    event.preventDefault();
    const focusable = [...event.currentTarget.querySelectorAll<HTMLElement>(FOCUSABLE)];
    if (focusable.length === 0) return;
    const current = focusable.indexOf(document.activeElement as HTMLElement);
    const step = event.shiftKey ? -1 : 1;
    const next = current < 0 ? 0 : (current + step + focusable.length) % focusable.length;
    focusable[next]?.focus();
  };

  return (
    <dialog
      ref={dialogRef}
      role="dialog"
      aria-modal="true"
      aria-labelledby={titleId}
      className="settings-sheet"
      data-testid="settings-dialog"
      onKeyDown={onKeyDown}
    >
      <h2 id={titleId} className="settings-sheet__title">
        設定
      </h2>

      <section className="settings-sheet__group" aria-labelledby={fontGroupId}>
        <h3 id={fontGroupId} className="settings-sheet__group-title">
          文字の大きさ
        </h3>
        <div className="settings-sheet__rows">
          <div className="settings-row" data-testid="font-size-row-terminal">
            <div className="settings-row__text">
              <span className="settings-row__label">ターミナル</span>
            </div>
            <FontSizeStepper
              label="ターミナルの文字の大きさ"
              size={terminal}
              onStep={(delta) => void actions.stepFontSize("terminal", delta)}
            />
          </div>
          <div className="settings-row" data-testid="font-size-row-interface">
            <div className="settings-row__text">
              <span className="settings-row__label">画面</span>
              <span className="settings-row__hint">サイドバー・タブ・ツリー・メニューの文字</span>
            </div>
            <FontSizeStepper
              label="画面の文字の大きさ"
              size={screen}
              onStep={(delta) => void actions.stepFontSize("interface", delta)}
            />
          </div>
        </div>
      </section>

      <section className="settings-sheet__group" aria-labelledby={shortcutGroupId}>
        <h3 id={shortcutGroupId} className="settings-sheet__group-title">
          ショートカット
        </h3>
        <div className="settings-sheet__rows">
          <div className="settings-row">
            <div className="settings-row__text">
              <span className="settings-row__hint settings-row__hint--body">キーは設定ファイル(settings.toml)で変えます</span>
            </div>
            <button type="button" className="button" onClick={() => void actions.openSettingsFile()}>
              設定ファイルを開く…
            </button>
          </div>
        </div>
      </section>

      <div className="settings-sheet__actions">
        <button
          type="button"
          className="button settings-sheet__reset"
          disabled={allDefault}
          onClick={() => void actions.resetFontSizes()}
        >
          既定に戻す
        </button>
        <button ref={doneRef} type="button" className="button button--primary" onClick={close}>
          完了
        </button>
      </div>
    </dialog>
  );
}
