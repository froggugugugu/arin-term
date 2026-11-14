import { describe, expect, it } from "vitest";

import { createCoalescer } from "./coalesce";

/** 手で進める偽の予約。 */
function fakeScheduler() {
  const pending: { run: () => void; cancelled: boolean }[] = [];
  return {
    schedule: (run: () => void) => {
      const entry = { run, cancelled: false };
      pending.push(entry);
      return () => {
        entry.cancelled = true;
      };
    },
    /** 予約のうち、取り消されていないものを実行する。 */
    fire() {
      for (const entry of pending.splice(0)) if (!entry.cancelled) entry.run();
    },
    get live() {
      return pending.filter((e) => !e.cancelled).length;
    },
  };
}

describe("createCoalescer(連続する要求をまとめる)", () => {
  it("続けて要求しても、予約が実行されたときに 1 回だけ実行する", () => {
    const scheduler = fakeScheduler();
    let runs = 0;
    const c = createCoalescer(() => (runs += 1), scheduler.schedule);

    c.request();
    c.request();
    c.request();
    expect(runs).toBe(0);
    scheduler.fire();

    expect(runs).toBe(1);
  });

  it("要求のたびに予約をやり直す(最後の要求から待つ)", () => {
    const scheduler = fakeScheduler();
    const c = createCoalescer(() => {}, scheduler.schedule);

    c.request();
    c.request();

    expect(scheduler.live).toBe(1);
  });

  it("実行のあとの要求は、また 1 回実行する", () => {
    const scheduler = fakeScheduler();
    let runs = 0;
    const c = createCoalescer(() => (runs += 1), scheduler.schedule);

    c.request();
    scheduler.fire();
    c.request();
    scheduler.fire();

    expect(runs).toBe(2);
  });

  it("取り消すと予約中の実行をしない。取り消しのあとの要求は受け付けない", () => {
    const scheduler = fakeScheduler();
    let runs = 0;
    const c = createCoalescer(() => (runs += 1), scheduler.schedule);

    c.request();
    c.cancel();
    c.request();
    scheduler.fire();

    expect(runs).toBe(0);
    expect(scheduler.live).toBe(0);
  });
});
