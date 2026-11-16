// 部品が使う写しの型。src/ui は src/ipc を import しない(境界の規則)ので、ストアの型から取り出す。

import type { AppState } from "../state/appStore";
import type { QuitConfirmation } from "../state/uiStore";

/** 状態の写し。 */
export type Snapshot = NonNullable<AppState["snapshot"]>;
/** カード 1 枚ぶんのプロジェクト。 */
export type ProjectCard = Snapshot["projects"][number];
/** プロジェクトの識別子。 */
export type ProjectId = ProjectCard["id"];
/** プロジェクト色の識別子。 */
export type ProjectColor = ProjectCard["color"];
/** ペインの寸法(保存値)。 */
export type LayoutSizes = Snapshot["layout"];
/** タブ 1 枚ぶんのターミナル。 */
export type TerminalTab = Snapshot["terminals"][number];
/** ターミナルの識別子。 */
export type TerminalTabId = TerminalTab["id"];
/** 動作中のターミナル(削除・終了の確認に出す)。 */
export type RunningTerminal = QuitConfirmation["running"][number];
/** エディタのタブ 1 枚(M4.8)。 */
export type EditorTab = Snapshot["editors"][number];
/** エディタの識別子。 */
export type EditorTabId = EditorTab["id"];
/** 読み取り専用で開いた理由。 */
export type ReadOnlyReason = NonNullable<EditorTab["readOnly"]>;
/** 未保存のエディタ(閉じる・終了・削除の確認に出す)。 */
export type UnsavedEditor = QuitConfirmation["unsaved"][number];
