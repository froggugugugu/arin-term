import "./ConfirmDialog.css";

import { type KeyboardEvent, type ReactNode, useEffect, useId, useRef } from "react";

interface ConfirmDialogProps {
  title: string;
  children: ReactNode;
  confirmLabel: string;
  cancelLabel: string;
  onConfirm: () => void;
  /** 取り消し(取り消しのボタン・Esc)。 */
  onCancel: () => void;
  /** 確定が取り返しのつかない操作か(確定のボタンを警告の色にする)。 */
  destructive?: boolean;
  /**
   * 3 つ目のボタン(衝突の確認の「置き換える」など)。macOS のアラートにならい、取り消しと確定から離して左端に置く。
   * `destructive` なら文字を警告の色にする。
   */
  alternate?: { readonly label: string; readonly onSelect: () => void; readonly destructive?: boolean };
  /** 開いたときにフォーカスするボタン。既定は取り消し(誤って確定しない)。確定は既定のボタンが安全な操作のときだけ。 */
  initialFocus?: "cancel" | "confirm";
  testId?: string;
}

const FOCUSABLE = "button:not([disabled]), [href], input:not([disabled]), [tabindex]:not([tabindex='-1'])";

/**
 * 確認ダイアログ(設計書 §9.1。削除の確認 FR-003、終了の確認 FR-063 は T34d が使う)。
 * モーダルの <dialog>(外の要素は操作できない)に alertdialog の役割と名前・説明を付ける。
 * 開くと「取り消し」にフォーカスし(誤って確定しない。initialFocus で確定にもできる)、Tab・Shift+Tab はダイアログの中の
 * ボタンを回り、Esc で取り消す。3 つ目のボタン(alternate)は左端に離して置く(macOS のアラートの並び)。
 * 閉じると、開く前にフォーカスがあった要素へ戻す。
 */
export function ConfirmDialog({
  title,
  children,
  confirmLabel,
  cancelLabel,
  onConfirm,
  onCancel,
  destructive = false,
  alternate,
  initialFocus = "cancel",
  testId,
}: ConfirmDialogProps) {
  const dialogRef = useRef<HTMLDialogElement>(null);
  const cancelRef = useRef<HTMLButtonElement>(null);
  const confirmRef = useRef<HTMLButtonElement>(null);
  const initialFocusRef = useRef(initialFocus);
  const titleId = useId();
  const bodyId = useId();
  const onCancelRef = useRef(onCancel);
  onCancelRef.current = onCancel;

  useEffect(() => {
    const dialog = dialogRef.current;
    if (dialog === null) return;
    const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    if (!dialog.open) dialog.showModal();
    (initialFocusRef.current === "confirm" ? confirmRef : cancelRef).current?.focus();
    // Esc は <dialog> の cancel になる。閉じるのは呼び出し側に任せる(状態で開閉する)。
    const onCancelEvent = (event: Event) => {
      event.preventDefault();
      onCancelRef.current();
    };
    dialog.addEventListener("cancel", onCancelEvent);
    return () => {
      dialog.removeEventListener("cancel", onCancelEvent);
      if (dialog.open) dialog.close();
      if (previous?.isConnected) previous.focus();
    };
  }, []);

  const onKeyDown = (event: KeyboardEvent<HTMLDialogElement>) => {
    if (event.key === "Escape") {
      event.preventDefault();
      onCancel();
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
      role="alertdialog"
      aria-modal="true"
      aria-labelledby={titleId}
      aria-describedby={bodyId}
      className="confirm-dialog"
      data-testid={testId}
      onKeyDown={onKeyDown}
    >
      <h2 id={titleId} className="confirm-dialog__title">
        {title}
      </h2>
      <div id={bodyId} className="confirm-dialog__body">
        {children}
      </div>
      <div className="confirm-dialog__actions">
        {alternate !== undefined && (
          <button
            type="button"
            className={
              alternate.destructive === true
                ? "button button--destructive-text confirm-dialog__alternate"
                : "button confirm-dialog__alternate"
            }
            onClick={alternate.onSelect}
          >
            {alternate.label}
          </button>
        )}
        <button ref={cancelRef} type="button" className="button" onClick={onCancel}>
          {cancelLabel}
        </button>
        <button
          ref={confirmRef}
          type="button"
          className={destructive ? "button button--danger" : "button button--primary"}
          onClick={onConfirm}
        >
          {confirmLabel}
        </button>
      </div>
    </dialog>
  );
}
