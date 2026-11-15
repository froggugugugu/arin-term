// ターミナルの ID とサーフェスの対応と、サーフェスと Rust の配線(設計書 §5.1、§7.1「ターミナルの入出力」)。
//
// - 1 つの ID に 1 つのサーフェスを持ち続ける。非表示のあいだも破棄しない(設計書 §1.3 #4)。
//   破棄するのは呼び出し側が dispose を呼んだときだけ
// - 出力: チャネル → surface.write → 取り込みが済んだ量を ack_terminal_output で返す(流量制御)
// - 入力: surface.onInput → write_terminal。寸法: surface.onResize → resize_terminal。
//   どちらもターミナルごとに直列に送る(前の送信の応答を待ってから次を送る)。Rust はコマンドごとに別のスレッドで動き、
//   PTY のロックは来た順を保証しないので、並行に送ると "a"→"b" が "ba" で届きうる。送信中に来た入力は連結し、
//   寸法は最新だけを次の 1 回で送る(src/logic/serialSender.ts)。入力と寸法のあいだの順序は保たない
// - シェルが終わった(写しの状態が exited / failed)ターミナルには入力も寸法も送らない(setStatus で伝える)
// - 置き場所(TerminalHost)への要素の付け外しもここで行い、React には要素を管理させない(ADR-003)
// - コピー・ペースト: 画面のイベント → handleClipboard → フォーカスのあるサーフェスの選択の取得・貼り付け(§9.1)
// - メニューの「すべてを選択」: menu-command イベント → handleMenuCommand → フォーカスのあるサーフェスの全選択
// - バイト列の入力: surface.onBinary → write_terminal_bytes。文字列の入力と同じ直列の列に入れる(順序を保つ)
// - 写しとの同期(M2): retain で写しから消えたターミナルを破棄する(タブを閉じた・シェルが終わってタブが閉じた。
//   状態は exited を経ずに消えることがある)。present で表示中の 1 枚と、隠して取り込みだけ続けるターミナルを伝える
//   (FR-042)。隠したまま作ったサーフェスは、最後に表示していたターミナルの寸法で接続する(設計書 §2.2)
//
// エンジンと通信は外から受け取る。本物は src/terminal/xterm/ と src/ipc/tauri.ts、
// テストでは src/test/fakeTerminalEngine.ts と src/test/fakeIpc.ts を渡す。

import type { ArinIpc, OutputSubscription } from "../ipc/client";
import { describeError } from "../logic/describeError";
import { type ClipboardFocus, type ClipboardKind, routeClipboard } from "../logic/keys/clipboard";
import { type MenuCommand, routeMenuCommand } from "../logic/keys/menuCommand";
import { createSerialSender, INPUT_QUEUE_LIMIT, latest, type SerialSender } from "../logic/serialSender";
import type { TerminalStatus } from "../ipc/generated/TerminalStatus";
import type { RendererKind, TerminalEngine, TerminalId, TerminalSize, TerminalSurface } from "./engine";
import { binaryToBytes, type InputBatch, inputBatchMerger } from "./inputQueue";

/** 画面のコピー・ペーストのイベントの clipboardData のうち、使う部分(DataTransfer はこの形を満たす)。 */
export interface ClipboardText {
  getData(format: string): string;
  setData(format: string, data: string): void;
}

const TEXT_FORMAT = "text/plain";

/** ターミナルが使う Rust の操作(ArinIpc の一部)。 */
export type TerminalBackend = Pick<
  ArinIpc,
  "connectTerminal" | "writeTerminal" | "writeTerminalBytes" | "ackTerminalOutput" | "resizeTerminal"
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
  /**
   * 入力(文字列とバイト列)の送信。接続の応答を待つあいだと送信中に来た入力を、上限(INPUT_QUEUE_LIMIT)まで
   * 来た順に連結して溜める。
   */
  readonly input: SerialSender<InputBatch>;
  /** 寸法の送信。送信中に来たら最新だけを残す。 */
  readonly resize: SerialSender<TerminalSize>;
  /** 最後に Rust へ伝えた寸法。 */
  sentSize: TerminalSize | null;
  /** いま要素を入れている置き場所。 */
  slot: TerminalSlot | null;
  /** 表示しているか(setVisible で最後に伝えた値。作った直後は表示)。 */
  visible: boolean;
  /**
   * present で隠したまま接続することにしたが、まだ接続していない(表示中のターミナルの寸法が分かるまで待つ)。
   * 表示に変わったら、置き場所に入れた・寸法を合わせたときに接続する。
   */
  wantsConnect: boolean;
  /** present が接続を始めたか(写しが届くたびに試し直さないため)。 */
  autoConnected: boolean;
  disposed: boolean;
}

