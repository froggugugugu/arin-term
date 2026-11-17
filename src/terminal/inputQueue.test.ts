import { describe, expect, it } from "vitest";

import { binaryToBytes, type InputBatch, inputBatchMerger } from "./inputQueue";

const text = (data: string) => ({ kind: "text", data }) as const;
const bytes = (...data: number[]) => ({ kind: "bytes", data: Uint8Array.from(data) }) as const;
const plain = (batch: InputBatch) => batch.map((c) => (c.kind === "text" ? c.data : [...c.data]));

describe("binaryToBytes(onBinary の文字列をバイト列にする)", () => {
  it("各文字の値をそのまま 1 バイトにする", () => {
    expect([...binaryToBytes("\x00\x1b\x7f\x80\xff")]).toEqual([0, 0x1b, 0x7f, 0x80, 0xff]);
    expect([...binaryToBytes("")]).toEqual([]);
  });

  it("255 を超える値は下位 8 ビットにする", () => {
    expect([...binaryToBytes("Ł")]).toEqual([0x41]);
  });
});

describe("inputBatchMerger(送信待ちの入力をまとめる)", () => {
  it("来た順を保ち、隣り合う同じ種類は連結する", () => {
    const merge = inputBatchMerger(100, () => {});
    let batch = merge(undefined, [text("a")]);
    batch = merge(batch, [text("b")]);
    batch = merge(batch, [bytes(1)]);
    batch = merge(batch, [bytes(2, 3)]);
    batch = merge(batch, [text("c")]);

    expect(plain(batch)).toEqual(["ab", [1, 2, 3], "c"]);
  });

  it("上限を超える分は捨て、溜まりが空から始まるまで 1 回だけ知らせる", () => {
    let overflows = 0;
    const merge = inputBatchMerger(4, () => {
      overflows += 1;
    });
    let batch = merge(undefined, [text("abc")]);
    batch = merge(batch, [bytes(1, 2)]);
    batch = merge(batch, [text("z")]);

    expect(plain(batch)).toEqual(["abc", [1]]);
    expect(overflows).toBe(1);

    merge(undefined, [text("12345")]);
    expect(overflows).toBe(2);
  });

  it("文字列はサロゲートペアの途中で切らない", () => {
    const merge = inputBatchMerger(3, () => {});

    expect(plain(merge(undefined, [text("ab😀")]))).toEqual(["ab"]);
  });
});
