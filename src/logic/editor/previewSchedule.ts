// プレビューの解析の予約(ADR-015 の 2 の 3)。DOM を使わない純粋な部品で、時計は外から受け取る。
//
// 大きい文書でキーのたびに解析すると重いので、プレビューが見えているエディタだけを、最後の変更から PREVIEW_DELAY_MS 後に
// 1 回解析する。隠しているあいだの変更は覚えておき、見えるようになったときに待たずに 1 回解析する
// (見えた瞬間に古い中身を出さないため)。エディタごとに別に数える。

/** 最後の変更から解析までの待ち(ms)。 */
export const PREVIEW_DELAY_MS = 150;

/** `run` を `ms` 後に 1 回呼ぶ予約をし、取り消す関数を返す(本物は setTimeout)。 */
export type PreviewTimer = (run: () => void, ms: number) => () => void;

export interface PreviewSchedule {
  /** プレビューが見えるか。見えるようになり、前の解析のあと変わっていれば(または初めてなら)すぐ解析する。隠すと予約を取り消す。 */
  setVisible(id: string, visible: boolean): void;
  /** 文書が変わった。見えていれば最後の変更から待って解析する。隠していれば覚えるだけ。 */
  changed(id: string): void;
  /** エディタを閉じた。予約を取り消して忘れる。 */
  forget(id: string): void;
  /** すべての予約を取り消し、以後は何もしない。 */
  dispose(): void;
}

interface Entry {
  visible: boolean;
  /** 前の解析のあと変わった(初めは真)。 */
  stale: boolean;
  cancel: (() => void) | null;
}

export function createPreviewSchedule(parse: (id: string) => void, timer: PreviewTimer): PreviewSchedule {
  const entries = new Map<string, Entry>();
  let disposed = false;

  const entryOf = (id: string): Entry => {
    let entry = entries.get(id);
    if (entry === undefined) {
      entry = { visible: false, stale: true, cancel: null };
      entries.set(id, entry);
    }
    return entry;
  };
  const cancel = (entry: Entry) => {
    entry.cancel?.();
    entry.cancel = null;
  };
  const run = (id: string, entry: Entry) => {
    cancel(entry);
    entry.stale = false;
    parse(id);
  };

  return {
    setVisible(id, visible) {
      if (disposed) return;
      const entry = entryOf(id);
      if (entry.visible === visible) return;
      entry.visible = visible;
      if (!visible) {
        // 予約していた変更は解析していないので、変わったままにしておく
        if (entry.cancel !== null) entry.stale = true;
        cancel(entry);
        return;
      }
      if (entry.stale) run(id, entry);
    },
    changed(id) {
      if (disposed) return;
      const entry = entryOf(id);
      entry.stale = true;
      if (!entry.visible) return;
      cancel(entry);
      entry.cancel = timer(() => {
        entry.cancel = null;
        run(id, entry);
      }, PREVIEW_DELAY_MS);
    },
    forget(id) {
      const entry = entries.get(id);
      if (entry === undefined) return;
      cancel(entry);
      entries.delete(id);
    },
    dispose() {
      disposed = true;
      for (const entry of entries.values()) cancel(entry);
      entries.clear();
    },
  };
}
