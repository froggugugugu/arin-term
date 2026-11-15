// ショートカットの表の型と、キーの文字列の読み方・比べ方・表示(計画 T41b。前提 (e)【要件の追加(本人。2026-11-15)】)。
// DOM を使わない純粋な関数。
//
// 表の正本は Rust の `arin-core/shortcuts`(計画 T38d)。画面は表を引数で受け取り、キーの文字列をコードに直書きしない
// (M4.5 で設定ファイルからキーを変えるため)。src/logic は生成した型を import しないので、表の形はここで定める
// (T38d の ts-rs の型と同じ形にする)。
//
// キーの文字列は Tauri(muda)のアクセラレータの書式(`Cmd+Shift+N`)。修飾キーの順は問わない。

/** コマンドの ID。文字列は M2 の `menu.rs` の ID と、M3 で足すもの(計画 T38d)。 */
export type ShortcutCommandId =
  | "terminal.new-tab"
  | "terminal.close-tab"
  | "terminal.previous-tab"
  | "terminal.next-tab"
  | "edit.select-all"
  | "app.quit"
  | "edit.copy"
  | "edit.cut"
  | "edit.paste"
  | "tree.open"
  | "tree.reveal-in-finder"
  | "tree.insert-path"
  | "tree.copy-relative-path"
  | "tree.copy-absolute-path"
  | "tree.rename"
  | "tree.trash"
  | "tree.new-file"
  | "tree.new-folder";

/**
 * キーの受け口(計画【仮定】21)。
 * - `menu-item`: メニューバーの項目のアクセラレータで OS が受ける
 * - `edit-item`: 「編集」の標準の項目が画面のコピー・切り取り・ペーストのイベントにする
 * - `screen`: ツリーにフォーカスがあるときに画面の keydown で受ける
 */
export type ShortcutReceiver = "menu-item" | "edit-item" | "screen";

/** 表の 1 行。 */
export interface ShortcutBinding {
  readonly command: ShortcutCommandId;
  /** キーの文字列(`Cmd+Shift+N` の形)。 */
  readonly accelerator: string;
  readonly receiver: ShortcutReceiver;
}

/** ショートカットの表。コマンドの ID は 1 行に 1 つ。 */
export type ShortcutTable = readonly ShortcutBinding[];

/** 読んだキーの文字列。`key` は文字のキーなら大文字 1 文字、記号なら 1 文字、ほかは名前(`Enter` など)。 */
export interface ParsedAccelerator {
  readonly meta: boolean;
  readonly shift: boolean;
  readonly alt: boolean;
  readonly ctrl: boolean;
  readonly key: string;
}

/** 修飾キーの名前(小文字)→ 修飾キー。muda が読む別名にそろえる。 */
const MODIFIERS: Readonly<Record<string, keyof Omit<ParsedAccelerator, "key">>> = {
  cmd: "meta",
  command: "meta",
  super: "meta",
  shift: "shift",
  alt: "alt",
  option: "alt",
  ctrl: "ctrl",
  control: "ctrl",
};

/**
 * 名前のキー(小文字 → KeyboardEvent の key の名前)。Rust の `KeyChord::parse`(`arin-core/shortcuts/key.rs`)が読む名前だけ
 * (`Return` は muda が読めないので読まない)。
 */
const NAMED_KEYS: Readonly<Record<string, string>> = {
  enter: "Enter",
  backspace: "Backspace",
  delete: "Delete",
  escape: "Escape",
  esc: "Escape",
  tab: "Tab",
  space: " ",
  arrowup: "ArrowUp",
  arrowdown: "ArrowDown",
  arrowleft: "ArrowLeft",
  arrowright: "ArrowRight",
};

/** 記号のキー → 物理的な位置(KeyboardEvent の code)と、US 配列で ⇧ を押したときの文字。 */
const SYMBOL_KEYS: Readonly<Record<string, { readonly code: string; readonly shifted: string }>> = {
  "[": { code: "BracketLeft", shifted: "{" },
  "]": { code: "BracketRight", shifted: "}" },
  "-": { code: "Minus", shifted: "_" },
  "=": { code: "Equal", shifted: "+" },
  ",": { code: "Comma", shifted: "<" },
  ".": { code: "Period", shifted: ">" },
  "/": { code: "Slash", shifted: "?" },
  ";": { code: "Semicolon", shifted: ":" },
  "'": { code: "Quote", shifted: '"' },
  "`": { code: "Backquote", shifted: "~" },
  "\\": { code: "Backslash", shifted: "|" },
};

/** 名前のキーの表示(macOS のメニューの記号)。 */
const NAMED_KEY_LABELS: Readonly<Record<string, string>> = {
  Enter: "↩",
  Backspace: "⌫",
  Delete: "⌦",
  Escape: "⎋",
  Tab: "⇥",
  " ": "Space",
  ArrowUp: "↑",
  ArrowDown: "↓",
  ArrowLeft: "←",
  ArrowRight: "→",
};

