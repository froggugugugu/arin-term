import { describe, expect, it } from "vitest";

import type { RendererKind } from "./engine";
import { type RendererBackend, RendererControl } from "./rendererControl";

/** 記録用の偽の描画。loadWebgl の失敗と、コンテキストの喪失を起こせる。 */
function setup(requested: RendererKind, { failLoad = false }: { failLoad?: boolean } = {}) {
  const events: string[] = [];
  const reported: string[] = [];
  const changes: RendererKind[] = [];
  const losses: (() => void)[] = [];
  let loads = 0;
  const backend: RendererBackend = {
    loadWebgl(onContextLoss) {
      loads += 1;
      const n = loads;
      events.push(`load#${n}`);
      if (failLoad) throw new Error("WebGL2 not supported null");
      losses.push(onContextLoss);
      return { dispose: () => events.push(`dispose#${n}`) };
    },
  };
  const control = new RendererControl({
    requested,
    backend,
    label: "t-1",
    reportError: (message) => reported.push(message),
  });
  control.onChange((renderer) => changes.push(renderer));
  return { control, events, reported, changes, loseContext: (i: number) => losses[i]?.() };
}

describe("RendererControl(描画方式の切り替え)", () => {
  it("dom の指定では何も読み込まない(表示・非表示でも)", () => {
    const { control, events, reported } = setup("dom");

    control.show();
    control.hide();
    control.show();

    expect(control.active).toBe("dom");
    expect(control.holdsWebgl).toBe(false);
    expect(events).toEqual([]);
    expect(reported).toEqual([]);
  });

  it("webgl の指定では、表示したときに WebGL を読み込む", () => {
    const { control, events, reported } = setup("webgl");
    expect(control.active).toBe("webgl");
    expect(control.holdsWebgl).toBe(false);

    control.show();
    control.show();

    expect(events).toEqual(["load#1"]);
    expect(control.holdsWebgl).toBe(true);
    expect(reported).toEqual([]);
  });

  it("隠すと WebGL を破棄してコンテキストを返し、表示し直すと作り直す", () => {
    const { control, events } = setup("webgl");
    control.show();

    control.hide();
    control.hide();
    expect(control.holdsWebgl).toBe(false);
    // 隠している間も、表示したときの方式は webgl のまま。
    expect(control.active).toBe("webgl");

    control.show();
    expect(events).toEqual(["load#1", "dispose#1", "load#2"]);
    expect(control.holdsWebgl).toBe(true);
  });

  it("WebGL を作れなければ DOM に戻し、報告して受け取り手に知らせる。以後は作り直さない", () => {
    const { control, events, reported, changes } = setup("webgl", { failLoad: true });

    control.show();

    expect(control.active).toBe("dom");
    expect(control.holdsWebgl).toBe(false);
    expect(reported).toEqual([
      "ターミナル t-1: WebGL の描画を作れなかった(WebGL2 not supported null)ので、DOM の描画に戻した",
    ]);
    expect(changes).toEqual(["dom"]);

    control.hide();
    control.show();
    expect(events).toEqual(["load#1"]);
    expect(reported).toHaveLength(1);
  });

  it("コンテキストを失ったら WebGL を破棄して DOM に戻し、報告する", () => {
    const { control, events, reported, changes, loseContext } = setup("webgl");
    control.show();

    loseContext(0);

    expect(events).toEqual(["load#1", "dispose#1"]);
    expect(control.active).toBe("dom");
    expect(control.holdsWebgl).toBe(false);
    expect(reported).toEqual(["ターミナル t-1: WebGL の描画コンテキストを失ったので、DOM の描画に戻した"]);
    expect(changes).toEqual(["dom"]);
  });

  it("隠したあと・作り直したあとに古い描画から届いた喪失の通知は無視する", () => {
    const { control, reported, changes, loseContext } = setup("webgl");
    control.show();
    control.hide();
    loseContext(0);
    expect(reported).toEqual([]);

    control.show();
    loseContext(0);
    expect(control.active).toBe("webgl");
    expect(control.holdsWebgl).toBe(true);
    expect(changes).toEqual([]);
  });

  it("破棄すると WebGL を破棄し、以後の表示では読み込まず、受け取り手にも知らせない", () => {
    const { control, events, changes, loseContext } = setup("webgl");
    control.show();

    control.dispose();
    control.dispose();
    control.show();
    loseContext(0);

    expect(events).toEqual(["load#1", "dispose#1"]);
    expect(changes).toEqual([]);
  });

  it("外した受け取り手には知らせない", () => {
    const { control } = setup("webgl", { failLoad: true });
    const seen: RendererKind[] = [];
    const off = control.onChange((r) => seen.push(r));
    off();

    control.show();

    expect(seen).toEqual([]);
  });
});
