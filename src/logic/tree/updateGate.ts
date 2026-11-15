// 更新の保留(設計書 §5.1 の `UpdateGate`、FR-037、計画 T41a)。DOM を使わない純粋な関数。
//
// ドラッグ中・名前の入力中・コンテキストメニューの表示中・確認ダイアログの表示中(【仮定】18)は、
// 外部の変更でツリーの行を動かさない。保留の理由が 1 つでも残っていれば、読み直すフォルダを積むだけにし、
// すべて解けたら積んだフォルダを重なりを除いて一度に返す(呼び出し側がまとめて読み直す)。

/** 保留の理由。 */
export type HoldReason = "drag" | "nameInput" | "contextMenu" | "dialog";

/** 保留の状態。 */
export interface UpdateGate {
  readonly holds: ReadonlySet<HoldReason>;
  /** 積んだ、読み直すフォルダ(ルート基準の相対パス。届いた順、重なりなし)。 */
  readonly pending: readonly string[];
}

/** 保留の結果。`flush` は今すぐ読み直すフォルダ(無ければ空)。 */
export interface GateResult {
  readonly gate: UpdateGate;
  readonly flush: readonly string[];
}

/** 保留の無い状態。 */
export function createUpdateGate(): UpdateGate {
  return { holds: new Set(), pending: [] };
}

/** 保留の理由が 1 つでも残っているか。 */
export function isHeld(gate: UpdateGate): boolean {
  return gate.holds.size > 0;
}

/** 保留の理由を足す。同じ理由を重ねても 1 つとして扱う。 */
export function holdUpdates(gate: UpdateGate, reason: HoldReason): UpdateGate {
  return gate.holds.has(reason) ? gate : { ...gate, holds: new Set(gate.holds).add(reason) };
}

/** 保留の理由を外す。最後の理由が外れたら、積んだフォルダを返して空にする。 */
export function releaseUpdates(gate: UpdateGate, reason: HoldReason): GateResult {
  if (!gate.holds.has(reason)) {
    return { gate, flush: [] };
  }
  const holds = new Set(gate.holds);
  holds.delete(reason);
  if (holds.size > 0) {
    return { gate: { ...gate, holds }, flush: [] };
  }
  return { gate: { holds, pending: [] }, flush: gate.pending };
}

/** 読み直すフォルダを渡す。保留中なら積むだけ、保留が無ければ重なりを除いてそのまま返す。 */
export function offerUpdates(gate: UpdateGate, dirs: readonly string[]): GateResult {
  if (!isHeld(gate)) {
    return { gate, flush: [...new Set(dirs)] };
  }
  return { gate: { ...gate, pending: [...new Set([...gate.pending, ...dirs])] }, flush: [] };
}
