// ターミナルの入力の送信待ちの形(TerminalManager が使う)。
//
// 入力には文字列(xterm.js の onData。write_terminal)とバイト列(onBinary。write_terminal_bytes)の 2 種類があり、
// 同じ直列の列(src/logic/serialSender.ts)で送って来た順を保つ。送信中に来た入力は、来た順のまま 1 つの束にまとめ、
// 隣り合う同じ種類は連結する(往復の回数を減らす)。溜める量には上限を掛ける(文字列は UTF-16 の文字数、
// バイト列はバイト数で数える)。

import { concatWithLimit } from "../logic/serialSender";

/** 入力の 1 かたまり。 */
export type InputChunk =
  | { readonly kind: "text"; readonly data: string }
  | { readonly kind: "bytes"; readonly data: Uint8Array };

/** 送信待ちの入力(来た順)。 */
export type InputBatch = readonly InputChunk[];

/**
 * xterm.js の onBinary の文字列をバイト列にする。各文字の値(0〜255)が 1 バイト。
 * 255 を超える値は来ない決まりだが、来たら下位 8 ビットにする。
 */
export function binaryToBytes(data: string): Uint8Array {
  const bytes = new Uint8Array(data.length);
  for (let i = 0; i < data.length; i++) bytes[i] = data.charCodeAt(i) & 0xff;
  return bytes;
}

const sizeOf = (chunk: InputChunk) => (chunk.kind === "text" ? chunk.data.length : chunk.data.byteLength);

/**
 * 送信待ちの束に次の入力を合わせる merge(serialSender に渡す)。合計が limit を超える分は捨て、
 * 溜まりが空から始まってから最初に超えたときだけ onOverflow を呼ぶ。文字列はサロゲートペアの途中で切らない。
 */
export function inputBatchMerger(
  limit: number,
  onOverflow: () => void,
): (pending: InputBatch | undefined, next: InputBatch) => InputBatch {
  let reported = false;
  const overflow = () => {
    if (reported) return;
    reported = true;
    onOverflow();
  };
  return (pending, next) => {
    if (pending === undefined) reported = false;
    const out: InputChunk[] = [...(pending ?? [])];
    let used = out.reduce((sum, chunk) => sum + sizeOf(chunk), 0);
    for (const chunk of next) {
      const room = Math.max(0, limit - used);
      const last = out.at(-1);
      let kept: InputChunk;
      if (chunk.kind === "text") {
        // 文字列の切り方(サロゲートペアの途中で切らない)は concatWithLimit に任せる。
        let cut = false;
        const text = concatWithLimit(room, () => {
          cut = true;
        })(undefined, chunk.data);
        if (cut) overflow();
        kept = { kind: "text", data: text };
      } else {
        if (chunk.data.byteLength > room) overflow();
        kept = { kind: "bytes", data: chunk.data.subarray(0, room) };
      }
      const size = sizeOf(kept);
      if (size === 0) continue;
      used += size;
      if (last?.kind === "text" && kept.kind === "text") {
        out[out.length - 1] = { kind: "text", data: last.data + kept.data };
      } else if (last?.kind === "bytes" && kept.kind === "bytes") {
        const joined = new Uint8Array(last.data.byteLength + kept.data.byteLength);
        joined.set(last.data);
        joined.set(kept.data, last.data.byteLength);
        out[out.length - 1] = { kind: "bytes", data: joined };
      } else {
        out.push(kept);
      }
    }
    return out;
  };
}
