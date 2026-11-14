// ターミナルの ID とサーフェスの対応と、サーフェスと Rust の配線(設計書 §5.1、§7.1「ターミナルの入出力」)。
//
// - 1 つの ID に 1 つのサーフェスを持ち続ける。非表示のあいだも破棄しない(設計書 §1.3 #4)。
//   破棄するのは呼び出し側が dispose を呼んだときだけ
// - 出力: チャネル → surface.write → 取り込みが済んだ量を ack_terminal_output で返す(流量制御)
// - 入力: surface.onInput → write_terminal。寸法: surface.onResize → resize_terminal
// - 置き場所(TerminalHost)への要素の付け外しもここで行い、React には要素を管理させない(ADR-003)
// - コピー・ペースト: 画面のイベント → handleClipboard → フォーカスのあるサーフェスの選択の取得・貼り付け(§9.1)
//
// エンジンと通信は外から受け取る。本物は src/terminal/xterm/ と src/ipc/tauri.ts、
// テストでは src/test/fakeTerminalEngine.ts と src/test/fakeIpc.ts を渡す。

import type { ArinIpc, OutputSubscription } from "../ipc/client";
import { describeError } from "../logic/describeError";
import { type ClipboardFocus, type ClipboardKind, routeClipboard } from "../logic/keys/clipboard";
import type { TerminalEngine, TerminalId, TerminalSize, TerminalSurface } from "./engine";

/** 画面のコピー・ペーストのイベントの clipboardData のうち、使う部分(DataTransfer はこの形を満たす)。 */
export interface ClipboardText {
  getData(format: string): string;
  setData(format: string, data: string): void;
}

const TEXT_FORMAT = "text/plain";

/** ターミナルが使う Rust の操作(ArinIpc の一部)。 */
export type TerminalBackend = Pick<
  ArinIpc,
  "connectTerminal" | "writeTerminal" | "ackTerminalOutput" | "resizeTerminal"
>;

/** サーフェスの要素を入れる置き場所(TerminalHost の要素)。 */
export interface TerminalSlot {
  appendChild(element: HTMLElement): unknown;
  removeChild(element: HTMLElement): unknown;
}

export interface TerminalManagerOptions {
  /** 接続・入出力の失敗の報告先。既定は console.error。 */
  reportError?: (message: string) => void;
}

type ConnectionState = "idle" | "connecting" | "connected";

interface Entry {
  readonly id: TerminalId;
  readonly surface: TerminalSurface;
  state: ConnectionState;
  /** 接続中・接続済みのときの受け取り。 */
  subscription: OutputSubscription | null;
  /** 接続中の connect が返す Promise(二重の接続で同じものを返す)。 */
  connecting: Promise<void> | null;
  /** 接続の応答を待つあいだの入力。 */
  pendingInput: string[];
  /** 最後に Rust へ伝えた寸法。 */
  sentSize: TerminalSize | null;
  /** いま要素を入れている置き場所。 */
  slot: TerminalSlot | null;
  disposed: boolean;
}

const sameSize = (a: TerminalSize, b: TerminalSize) => a.rows === b.rows && a.cols === b.cols;

export class TerminalManager {
  readonly #engine: TerminalEngine;
  readonly #backend: TerminalBackend;
  readonly #reportError: (message: string) => void;
  readonly #entries = new Map<TerminalId, Entry>();

  constructor(engine: TerminalEngine, backend: TerminalBackend, options: TerminalManagerOptions = {}) {
    this.#engine = engine;
    this.#backend = backend;
    this.#reportError = options.reportError ?? ((message) => console.error(message));
  }

  /** 持っているサーフェスの数。 */
  get size(): number {
    return this.#entries.size;
  }

  /** ID のサーフェスを返す。無ければ undefined。 */
  get(id: TerminalId): TerminalSurface | undefined {
    return this.#entries.get(id)?.surface;
  }

  /** ID のサーフェスを返す。無ければエンジンで作り、入力と寸法の受け取り手を付けて覚える。同じ ID で二重に作らない。 */
  getOrCreate(id: TerminalId): TerminalSurface {
    return this.#entry(id).surface;
  }

  /**
   * サーフェスの要素を置き場所に入れ、寸法を合わせ、まだなら接続する。外す関数を返す。
   * 外す関数は要素を置き場所から出すだけで、ターミナルの破棄も切断もしない(部品の付け外しで作り直さない)。
   */
  attach(id: TerminalId, slot: TerminalSlot): () => void {
    const entry = this.#entry(id);
    if (entry.slot !== slot) {
      entry.slot?.removeChild(entry.surface.element);
      slot.appendChild(entry.surface.element);
      entry.slot = slot;
    }
    entry.surface.fit();
    void this.connect(id);
    return () => {
      if (entry.slot !== slot) return;
      slot.removeChild(entry.surface.element);
      entry.slot = null;
    };
  }

  /** 置き場所の大きさに合わせて行・桁を決め直す。変われば寸法のコマンドが送られる。 */
  fit(id: TerminalId): void {
    this.#entries.get(id)?.surface.fit();
  }

