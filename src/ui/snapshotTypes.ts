// 部品が使う写しの型。src/ui は src/ipc を import しない(境界の規則)ので、ストアの型から取り出す。

import type { AppState } from "../state/appStore";

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
