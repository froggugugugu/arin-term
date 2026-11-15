// ツリーで扱う、Rust から届く値の最小の型(計画 T41a)。
//
// `src/logic` は他の `src/*` を import できないため、生成した型(`src/ipc/generated/`)は使わず、
// ここで必要な値だけを定める。形は `arin-core` の `EntryKind`・`DirEntry`・`TreeChange`(計画 T38a)の
// JSON と同じにしてあり、生成物の値をそのまま渡せる(`TreeChange` の `projectId` はここでは使わない)。

/** 項目の種類。フォルダへのリンクは展開しない(設計書 §15.2 #8)。 */
export type TreeEntryKind = "file" | "directory" | "directoryLink" | "fileLink" | "other";

/** フォルダの中の 1 項目。並び順は Rust(`arin-core/tree`。計画 T38c)が決めたまま使う。 */
export interface TreeEntry {
  readonly name: string;
  readonly kind: TreeEntryKind;
}

/**
 * ツリーの変更(`tree-changed` の中身のうち、画面の計算に要る部分)。
 * `dirs` はルート基準の相対パス(ルートは空の文字列)。`fullReload` は「全部読み直し」の印(【仮定】19)。
 */
export interface TreeChange {
  readonly dirs: readonly string[];
  readonly fullReload: boolean;
}
