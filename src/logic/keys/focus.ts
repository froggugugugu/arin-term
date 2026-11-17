// キーとコピー・ペーストを振り分けるときのフォーカス位置(設計書 §9.1、PRD C-04)。

/**
 * フォーカス位置の種類。
 * - `tree`: ツリー(行やヘッダ。名前の入力欄を除く)
 * - `terminal`: ターミナル
 * - `name-input`: ツリーの名前の入力欄(新規作成・名称変更のインライン編集)
 * - `other`: それ以外
 */
export type FocusArea = "tree" | "terminal" | "name-input" | "other";