const sameSize = (a: TerminalSize, b: TerminalSize) => a.rows === b.rows && a.cols === b.cols;

export class TerminalManager {
  readonly #engine: TerminalEngine;
  readonly #backend: TerminalBackend;
  readonly #reportError: (message: string) => void;
  readonly #entries = new Map<TerminalId, Entry>();
  /** 写しで伝えられた状態の種類(サーフェスを作る前に伝えられたものも持つ)。 */
  readonly #statuses = new Map<TerminalId, TerminalStatus["kind"]>();
  /** present で伝えられた表示中のターミナル。present を使わない(検証画面)なら null のまま。 */
  #shown: TerminalId | null = null;
  /** 最後に表示していたターミナルの寸法。隠したまま作るサーフェスをこの寸法で始める。 */
  #lastShownSize: TerminalSize | null = null;

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

  /**
   * 置き場所の大きさに合わせて行・桁を決め直す。変われば寸法のコマンドが送られる。
   * 隠したまま接続を待たせていたターミナルが表示に変わっていれば、合わせた寸法で接続する。
   */
  fit(id: TerminalId): void {
    const entry = this.#entries.get(id);
    if (entry === undefined) return;
    entry.surface.fit();
    if (entry.wantsConnect && entry.visible) void this.connect(id);
  }

  /**
   * 表示と非表示を切り替える(設計書 §2.2「非表示のインスタンス」)。隠すとサーフェスは描画の資源を手放す。
   * 表示に戻したら寸法を合わせ直す(隠している間に置き場所の大きさが変わっていてもよいように)。
   * 隠している間も出力は取り込み続け、処理済みの量を Rust へ返す(接続も切らない)。
   * 持っていない ID なら何もしない。
   */
  setVisible(id: TerminalId, visible: boolean): void {
    const entry = this.#entries.get(id);
    if (entry === undefined) return;
    entry.visible = visible;
    entry.surface.setVisible(visible);
    if (visible) {
      entry.surface.fit();
      this.#noteShownSize(entry);
    }
  }

