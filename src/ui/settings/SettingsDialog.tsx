import "./SettingsDialog.css";

import { type KeyboardEvent, useEffect, useId, useRef } from "react";
import { useStore } from "zustand";

import { findBinding } from "../../logic/keys/shortcutTable";
import type { Actions } from "../../state/actions";
import type { AppStore } from "../../state/appStore";
import type { EditorController } from "../../state/editorController";
import type { UiStore } from "../../state/uiStore";
import { type SegmentedOption, SegmentedRadio } from "../design/SegmentedRadio";
import type { Snapshot } from "../snapshotTypes";
import { type StepperUnit, ValueStepper } from "./ValueStepper";

interface SettingsDialogProps {
  store: AppStore;
  /** 開閉(settingsOpen)とショートカットの表(⌘W のキーを引く)。 */
  ui: UiStore;
  actions: Pick<Actions, "stepFontSize" | "stepTranslucency" | "setTheme" | "resetAppearance" | "openSettingsFile" | "setCheckUpdates">;
  /** Vim のキー操作の切替(M4.8。ADR-015 の 6)。 */
  editor: Pick<EditorController, "setVim">;
}

/** 文字の大きさと透過の単位。 */
const POINTS: StepperUnit = { suffix: " pt", spoken: "ポイント" };
const PERCENT: StepperUnit = { suffix: "%", spoken: "パーセント" };

/** テーマの選び方(ADR-016。並びは macOS の設定の「外観」にならう)。 */
const THEME_OPTIONS: readonly SegmentedOption<Snapshot["theme"]>[] = [
  { value: "system", label: "システム" },
  { value: "light", label: "ライト" },
  { value: "dark", label: "ダーク" },
];

/** Tab で回る要素(▲▼ のボタンは tabindex="-1" なので入らない。無効のボタンも入らない)。 */
const FOCUSABLE = "button:not([disabled]):not([tabindex='-1']), [tabindex]:not([tabindex='-1'])";

/** ⌘W を受けたかを見る受け口(メニューの項目の行。キーの文字列は表から引く)。 */
const MENU_RECEIVERS = ["menu-item"] as const;

/**
 * 設定画面(M4.6。ADR-013 の 5)。⌘, とメニューの「設定…」で開く画面内のシート。uiStore の settingsOpen が真のあいだだけ描く。
 * 文字の大きさ(ターミナル・画面)と透過(ウィンドウ・ターミナルの背景。M4.7。ADR-014 の 3)のステッパーと、
 * 「設定ファイルを開く…」、「既定に戻す」(4 つの値をすべて戻す。すべて既定なら無効)・「完了」を持つ。値は写しから読み、
 * 押すたびにすぐ反映する(「適用」は無い)。背景は暗くしない(変えた大きさを後ろで見ながら選べる。【要確認】4)。
 * 開くとターミナルのステッパーにフォーカスし、Tab・Shift+Tab は中を回る。Esc・Return(ボタン以外の上)・「完了」・
 * ⌘W(表の terminal.close-tab のキー。画面の keydown は取り消す)で閉じ、開く前のフォーカスへ戻す。
 * M4.8: 「エディタ」の組に「Vim のキー操作」の切替(role="switch")。押すと set_editor_vim を送り、写しの editor.vim で表示が替わる。
 * 「外観」の組(ADR-016。M4.7 の「透過」の組にテーマを足して名前を替えた): テーマを「システム・ライト・ダーク」から選ぶ。押すと set_theme を送り、写しの theme で表示が替わる
 * (色を替えるのは Rust。アプリの外観が替わり、画面の prefers-color-scheme が追従する)。「既定に戻す」はテーマを戻さない。
 * 「既定に戻す」は Vim を戻さない(見た目ではなく操作の選択なので。ADR-015 の 6)。ターミナルの文字の大きさの行は
 * エディタの本文にも効くので「ターミナルとエディタ」と呼ぶ(ADR-015 の 7)。
 */
export function SettingsDialog({ store, ui, actions, editor }: SettingsDialogProps) {
  const open = useStore(ui, (s) => s.settingsOpen);
  const fontSizes = useStore(store, (s) => s.snapshot?.fontSizes ?? null);
  const translucency = useStore(store, (s) => s.snapshot?.translucency ?? null);
  const vim = useStore(store, (s) => s.snapshot?.editor.vim ?? false);
  const theme = useStore(store, (s) => s.snapshot?.theme ?? "system");
  const checkUpdates = useStore(store, (s) => s.snapshot?.checkUpdates ?? false);
  if (!open || fontSizes === null || translucency === null) return null;
  return (
    <SettingsSheet
      fontSizes={fontSizes}
      translucency={translucency}
      vim={vim}
      theme={theme}
      checkUpdates={checkUpdates}
      ui={ui}
      actions={actions}
      editor={editor}
    />
  );
}

interface SettingsSheetProps {
  fontSizes: Snapshot["fontSizes"];
  translucency: Snapshot["translucency"];
  vim: boolean;
  theme: Snapshot["theme"];
  checkUpdates: boolean;
  ui: UiStore;
  actions: SettingsDialogProps["actions"];
  editor: SettingsDialogProps["editor"];
}

/** 写しの値が既定か。 */
const isDefault = (entry: { value: number; defaultValue: number }) => entry.value === entry.defaultValue;

