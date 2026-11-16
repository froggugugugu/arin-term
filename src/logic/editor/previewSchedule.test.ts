import { describe, expect, it } from "vitest";

import { createPreviewSchedule, PREVIEW_DELAY_MS, type PreviewTimer } from "./previewSchedule";

/** 手で進める偽の時計。 */
function fakeTimer() {
  let now = 0;
  let pending: { at: number; run: () => void; cancelled: boolean }[] = [];
  const timer: PreviewTimer = (run, ms) => {
    const entry = { at: now + ms, run, cancelled: false };
    pending.push(entry);
    return () => {
      entry.cancelled = true;
    };
  };
  const advance = (ms: number) => {
    now += ms;
    const due = pending.filter((p) => !p.cancelled && p.at <= now);
    pending = pending.filter((p) => !due.includes(p) && !p.cancelled);
    for (const p of due) p.run();
  };
  return { timer, advance, pendingCount: () => pending.filter((p) => !p.cancelled).length };
}

describe("createPreviewSchedule(プレビューの解析の予約。ADR-015 の 2 の 3)", () => {
  it("待ちは_150_ms", () => {
    expect(PREVIEW_DELAY_MS).toBe(150);
  });

  it("続けて打っても解析は最後から_150_ms_後に_1_回", () => {
    const clock = fakeTimer();
    const parsed: string[] = [];
    const schedule = createPreviewSchedule((id) => parsed.push(id), clock.timer);
    schedule.setVisible("e1", true);
    parsed.length = 0;

    for (let i = 0; i < 5; i++) {
      schedule.changed("e1");
      clock.advance(100);
    }
    expect(parsed).toEqual([]);
    clock.advance(49);
    expect(parsed).toEqual([]);
    clock.advance(1);
    expect(parsed).toEqual(["e1"]);
    clock.advance(1000);
    expect(parsed).toEqual(["e1"]);
  });

  it("見えるようになったら待たずに_1_回解析し_隠しているあいだは解析しない", () => {
    const clock = fakeTimer();
    const parsed: string[] = [];
    const schedule = createPreviewSchedule((id) => parsed.push(id), clock.timer);

    schedule.changed("e1");
    clock.advance(1000);
    expect(parsed).toEqual([]);

    schedule.setVisible("e1", true);
    expect(parsed).toEqual(["e1"]);
    // 同じ値で呼んでも解析し直さない
    schedule.setVisible("e1", true);
    expect(parsed).toEqual(["e1"]);
  });

  it("隠すと予約を取り消し_変わっていなければ見え直しても解析しない", () => {
    const clock = fakeTimer();
    const parsed: string[] = [];
    const schedule = createPreviewSchedule((id) => parsed.push(id), clock.timer);
    schedule.setVisible("e1", true);
    schedule.changed("e1");
    schedule.setVisible("e1", false);
    clock.advance(1000);
    expect(parsed).toEqual(["e1"]);
    expect(clock.pendingCount()).toBe(0);

    // 隠す前の変更は解析していないので、見え直したら解析する
    schedule.setVisible("e1", true);
    expect(parsed).toEqual(["e1", "e1"]);
    schedule.setVisible("e1", false);
    schedule.setVisible("e1", true);
    expect(parsed).toEqual(["e1", "e1"]);
  });

  it("エディタごとに別に数える", () => {
    const clock = fakeTimer();
    const parsed: string[] = [];
    const schedule = createPreviewSchedule((id) => parsed.push(id), clock.timer);
    schedule.setVisible("e1", true);
    schedule.setVisible("e2", true);
    parsed.length = 0;

    schedule.changed("e1");
    clock.advance(100);
    schedule.changed("e2");
    clock.advance(50);
    expect(parsed).toEqual(["e1"]);
    clock.advance(100);
    expect(parsed).toEqual(["e1", "e2"]);
  });

  it("忘れると予約を取り消し_ほかは続く_破棄で全部止まる", () => {
    const clock = fakeTimer();
    const parsed: string[] = [];
    const schedule = createPreviewSchedule((id) => parsed.push(id), clock.timer);
    schedule.setVisible("e1", true);
    schedule.setVisible("e2", true);
    parsed.length = 0;
    schedule.changed("e1");
    schedule.changed("e2");
    schedule.forget("e1");
    clock.advance(150);
    expect(parsed).toEqual(["e2"]);

    schedule.changed("e2");
    schedule.dispose();
    clock.advance(150);
    schedule.changed("e2");
    schedule.setVisible("e3", true);
    clock.advance(150);
    expect(parsed).toEqual(["e2"]);
  });
});