/**
 * キーの文字列を修飾キーとキーに分けて読む。受け付ける範囲は Rust の `KeyChord::parse` と同じ。
 * 読めなければ null(未知の修飾キー・キーが無い・2 つ以上・同じ修飾キーの重なり・キーの後ろの修飾キー)。
 */
export function parseAccelerator(accelerator: string): ParsedAccelerator | null {
  const parts = accelerator.split("+");
  // 「Cmd++」のような + のキーは表に無いので扱わない
  if (parts.some((part) => part === "")) return null;
  const mods = { meta: false, shift: false, alt: false, ctrl: false };
  let key: string | null = null;
  for (const part of parts) {
    const modifier = MODIFIERS[part.toLowerCase()];
    if (modifier !== undefined) {
      // 修飾キーを先に、キーを最後に書く(`A+Cmd` は読まない)
      if (key !== null || mods[modifier]) return null;
      mods[modifier] = true;
      continue;
    }
    if (key !== null) return null;
    key = normalizeKey(part);
    if (key === null) return null;
  }
  if (key === null) return null;
  return { ...mods, key };
}

function normalizeKey(part: string): string | null {
  if (/^[a-z0-9]$/i.test(part)) return part.toUpperCase();
  if (SYMBOL_KEYS[part] !== undefined) return part;
  return NAMED_KEYS[part.toLowerCase()] ?? null;
}

/** 押されたキーの物理的な位置(code)で比べるときの値。 */
function codeOf(key: string): string {
  if (/^[A-Z]$/.test(key)) return `Key${key}`;
  if (/^[0-9]$/.test(key)) return `Digit${key}`;
  return SYMBOL_KEYS[key]?.code ?? key;
}

/** 押されたキーの押し下げのうち、比べるのに使う部分(KeyboardEvent はこの形を満たす)。 */
export interface PressedKey {
  readonly key: string;
  readonly code?: string;
  readonly metaKey: boolean;
  readonly ctrlKey: boolean;
  readonly altKey: boolean;
  readonly shiftKey: boolean;
}

/**
 * 押されたキーが、読んだキーの文字列と一致するか。修飾キーは過不足なく一致させる。
 * - ⌥ を含むときは物理的な位置(`code`)で比べる(⌥ で `key` が別の文字になるため。計画【仮定】22)
 * - 文字のキーは大文字・小文字を区別しない(Caps Lock)
 * - ⇧ を含む記号のキーは、⇧ 付きの文字でも一致する(配列によって `[` のままのことも `{` になることもある)
 */
export function matchesAccelerator(parsed: ParsedAccelerator, pressed: PressedKey): boolean {
  if (
    parsed.meta !== pressed.metaKey ||
    parsed.ctrl !== pressed.ctrlKey ||
    parsed.alt !== pressed.altKey ||
    parsed.shift !== pressed.shiftKey
  ) {
    return false;
  }
  if (parsed.alt) return pressed.code === codeOf(parsed.key);
  if (parsed.key.length === 1) {
    const key = pressed.key.length === 1 ? pressed.key.toUpperCase() : pressed.key;
    if (key === parsed.key) return true;
    return parsed.shift && SYMBOL_KEYS[parsed.key]?.shifted === pressed.key;
  }
  return pressed.key === parsed.key;
}

/** メニューに出すキーの表示(macOS の順 ⌃⌥⇧⌘ の記号 + キー)。読めない文字列は null。 */
export function formatAccelerator(accelerator: string): string | null {
  const parsed = parseAccelerator(accelerator);
  if (parsed === null) return null;
  const mods = `${parsed.ctrl ? "⌃" : ""}${parsed.alt ? "⌥" : ""}${parsed.shift ? "⇧" : ""}${parsed.meta ? "⌘" : ""}`;
  return `${mods}${NAMED_KEY_LABELS[parsed.key] ?? parsed.key}`;
}

/** 表からコマンドの ID のキーの文字列を引く。無ければ null。 */
export function acceleratorOf(table: ShortcutTable, command: ShortcutCommandId): string | null {
  return table.find((binding) => binding.command === command)?.accelerator ?? null;
}

/** 表からコマンドの ID のキーの表示を引く。無い・読めなければ null。 */
export function shortcutLabel(table: ShortcutTable, command: ShortcutCommandId): string | null {
  const accelerator = acceleratorOf(table, command);
  return accelerator === null ? null : formatAccelerator(accelerator);
}

/** 受け口が `receivers` のどれかの行のうち、押されたキーと一致する最初の行。読めない行は一致しない。 */
export function findBinding(
  table: ShortcutTable,
  pressed: PressedKey,
  receivers: readonly ShortcutReceiver[],
): ShortcutBinding | null {
  for (const binding of table) {
    if (!receivers.includes(binding.receiver)) continue;
    const parsed = parseAccelerator(binding.accelerator);
    if (parsed !== null && matchesAccelerator(parsed, pressed)) return binding;
  }
  return null;
}
