/** 予約する関数。run を後で 1 回呼ぶ予約をし、予約を取り消す関数を返す(本物は setTimeout)。 */
export type Schedule = (run: () => void) => () => void;

export interface Coalescer {
  /** 実行を求める。予約中なら予約をやり直し、最後の要求から待つ。 */
  request(): void;
  /** 予約を取り消し、以後の要求を受け付けない。 */
  cancel(): void;
}

/**
 * 連続する要求を 1 回の実行にまとめる(最後の要求から一定の時間を待って実行する)。
 * 置き場所の大きさが続けて変わるとき、寸法合わせと PTY への通知を 1 回にするのに使う。
 */
export function createCoalescer(run: () => void, schedule: Schedule): Coalescer {
  let cancelPending: (() => void) | null = null;
  let stopped = false;
  return {
    request() {
      if (stopped) return;
      cancelPending?.();
      cancelPending = schedule(() => {
        cancelPending = null;
        run();
      });
    },
    cancel() {
      stopped = true;
      cancelPending?.();
      cancelPending = null;
    },
  };
}
