// テストのファイルは製品に入らないので、画面の API の検査から外す。
// (この見本は node --test や vitest が拾わない拡張子 .tsx にしてある)
export function snapshot(el: { innerHTML: string }): string {
  return el.innerHTML;
}
