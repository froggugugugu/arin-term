import { describe, expect, it } from "vitest";

import { DRAG_THRESHOLD_PX, DragMachine } from "./dragMachine";
import { resolveDropTarget } from "./dropTarget";

const source = { path: "src/a.ts" };

/** 押して、しきい値を超えて動かしたところまで進める。 */
function dragging(): DragMachine {
  const machine = new DragMachine();
  machine.press(source, { x: 10, y: 10 });
  expect(machine.move({ x: 20, y: 10 })).toEqual({ type: "drag-start", source });
  return machine;
}

describe("DragMachine(ドラッグの状態)", () => {
  it("しきい値は 4px", () => {
    expect(DRAG_THRESHOLD_PX).toBe(4);
  });

  it("しきい値より小さい動きはクリックのまま", () => {
    const machine = new DragMachine();
    machine.press(source, { x: 10, y: 10 });
    expect(machine.phase).toBe("pressed");
    expect(machine.move({ x: 13, y: 10 })).toEqual({ type: "none" });
    // ちょうど 4px(斜めに 3・4 動いて距離 5 ではなく、縦に 4)はまだクリック
    expect(machine.move({ x: 10, y: 14 })).toEqual({ type: "none" });
    expect(machine.isDragging).toBe(false);
    expect(machine.release({ kind: "folder", path: "lib" }, false)).toEqual({ type: "click", source });
    expect(machine.phase).toBe("idle");
  });

  it("しきい値を超えるとドラッグになる", () => {
    const machine = new DragMachine();
    machine.press(source, { x: 10, y: 10 });
    // 斜めに 3・4 動くと距離 5 でしきい値を超える
    expect(machine.move({ x: 13, y: 14 })).toEqual({ type: "drag-start", source });
    expect(machine.phase).toBe("dragging");
    expect(machine.isDragging).toBe(true);
    expect(machine.source).toEqual(source);
    // 開始は 1 回だけ知らせる。押した位置に戻ってもドラッグのまま
    expect(machine.move({ x: 10, y: 10 })).toEqual({ type: "none" });
    expect(machine.isDragging).toBe(true);
  });

  it("フォルダの行に落とすとそのフォルダへ", () => {
    expect(dragging().release({ kind: "folder", path: "lib" }, false)).toEqual({
      type: "drop",
      result: { type: "move", source: "src/a.ts", destination: "lib" },
    });
  });

  it("option_を押して離すとコピー", () => {
    const machine = dragging();
    expect(machine.release({ kind: "folder", path: "lib" }, true)).toEqual({
      type: "drop",
      result: { type: "copy", source: "src/a.ts", destination: "lib" },
    });
    expect(machine.phase).toBe("idle");
    expect(machine.source).toBeNull();
  });

  it("ターミナルの置き場所に落とすと挿入", () => {
    expect(dragging().release({ kind: "terminal", terminalId: "t-1" }, false)).toEqual({
      type: "drop",
      result: { type: "insert", source: "src/a.ts", terminalId: "t-1" },
    });
  });

  it("落とし先が無いところで離すと何もしない", () => {
    expect(dragging().release(null, false)).toEqual({
      type: "drop",
      result: { type: "none", reason: "no-target" },
    });
  });

  it("esc_で取り消す", () => {
    const machine = dragging();
    expect(machine.cancel()).toEqual({ type: "cancelled" });
    expect(machine.phase).toBe("idle");
    expect(machine.isDragging).toBe(false);
    // 取り消したあとに離しても何も起きない
    expect(machine.release({ kind: "folder", path: "lib" }, false)).toEqual({ type: "none" });
  });

  it("ドラッグになる前の取り消しはクリックにもしない", () => {
    const machine = new DragMachine();
    machine.press(source, { x: 0, y: 0 });
    expect(machine.cancel()).toEqual({ type: "none" });
    expect(machine.release(null, false)).toEqual({ type: "none" });
  });

  it("押していないときの動きと取り消しは何もしない", () => {
    const machine = new DragMachine();
    expect(machine.move({ x: 100, y: 100 })).toEqual({ type: "none" });
    expect(machine.cancel()).toEqual({ type: "none" });
    expect(machine.phase).toBe("idle");
  });

  it("押し直すと前の状態を捨てて始め直す", () => {
    const machine = dragging();
    const other = { path: "README.md" };
    machine.press(other, { x: 50, y: 50 });
    expect(machine.phase).toBe("pressed");
    expect(machine.release(null, false)).toEqual({ type: "click", source: other });
  });
});

describe("resolveDropTarget(ドロップ先の判定)", () => {
  it("フォルダの行に落とすとそのフォルダへ", () => {
    expect(resolveDropTarget(source, { kind: "folder", path: "lib/util" }, false)).toEqual({
      type: "move",
      source: "src/a.ts",
      destination: "lib/util",
    });
  });

  it("ファイルの行に落とすとその親のフォルダへ", () => {
    expect(resolveDropTarget(source, { kind: "file", path: "lib/b.ts" }, false)).toEqual({
      type: "move",
      source: "src/a.ts",
      destination: "lib",
    });
    expect(resolveDropTarget(source, { kind: "file", path: "README.md" }, true)).toEqual({
      type: "copy",
      source: "src/a.ts",
      destination: "",
    });
  });

  it("ヘッダに落とすとルートへ", () => {
    expect(resolveDropTarget(source, { kind: "root" }, false)).toEqual({
      type: "move",
      source: "src/a.ts",
      destination: "",
    });
  });

  it("自分自身と子孫には落とせない", () => {
    const folder = { path: "src" };
    expect(resolveDropTarget(folder, { kind: "folder", path: "src" }, false)).toEqual({
      type: "none",
      reason: "self-or-descendant",
    });
    expect(resolveDropTarget(folder, { kind: "folder", path: "src/lib" }, false)).toEqual({
      type: "none",
      reason: "self-or-descendant",
    });
    expect(resolveDropTarget(folder, { kind: "file", path: "src/lib/a.ts" }, true)).toEqual({
      type: "none",
      reason: "self-or-descendant",
    });
    // 名前の前方が同じだけの別のフォルダには落とせる
    expect(resolveDropTarget(folder, { kind: "folder", path: "src2" }, false)).toEqual({
      type: "move",
      source: "src",
      destination: "src2",
    });
  });

  it("同じ親のフォルダに落とすと何もしない", () => {
    expect(resolveDropTarget(source, { kind: "folder", path: "src" }, false)).toEqual({
      type: "none",
      reason: "same-parent",
    });
    // 自分の行や同じフォルダのファイルの行に落としても同じ。⌥ でも何もしない(【仮定】複製は M6)
    expect(resolveDropTarget(source, { kind: "file", path: "src/a.ts" }, true)).toEqual({
      type: "none",
      reason: "same-parent",
    });
    expect(resolveDropTarget(source, { kind: "file", path: "src/b.ts" }, false)).toEqual({
      type: "none",
      reason: "same-parent",
    });
    expect(resolveDropTarget({ path: "README.md" }, { kind: "root" }, false)).toEqual({
      type: "none",
      reason: "same-parent",
    });
  });

  it("ターミナルの置き場所に落とすと挿入(option_は関係ない)", () => {
    expect(resolveDropTarget({ path: "src" }, { kind: "terminal", terminalId: "t-2" }, true)).toEqual({
      type: "insert",
      source: "src",
      terminalId: "t-2",
    });
  });

  it("落とし先が無ければ何もしない", () => {
    expect(resolveDropTarget(source, null, false)).toEqual({ type: "none", reason: "no-target" });
  });
});
