/**
 * cc-bus 收信钩子在一台机器上各家装了没有、信任对没对上（只读）：那台后端读各家自己的钩子文件（声明了信任的再读信任那份）、出整份成品
 * （帧命令 `hooks-diag`，本机远端同一条 `callTyped(origin, …)`）：各态那一句与色调由后端写（`diagnosis.said` · `tone`），界面照抄；不读、不写那几份文件。
 *
 * 成品的形状从后端类型生成（`generated/HooksReport.ts`），线上形状由握手指纹核过、由跨语言金样 `tests/__fixtures__/hooks-diag.golden.json` 钉住。
 * `HookState` 是后端 `#[serde(tag = "kind", rename_all = "kebab-case")]` 的内部标记枚举 → 判别联合。
 */
import type { Machine } from "./ipc/origin";
import { budgetWithin, callTyped } from "./ipc/chan-caller";
import type { HookState } from "./generated/HookState";
import type { FamilyHooks } from "./generated/FamilyHooks";
import type { HooksDiagnosis } from "./generated/HooksDiagnosis";
import type { HooksReport } from "./generated/HooksReport";

export type { FamilyHooks, HookState, HooksDiagnosis, HooksReport };

/** 每家读一份钩子文件（声明了信任的再读信任那份）＋ 几次 stat ＋ 回程。 */
const HOOKS_DIAG_BUDGET_MS = 30_000;

/** 问 `origin` 那台后端要一份成品（本机逐字 `LOCAL`）。问不出来 ⇒ 抛（带那台的原话）。 */
export async function fetchHooksReport(origin: Machine): Promise<HooksReport> {
  return await callTyped(origin, "hooks-diag", {}, budgetWithin(HOOKS_DIAG_BUDGET_MS));
}