  /**
   * 写しにあるターミナルだけを残す(main.tsx が写しの更新ごとに呼ぶ)。無いものはサーフェスを破棄し、出力の受け取りをやめる。
   * タブを閉じたときと、シェルが終わってタブが閉じたときに消える(状態は exited を経ずに消えることがある)。
   * 破棄の失敗は報告して、残りの破棄を続ける。
   */
  retain(ids: Iterable<TerminalId>): void {
    const keep = new Set(ids);
    for (const id of [...this.#entries.keys()]) {
      if (keep.has(id)) continue;
      try {
        this.dispose(id);
      } catch (cause) {
        this.#reportError(`ターミナル ${id} を破棄できなかった: ${describeError(cause)}`);
      }
    }
    for (const id of [...this.#statuses.keys()]) {
      if (!keep.has(id)) this.#statuses.delete(id);
    }
    if (this.#shown !== null && !keep.has(this.#shown)) this.#shown = null;
  }

  /**
   * 表示中のターミナル(選択中のプロジェクトの表示中のタブ。無ければ null)と、隠したまま出力を取り込み続ける
   * ターミナルを伝える(FR-042。main.tsx が写しの更新ごとに呼ぶ。検証画面では呼ばない)。
   * - 表示中: 隠していれば表示に戻す。置き場所に入れるのと接続は TerminalHost(attach)が行う
   * - 隠すもの: 表示していれば隠す。サーフェスが無ければ隠したまま作り、接続する(1 回だけ試す)。
   *   接続の寸法は最後に表示していたターミナルの寸法。表示中のターミナルがまだ寸法を決めていなければ、
   *   決まるまで接続を待たせる。表示中が無く寸法も分からなければ、エンジンの既定の寸法で接続する
   */
  present(shown: TerminalId | null, background: readonly TerminalId[]): void {
    this.#shown = shown;
    if (shown !== null) {
      const entry = this.#entries.get(shown);
      if (entry !== undefined && !entry.visible) this.setVisible(shown, true);
    }
    for (const id of background) {
      if (id === shown) continue;
      const entry = this.#entries.get(id) ?? this.#entry(id, { visible: false });
      if (entry.visible) this.setVisible(id, false);
      if (entry.autoConnected) continue;
      entry.autoConnected = true;
      void this.connect(id);
    }
  }

  /** ID のサーフェスへ入力のフォーカスを移す。持っていない ID なら何もしない。 */
  focus(id: TerminalId): void {
    this.#entries.get(id)?.surface.focus();
  }

  /** ID のサーフェスのいまの描画方式。サーフェスが無ければ undefined(作らない。描画中に呼ばれるため)。 */
  rendererOf(id: TerminalId): RendererKind | undefined {
    return this.#entries.get(id)?.surface.renderer();
  }

  /** ID のサーフェスの描画方式が変わったときに受け取る。サーフェスが無ければ作る。外す関数を返す。 */
  onRendererChange(id: TerminalId, listener: (renderer: RendererKind) => void): () => void {
    return this.#entry(id).surface.onRendererChange(listener);
  }

  /**
   * 写しにあるターミナルの状態を伝える(main.tsx が写しの更新ごとに呼ぶ)。
   * exited / failed のあいだは入力と寸法を送らず、送信待ちの入力も捨てる(終わったシェルへの書き込みは失敗し、
   * キーを押すたびに失敗を報告して報告の上限を使い切ってしまうため)。
   * failed は次の接続で起動をやり直せる(Rust の仕様)ので、connect を始めたら解く。サーフェスが無くても覚えておく。
   */
  setStatus(id: TerminalId, status: TerminalStatus): void {
    this.#statuses.set(id, status.kind);
    if (!this.#stopped(id)) return;
    const entry = this.#entries.get(id);
    entry?.input.clear();
    entry?.resize.clear();
  }

  /**
   * いまの行・桁で出力のチャネルをつなぐ。接続中・接続済みなら何もしない(同じ Promise を返す)。
   * 失敗は報告し、次に呼べば試し直す。この Promise は失敗しない。
   */
  connect(id: TerminalId): Promise<void> {
    const entry = this.#entry(id);
    if (entry.state === "connected") return Promise.resolve();
    if (entry.connecting !== null) return entry.connecting;

    if (!entry.visible) {
      // 隠したまま接続する: 表示中のターミナルの寸法で始める。まだ分からなければ分かるまで待たせる。
      if (this.#shown !== null && this.#lastShownSize === null) {
        entry.wantsConnect = true;
        return Promise.resolve();
      }
      const shownSize = this.#lastShownSize;
      if (shownSize !== null && !sameSize(shownSize, entry.surface.size())) entry.surface.resize(shownSize);
    }
    entry.wantsConnect = false;
    const size = entry.surface.size();
    // 起動の失敗のあとの接続は、Rust がもう一度起動を試す。
    if (this.#statuses.get(id) === "failed") this.#statuses.delete(id);
    entry.state = "connecting";
    const subscription = this.#backend.connectTerminal(
      id,
      size.rows,
      size.cols,
      (data) => this.#onOutput(entry, data),
      (message) => {
        // Tauri 2.12.5 は生のバイト列を常に ArrayBuffer で渡すので、起きる経路は見つかっていない。
        // 取り込まなかった分の処理済みは返らない(回復は先送り。計画の M1 の見送り項目の割当)。
        if (entry.disposed) return;
        this.#reportError(`ターミナル ${id} の出力をバイト列にできなかったので捨てた(${typeof message})`);
      },
    );
    entry.subscription = subscription;
    entry.sentSize = size;
    if (entry.visible) {
      this.#noteShownSize(entry);
      this.#connectWaiting();
    }
    entry.connecting = subscription.connected.then(
      (outcome) => {
        entry.connecting = null;
        if (entry.disposed) return;
        entry.state = "connected";
        entry.input.setReady(true);
        const now = entry.surface.size();
        // 再接続で Rust が寸法を PTY に反映できなかったときは、変わっていなくても 1 回送り直す。
        if (!outcome.sizeApplied || entry.sentSize === null || !sameSize(entry.sentSize, now)) {
          this.#resize(entry, now);
        }
      },
      (cause: unknown) => {
        entry.connecting = null;
        entry.subscription = null;
        entry.sentSize = null;
        entry.input.clear();
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

  /**
   * メニューから届いた操作(menu-command イベント)を、フォーカスのあるターミナルへ振り分ける(設計書 §9.1)。
   * 「すべてを選択」はフォーカスのあるターミナルの全選択。処理したら true。フォーカスがターミナルに無ければ false
   * (何もしない)。
   */
  handleMenuCommand(command: MenuCommand): boolean {
    const surface = this.#focusedSurface();
    switch (routeMenuCommand(command, { kind: surface === undefined ? "other" : "terminal" })) {
      case "select-all-terminal":
        surface?.selectAll();
        return true;
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
    entry.input.close();
    entry.resize.close();
    entry.subscription?.close();
    entry.subscription = null;
    entry.slot?.removeChild(entry.surface.element);
    entry.slot = null;
    entry.surface.dispose();
    return true;
  }

  #entry(id: TerminalId, init: { visible: boolean } = { visible: true }): Entry {
    const existing = this.#entries.get(id);
    if (existing) return existing;
    const surface = this.#engine.createSurface(id);
    if (!init.visible) surface.setVisible(false);
    const report = this.#reportError;
    const entry: Entry = {
      id,
      surface,
      state: "idle",
      subscription: null,
      connecting: null,
      // 接続の応答までは溜める(ready: false)。
      input: createSerialSender<InputBatch>({
        send: async (batch) => {
          for (const chunk of batch) {
            if (chunk.kind === "text") await this.#backend.writeTerminal(id, chunk.data);
            else await this.#backend.writeTerminalBytes(id, chunk.data);
          }
        },
        merge: inputBatchMerger(INPUT_QUEUE_LIMIT, () => {
          report(`ターミナル ${id} の送信待ちの入力が上限(${INPUT_QUEUE_LIMIT} 文字)を超えたので、超えた分を捨てた`);
        }),
        onError: (cause) => report(`ターミナル ${id} への入力を送れなかった: ${describeError(cause)}`),
        ready: false,
      }),
      resize: createSerialSender<TerminalSize>({
        send: (size) => this.#backend.resizeTerminal(id, size.rows, size.cols),
        merge: latest,
        onError: (cause) => report(`ターミナル ${id} の寸法を変えられなかった: ${describeError(cause)}`),
      }),
      sentSize: null,
      slot: null,
      visible: init.visible,
      wantsConnect: false,
      autoConnected: false,
      disposed: false,
    };
    const pushInput = (batch: InputBatch) => {
      if (entry.disposed || this.#stopped(id)) return;
      // 接続していない(接続の失敗のあとなど)ときの入力は送り先が無いので捨てる。
      if (entry.state !== "idle") entry.input.push(batch);
    };
    surface.onInput((data) => pushInput([{ kind: "text", data }]));
    surface.onBinary((data) => pushInput([{ kind: "bytes", data: binaryToBytes(data) }]));
    surface.onResize((cols, rows) => {
      if (entry.disposed) return;
      this.#noteShownSize(entry);
      // 接続を待つあいだの変更は、接続のあとにまとめて送る。
      if (entry.state !== "connected") return;
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

  /**
   * 表示しているターミナル(置き場所に入っているもの)の寸法を、隠したまま作るサーフェスのために覚える。
   */
  #noteShownSize(entry: Entry): void {
    // 置き場所に入っていないサーフェスの寸法は、置き場所に合わせたものではないので使わない。
    if (!entry.visible || entry.disposed || entry.slot === null) return;
    this.#lastShownSize = entry.surface.size();
  }

  /** 表示中のターミナルの寸法が分かるまで待たせていた、隠したターミナルを接続する。 */
  #connectWaiting(): void {
    for (const waiting of [...this.#entries.values()]) {
      if (waiting.wantsConnect && !waiting.visible) void this.connect(waiting.id);
    }
  }

  /** シェルが終わっている(写しの状態が exited / failed)か。 */
  #stopped(id: TerminalId): boolean {
    const kind = this.#statuses.get(id);
    return kind === "exited" || kind === "failed";
  }

  #resize(entry: Entry, size: TerminalSize): void {
    if (this.#stopped(entry.id)) return;
    entry.sentSize = size;
    entry.resize.push(size);
  }
}
