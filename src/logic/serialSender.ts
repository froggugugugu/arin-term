// 1 つの送り先への送信を直列にする(前の送信が終わるまで次を送らない)。
// ターミナルの入力(write_terminal)と寸法(resize_terminal)を、Rust に来た順のまま届けるのに使う。
// Rust はコマンドごとに別のスレッドで動き、PTY のロック(std::sync::Mutex)は来た順を保証しないので、
// 前の送信の応答を待ってから次を送ることで、それぞれの中の順序を保つ。
//
// 送信中(または準備ができる前)に来た値は merge で 1 つにまとめ、送信が終わったら次の 1 回で送る。
// 入力は連結(順序を保ち、往復の回数も減る)、寸法は最新だけ(latest)にする。

export interface SerialSenderOptions<T> {
  /** 1 回の送信。失敗(拒否・同期の例外)は onError に渡し、次へ進む。 */
  send: (value: T) => Promise<void>;
  /** 待っている値(無ければ undefined)に次の値を合わせる。 */
  merge: (pending: T | undefined, next: T) => T;
  /** 送信の失敗の報告先。 */
  onError: (cause: unknown) => void;
  /** 最初から送ってよいか。false なら setReady(true) まで溜める。既定は true。 */
  ready?: boolean;
}

export interface SerialSender<T> {
  /** 値を送る。送信中・準備前なら待たせ、待っている値と合わせる。 */
  push(value: T): void;
  /** 送ってよいかを切り替える。true にしたら待っている値を送る。 */
  setReady(ready: boolean): void;
  /** 待っている値を捨てる(送信中のものはそのまま終わる)。 */
  clear(): void;
  /** 待っている値を捨て、以後は何も送らない。 */
  close(): void;
  /** 送信中のものも待っている値も無いか。 */
  readonly idle: boolean;
}

/** 待っている値を捨てて、最新の値だけを残す(寸法用)。 */
export const latest = <T>(_pending: T | undefined, next: T): T => next;

export function createSerialSender<T>({ send, merge, onError, ready = true }: SerialSenderOptions<T>): SerialSender<T> {
  let isReady = ready;
  let closed = false;
  let inFlight = false;
  let hasPending = false;
  let pending: T | undefined;

  const flush = () => {
    if (closed || !isReady || inFlight || !hasPending) return;
    const value = pending as T;
    pending = undefined;
    hasPending = false;
    inFlight = true;
    let request: Promise<void>;
    try {
      request = send(value);
    } catch (cause) {
      request = Promise.reject(cause);
    }
    request
      .then(undefined, (cause: unknown) => onError(cause))
      .then(() => {
        inFlight = false;
        flush();
      });
  };

  return {
    push(value) {
      if (closed) return;
      if (isReady && !inFlight && !hasPending) {
        // すぐ送れる値は待ちの量ではないので、merge(上限)を通さない。
        pending = value;
      } else {
        pending = merge(hasPending ? pending : undefined, value);
      }
      hasPending = true;
      flush();
    },
    setReady(next) {
      isReady = next;
      flush();
    },
    clear() {
      pending = undefined;
      hasPending = false;
    },
    close() {
      closed = true;
      pending = undefined;
      hasPending = false;
    },
    get idle() {
      return !inFlight && !hasPending;
    },
  };
}

/**
 * 送信待ちの入力の上限(UTF-16 の文字数。4 Mi 文字は UTF-8 でおよそ 4〜12 MiB)。
 * 数 MB のログや文書の貼り付けは普通に通し、シェルが入力を読まないまま貼り付けを繰り返しても
 * 画面のメモリに際限なく溜めない量にする。
 */
export const INPUT_QUEUE_LIMIT = 4 * 1024 * 1024;

const isHighSurrogate = (code: number) => code >= 0xd800 && code <= 0xdbff;

/**
 * 入力を連結する merge。連結した長さが limit を超える分は捨て、溜まりが空から始まってから最初に超えたときだけ
 * onOverflow を呼ぶ(溜まりを送り終えたあとに超えたら、また呼ぶ)。サロゲートペアの途中では切らない。
 */
export function concatWithLimit(
  limit: number,
  onOverflow: () => void,
): (pending: string | undefined, next: string) => string {
  let reported = false;
  return (pending, next) => {
    if (pending === undefined) reported = false;
    const base = pending ?? "";
    const room = limit - base.length;
    if (next.length <= room) return base + next;
    let keep = Math.max(0, room);
    if (keep > 0 && isHighSurrogate(next.charCodeAt(keep - 1))) keep -= 1;
    if (!reported) {
      reported = true;
      onOverflow();
    }
    return base + next.slice(0, keep);
  };
}
