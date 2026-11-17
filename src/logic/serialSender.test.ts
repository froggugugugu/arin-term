import { describe, expect, it } from "vitest";

import { concatWithLimit, createSerialSender, INPUT_QUEUE_LIMIT, latest, type SerialSender } from "./serialSender";

/** 解決を遅らせられる偽の送信。送った値を順に記録し、テストが 1 件ずつ成功・失敗させる。 */
function deferredSend<T>() {
  const sent: T[] = [];
  const waiting: { resolve: () => void; reject: (cause: unknown) => void }[] = [];
  const send = (value: T) => {
    sent.push(value);
    return new Promise<void>((resolve, reject) => waiting.push({ resolve, reject }));
  };
  const settle = async (outcome: "ok" | { fail: unknown }) => {
    const next = waiting.shift();
    if (next === undefined) throw new Error("送信中のものが無い");
    if (outcome === "ok") next.resolve();
    else next.reject(outcome.fail);
    await flush();
  };
  return { sent, send, settle, inFlight: () => waiting.length };
}

/** 溜まっている Promise の続きを流し切る(src/logic は DOM・タイマーの型を読まないので、マイクロタスクで回す)。 */
const flush = async () => {
  for (let i = 0; i < 20; i++) await Promise.resolve();
};

function setupText(ready = true) {
  const d = deferredSend<string>();
  const errors: unknown[] = [];
  const sender: SerialSender<string> = createSerialSender<string>({
    send: d.send,
    merge: (pending, next) => (pending ?? "") + next,
    onError: (cause) => errors.push(cause),
    ready,
  });
  return { ...d, errors, sender };
}

describe("createSerialSender(送信の直列化)", () => {
  it("送信中でなければすぐ送る", () => {
    const { sender, sent } = setupText();

    sender.push("a");

    expect(sent).toEqual(["a"]);
  });

  it("前の送信が終わるまで次を送らず、待つあいだの値はまとめて次の 1 回で送る(順序を保つ)", async () => {
    const { sender, sent, settle } = setupText();

    sender.push("a");
    sender.push("b");
    sender.push("c");
    expect(sent).toEqual(["a"]);

    await settle("ok");
    expect(sent).toEqual(["a", "bc"]);
    sender.push("d");
    expect(sent).toEqual(["a", "bc"]);

    await settle("ok");
    expect(sent).toEqual(["a", "bc", "d"]);
    await settle("ok");
    expect(sender.idle).toBe(true);
  });

  it("送信が失敗したら報告し、待っていた値を次に送る(詰まらない)", async () => {
    const { sender, sent, settle, errors } = setupText();

    sender.push("a");
    sender.push("b");
    await settle({ fail: "書けなかった" });

    expect(errors).toEqual(["書けなかった"]);
    expect(sent).toEqual(["a", "b"]);
    await settle("ok");
    sender.push("c");
    expect(sent).toEqual(["a", "b", "c"]);
  });

  it("送信の関数が同期で投げても報告し、次へ進む", async () => {
    const errors: unknown[] = [];
    const sent: string[] = [];
    let first = true;
    const sender = createSerialSender<string>({
      send: (value) => {
        sent.push(value);
        if (first) {
          first = false;
          throw new Error("同期の失敗");
        }
        return Promise.resolve();
      },
      merge: (pending, next) => (pending ?? "") + next,
      onError: (cause) => errors.push(cause),
    });

    sender.push("a");
    sender.push("b");
    await flush();

    expect(errors.map((e) => (e as Error).message)).toEqual(["同期の失敗"]);
    expect(sent).toEqual(["a", "b"]);
  });

  it("準備ができるまでは溜め、準備ができたらまとめて 1 回で送る", async () => {
    const { sender, sent, settle } = setupText(false);

    sender.push("a");
    sender.push("b");
    expect(sent).toEqual([]);
    expect(sender.idle).toBe(false);

    sender.setReady(true);
    expect(sent).toEqual(["ab"]);
    await settle("ok");
    expect(sender.idle).toBe(true);
  });

  it("clear は待っている値を捨てる(送信中のものはそのまま終わる)", async () => {
    const { sender, sent, settle } = setupText();

    sender.push("a");
    sender.push("b");
    sender.clear();
    await settle("ok");

    expect(sent).toEqual(["a"]);
    expect(sender.idle).toBe(true);
  });

  it("close のあとは、送信中のものが終わっても次を送らず、新しい値も受け付けない", async () => {
    const { sender, sent, settle } = setupText();

    sender.push("a");
    sender.push("b");
    sender.close();
    sender.push("c");
    sender.setReady(true);
    await settle("ok");

    expect(sent).toEqual(["a"]);
  });

  it("latest で合わせると、送信中に来た値のうち最新の 1 つだけを次に送る", async () => {
    const d = deferredSend<number>();
    const sender = createSerialSender<number>({ send: d.send, merge: latest, onError: () => {} });

    sender.push(1);
    sender.push(2);
    sender.push(3);
    await d.settle("ok");
    await d.settle("ok");

    expect(d.sent).toEqual([1, 3]);
  });
});

describe("concatWithLimit(待ちの入力の上限)", () => {
  it("上限までは連結する", () => {
    const merge = concatWithLimit(5, () => {
      throw new Error("報告しない");
    });

    expect(merge(undefined, "ab")).toBe("ab");
    expect(merge("ab", "cde")).toBe("abcde");
  });

  it("上限を超えた分は捨て、溜まりが空になるまでは 1 回だけ報告する", () => {
    let reported = 0;
    const merge = concatWithLimit(4, () => {
      reported += 1;
    });

    let pending = merge(undefined, "abc");
    pending = merge(pending, "def");
    pending = merge(pending, "ghi");

    expect(pending).toBe("abcd");
    expect(reported).toBe(1);

    // 送って溜まりが空になった(次は undefined から始まる)あとに超えたら、また報告する。
    expect(merge(undefined, "123456")).toBe("1234");
    expect(reported).toBe(2);
  });

  it("サロゲートペアの途中では切らない", () => {
    const merge = concatWithLimit(3, () => {});

    expect(merge("ab", "😀")).toBe("ab");
    expect(merge(undefined, "a😀b")).toBe("a😀");
  });

  it("既定の上限は大きな貼り付けを通せる量(4 Mi 文字)", () => {
    expect(INPUT_QUEUE_LIMIT).toBe(4 * 1024 * 1024);
  });
});
