/** 失敗の原因を、画面やログに出せる 1 行の文字列にする。Tauri のコマンドの失敗は文字列で届く。 */
export function describeError(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  try {
    const json = JSON.stringify(error);
    if (json !== undefined) return json;
  } catch {
    // BigInt や循環参照など、JSON にできないものは下で String にする。
  }
  return String(error);
}