function SettingsSheet({ fontSizes, translucency, vim, theme, checkUpdates, ui, actions, editor }: SettingsSheetProps) {
  const dialogRef = useRef<HTMLDialogElement>(null);
  const doneRef = useRef<HTMLButtonElement>(null);
  const titleId = useId();
  const fontGroupId = useId();
  const appearanceGroupId = useId();
  const shortcutGroupId = useId();
  const editorGroupId = useId();
  const vimLabelId = useId();
  const vimHintId = useId();
  const updateGroupId = useId();
  const updateLabelId = useId();
  const updateHintId = useId();
  const close = () => ui.getState().closeSettings();
  const { terminal, interface: screen } = fontSizes;
  const allDefault = [terminal, screen, translucency.window, translucency.terminal].every(isDefault);

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
              <span className="settings-row__label">ターミナルとエディタ</span>
            </div>
            <ValueStepper
              label="ターミナルの文字の大きさ"
              size={terminal}
              unit={POINTS}
              upLabel="大きくする"
              downLabel="小さくする"
              onStep={(delta) => void actions.stepFontSize("terminal", delta)}
            />
          </div>
          <div className="settings-row" data-testid="font-size-row-interface">
            <div className="settings-row__text">
              <span className="settings-row__label">画面</span>
              <span className="settings-row__hint">サイドバー・タブ・ツリー・メニューの文字</span>
            </div>
            <ValueStepper
              label="画面の文字の大きさ"
              size={screen}
              unit={POINTS}
              upLabel="大きくする"
              downLabel="小さくする"
              onStep={(delta) => void actions.stepFontSize("interface", delta)}
            />
          </div>
        </div>
      </section>

      <section className="settings-sheet__group" aria-labelledby={appearanceGroupId}>
        <h3 id={appearanceGroupId} className="settings-sheet__group-title">
          外観
        </h3>
        <div className="settings-sheet__rows">
          <div className="settings-row" data-testid="theme-row">
            <div className="settings-row__text">
              <span className="settings-row__label">テーマ</span>
            </div>
            <SegmentedRadio
              label="テーマ"
              options={THEME_OPTIONS}
              value={theme}
              onChange={(next) => void actions.setTheme(next)}
              itemClassName="settings-segmented__item"
              testId="theme-switch"
            />
          </div>
          <div className="settings-row" data-testid="translucency-row-window">
            <div className="settings-row__text">
              <span className="settings-row__label">ウィンドウの透過</span>
              <span className="settings-row__hint">サイドバーと上部の帯</span>
            </div>
            <ValueStepper
              label="ウィンドウの透過"
              size={translucency.window}
              unit={PERCENT}
              step={translucency.window.step}
              upLabel="透過を強くする"
              downLabel="透過を弱くする"
              onStep={(delta) => void actions.stepTranslucency("window", delta)}
            />
          </div>
          <div className="settings-row" data-testid="translucency-row-terminal">
            <div className="settings-row__text">
              <span className="settings-row__label">ターミナルの背景の透過</span>
            </div>
            <ValueStepper
              label="ターミナルの背景の透過"
              size={translucency.terminal}
              unit={PERCENT}
              step={translucency.terminal.step}
              upLabel="透過を強くする"
              downLabel="透過を弱くする"
              onStep={(delta) => void actions.stepTranslucency("terminal", delta)}
            />
          </div>
        </div>
      </section>

      <section className="settings-sheet__group" aria-labelledby={editorGroupId}>
        <h3 id={editorGroupId} className="settings-sheet__group-title">
          エディタ
        </h3>
        <div className="settings-sheet__rows">
          <div className="settings-row" data-testid="editor-vim-row">
            <div className="settings-row__text">
              <span id={vimLabelId} className="settings-row__label">
                Vim のキー操作
              </span>
              <span id={vimHintId} className="settings-row__hint">
                Markdown のエディタで使う。:w で保存
              </span>
            </div>
            <button
              type="button"
              role="switch"
              aria-checked={vim}
              aria-labelledby={vimLabelId}
              aria-describedby={vimHintId}
              className="settings-switch"
              data-testid="editor-vim-switch"
              onClick={() => void editor.setVim(!vim)}
            >
              <span className="settings-switch__thumb" aria-hidden="true" />
            </button>
          </div>
        </div>
      </section>

      <section className="settings-sheet__group" aria-labelledby={updateGroupId}>
        <h3 id={updateGroupId} className="settings-sheet__group-title">
          アップデート
        </h3>
        <div className="settings-sheet__rows">
          <div className="settings-row" data-testid="check-updates-row">
            <div className="settings-row__text">
              <span id={updateLabelId} className="settings-row__label">
                起動時に新しい版を確かめる
              </span>
              <span id={updateHintId} className="settings-row__hint">
                GitHub の Releases に問い合わせる
              </span>
            </div>
            <button
              type="button"
              role="switch"
              aria-checked={checkUpdates}
              aria-labelledby={updateLabelId}
              aria-describedby={updateHintId}
              className="settings-switch"
              data-testid="check-updates-switch"
              onClick={() => void actions.setCheckUpdates(!checkUpdates)}
            >
              <span className="settings-switch__thumb" aria-hidden="true" />
            </button>
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
          onClick={() => void actions.resetAppearance()}
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
