// 描画方式の切り替えの判断(T21。設計書 §2.2「描画方式」「非表示のインスタンス」)。
//
// xterm.js を知らない形にして、node の環境の Vitest で確かめられるようにする。
// 実際に WebGL のアドオンを読み込む・破棄するのは RendererBackend(src/terminal/xterm/XtermEngine.ts)。
//
// - `dom`: xterm.js の既定の描画。読み込むものは無い
// - `webgl`: 表示している間だけ WebGL の描画を持つ。隠したら破棄してコンテキストを返し、表示したら作り直す
// - WebGL を作れない・コンテキストを失ったときは DOM に戻し、報告する。戻したあとは作り直さない
//   (「WebGL を指定したのに DOM で動いていた」を見落とさないよう、黙って行き来させない)

import { describeError } from "../logic/describeError";
import type { RendererKind } from "./engine";

/** 読み込んだ WebGL の描画。 */
export interface WebglRendererHandle {
  /** 破棄する。xterm.js は DOM の描画に戻る。2 回目以降は何もしない。 */
  dispose(): void;
}

/** 描画を実際に付け外しするもの。 */
export interface RendererBackend {
  /**
   * WebGL の描画を読み込む。作れなければ例外を投げる。
   * 読み込んだあとにコンテキストを失ったら onContextLoss を呼ぶ。
   */
  loadWebgl(onContextLoss: () => void): WebglRendererHandle;
}

export interface RendererControlOptions {
  /** 起動時に指定された描画方式。 */
  readonly requested: RendererKind;
  readonly backend: RendererBackend;
  /** 報告の文に入れる名前(ターミナルの ID など)。 */
  readonly label: string;
  /** DOM に戻したことの報告先(診断。標準エラーの `[arin:frontend-error]` になる)。 */
  readonly reportError: (message: string) => void;
}

export class RendererControl {
  readonly #backend: RendererBackend;
  readonly #label: string;
  readonly #reportError: (message: string) => void;
  readonly #listeners = new Set<(renderer: RendererKind) => void>();
  #active: RendererKind;
  #webgl: WebglRendererHandle | null = null;
  #visible = false;
  #disposed = false;

  constructor({ requested, backend, label, reportError }: RendererControlOptions) {
    this.#active = requested;
    this.#backend = backend;
    this.#label = label;
    this.#reportError = reportError;
  }

  /** 表示しているときに使う描画方式。 */
  get active(): RendererKind {
    return this.#active;
  }

  /** いま WebGL の描画を持っているか。 */
  get holdsWebgl(): boolean {
    return this.#webgl !== null;
  }

  /** 描画方式が変わったときに受け取る。外す関数を返す。 */
  onChange(listener: (renderer: RendererKind) => void): () => void {
    this.#listeners.add(listener);
    return () => {
      this.#listeners.delete(listener);
    };
  }

  /** 表示したときに呼ぶ。WebGL を指定していれば読み込む。表示中なら何もしない。 */
  show(): void {
    if (this.#disposed || this.#visible) return;
    this.#visible = true;
    if (this.#active !== "webgl") return;
    let handle: WebglRendererHandle | null = null;
    const onContextLoss = () => {
      // 隠したあと・作り直したあとに、古い描画から遅れて届いた通知は無視する。
      if (handle === null || this.#webgl !== handle) return;
      this.#fallBack("WebGL の描画コンテキストを失った");
    };
    try {
      handle = this.#backend.loadWebgl(onContextLoss);
    } catch (cause) {
      this.#fallBack(`WebGL の描画を作れなかった(${describeError(cause)})`);
      return;
    }
    this.#webgl = handle;
  }

  /** 隠したときに呼ぶ。WebGL の描画を持っていれば破棄する(コンテキストを返す)。 */
  hide(): void {
    if (this.#disposed || !this.#visible) return;
    this.#visible = false;
    this.#releaseWebgl();
  }

  /** 破棄する。WebGL の描画を持っていれば破棄する。2 回目以降は何もしない。 */
  dispose(): void {
    if (this.#disposed) return;
    this.#disposed = true;
    this.#visible = false;
    this.#releaseWebgl();
    this.#listeners.clear();
  }

  #releaseWebgl(): void {
    const handle = this.#webgl;
    this.#webgl = null;
    handle?.dispose();
  }

  #fallBack(reason: string): void {
    this.#releaseWebgl();
    this.#active = "dom";
    this.#reportError(`ターミナル ${this.#label}: ${reason}ので、DOM の描画に戻した`);
    for (const listener of [...this.#listeners]) listener("dom");
  }
}