  /**
   * いまの行・桁で出力のチャネルをつなぐ。接続中・接続済みなら何もしない(同じ Promise を返す)。
   * 失敗は報告し、次に呼べば試し直す。この Promise は失敗しない。
   */
  connect(id: TerminalId): Promise<void> {
    const entry = this.#entry(id);
    if (entry.state === "connected") return Promise.resolve();
    if (entry.connecting !== null) return entry.connecting;

    const size = entry.surface.size();
    entry.state = "connecting";
    const subscription = this.#backend.connectTerminal(id, size.rows, size.cols, (data) =>
      this.#onOutput(entry, data),
    );
    entry.subscription = subscription;
    entry.sentSize = size;
    entry.connecting = subscription.connected.then(
      () => {
        entry.connecting = null;
        if (entry.disposed) return;
        entry.state = "connected";
        for (const data of entry.pendingInput.splice(0)) this.#send(entry, data);
        const now = entry.surface.size();
        if (entry.sentSize === null || !sameSize(entry.sentSize, now)) this.#resize(entry, now);
      },
      (cause: unknown) => {
        entry.connecting = null;
        entry.subscription = null;
        entry.sentSize = null;
        entry.pendingInput = [];
        if (entry.disposed) return;
        entry.state = "idle";
        this.#reportError(`ターミナル ${id} に接続できなかった: ${describeError(cause)}`);
      },
    );
    return entry.connecting;
  }

  /**
   * 画面のコピー・ペーストのイベントを、フォーカスのあるターミナルへ振り分ける(設計書 §9.1、FR-043)。
   * - コピー: 選択があれば、その文字列を text/plain に書く
   * - ペースト: text/plain をそのターミナルに貼り付ける(PTY への入力になる)。文字列が無ければ貼り付けない
   *
   * 処理したら true を返す。呼び出し側はイベントの既定の動作と、xterm.js 自身の受け取りを止める。
   * false のとき(フォーカスがターミナルに無い、選択の無いコピー)は何もしていないので、既定の動作に任せる。
   */
  handleClipboard(kind: ClipboardKind, data: ClipboardText): boolean {
    const surface = this.#focusedSurface();
    const selection = surface?.getSelection() ?? "";
    const focus: ClipboardFocus =
      surface === undefined ? { kind: "other" } : { kind: "terminal", hasSelection: selection !== "" };
    switch (routeClipboard(kind, focus)) {
      case "copy-selection":
        data.setData(TEXT_FORMAT, selection);
        return true;
      case "paste": {
        const text = data.getData(TEXT_FORMAT);
        if (text !== "") surface?.paste(text);
        return true;
      }
      case "none":
        return false;
    }
  }

  #focusedSurface(): TerminalSurface | undefined {
    for (const entry of this.#entries.values()) {
      if (entry.surface.hasFocus()) return entry.surface;
    }
    return undefined;
  }

  /**
   * ID のサーフェスを破棄し、出力の受け取りをやめ、対応を外す。持っていなければ何もせず false を返す。
   * サーフェスの dispose が例外を投げても、対応は先に外してあるので残らない(例外はそのまま伝える)。
   */
  dispose(id: TerminalId): boolean {
    const entry = this.#entries.get(id);
    if (!entry) return false;
    this.#entries.delete(id);
    entry.disposed = true;
    entry.pendingInput = [];
    entry.subscription?.close();
    entry.subscription = null;
    entry.slot?.removeChild(entry.surface.element);
    entry.slot = null;
    entry.surface.dispose();
    return true;
  }

  #entry(id: TerminalId): Entry {
    const existing = this.#entries.get(id);
    if (existing) return existing;
    const surface = this.#engine.createSurface(id);
    const entry: Entry = {
      id,
      surface,
      state: "idle",
      subscription: null,
      connecting: null,
      pendingInput: [],
      sentSize: null,
      slot: null,
      disposed: false,
    };
    surface.onInput((data) => {
      if (entry.disposed) return;
      if (entry.state === "connected") this.#send(entry, data);
      else if (entry.state === "connecting") entry.pendingInput.push(data);
      // 接続していない(接続の失敗のあとなど)ときの入力は送り先が無いので捨てる。
    });
    surface.onResize((cols, rows) => {
      // 接続を待つあいだの変更は、接続のあとにまとめて送る。
      if (entry.disposed || entry.state !== "connected") return;
      this.#resize(entry, { rows, cols });
    });
    this.#entries.set(id, entry);
    return entry;
  }

  #onOutput(entry: Entry, data: Uint8Array): void {
    if (entry.disposed) return;
    entry.surface.write(data).then(
      (processed) => {
        if (processed <= 0 || entry.disposed) return;
        this.#backend.ackTerminalOutput(entry.id, processed).catch((cause: unknown) => {
          this.#reportError(`ターミナル ${entry.id} の処理済みの量を返せなかった: ${describeError(cause)}`);
        });
      },
      (cause: unknown) => {
        this.#reportError(`ターミナル ${entry.id} に出力を書き込めなかった: ${describeError(cause)}`);
      },
    );
  }

  #send(entry: Entry, data: string): void {
    this.#backend.writeTerminal(entry.id, data).catch((cause: unknown) => {
      this.#reportError(`ターミナル ${entry.id} への入力を送れなかった: ${describeError(cause)}`);
    });
  }

  #resize(entry: Entry, size: TerminalSize): void {
    entry.sentSize = size;
    this.#backend.resizeTerminal(entry.id, size.rows, size.cols).catch((cause: unknown) => {
      this.#reportError(`ターミナル ${entry.id} の寸法を変えられなかった: ${describeError(cause)}`);
    });
  }
}
