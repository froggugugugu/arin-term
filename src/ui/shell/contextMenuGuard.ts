// WebKit の標準の右クリックのメニューを止める。
//
// 標準のメニューには「再読み込み」があり、選ぶと画面が読み込み直されてターミナルの表示が消える(シェルは生きているが、
// 表示の中身は画面の側にしか無い)。アプリに要る項目は無いので、文字の入力欄(input・textarea・contenteditable。
// Markdown エディタの本文を含む)のカット・コピー・ペーストのためだけに残し、ほかはどこでも止める。
// xterm.js は右クリックで入力用の textarea をポインタの下へ動かすので、ターミナルの中は入力欄でも止める。
// 独自のメニュー(カード・ツリー・プロジェクト欄の余白)は、それぞれの部品が先に開いて既定の動作を止める。

/** 右クリックの対象。本物は DOM の要素。 */
export interface ContextMenuTarget {
  closest(selector: string): unknown;
}

const EDITABLE = 'input, textarea, [contenteditable]:not([contenteditable="false"])';
const TERMINAL = ".xterm";

/** 標準のメニューを残すか(文字の入力欄で、ターミナルの中でないときだけ)。 */
export function keepsNativeContextMenu(target: ContextMenuTarget | null): boolean {
  if (target === null) return false;
  return target.closest(EDITABLE) != null && target.closest(TERMINAL) == null;
}

/** イベントを受ける先(本物は document)。 */
export type ContextMenuEventTarget = Pick<Document, "addEventListener" | "removeEventListener">;

/** 標準のメニューの抑止を付ける。外す関数を返す。 */
export function installContextMenuGuard(target: ContextMenuEventTarget): () => void {
  const onContextMenu = (event: Event) => {
    const el = event.target as Partial<ContextMenuTarget> | null;
    const element = el !== null && typeof el.closest === "function" ? (el as ContextMenuTarget) : null;
    if (!keepsNativeContextMenu(element)) event.preventDefault();
  };
  target.addEventListener("contextmenu", onContextMenu);
  return () => target.removeEventListener("contextmenu", onContextMenu);
}
