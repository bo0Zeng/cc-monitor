/**
 * cc-bus 那两条钩子在一台机器上装了没有（只读）：那台后端读它自己的 agent 配置里的 `settings.json`、出整份成品
 * （帧命令 `hooks-diag`，本机远端同一条 `callTyped(origin, …)`）。界面照态画；不读、不写那份文件。
 *
 * 成品的形状从后端类型生成（`generated/HooksReport.ts`），线上形状由握手指纹核过、由跨语言金样 `tests/__fixtures__/hooks-diag.golden.json` 钉住。
 * `HookState` 是后端 `#[serde(tag = "kind", rename_all = "kebab-case")]` 的内部标记枚举 → 判别联合。
 */
import type { Origin } from "./ipc/origin";
import { budgetWithin, callTyped } from "./ipc/chan-caller";
import { copyText } from "./copy-table";
import type { HookState } from "./generated/HookState";
import type { HooksDiagnosis } from "./generated/HooksDiagnosis";
import type { HooksReport } from "./generated/HooksReport";
import { saidOf } from "./kit/detail";

export type { HookState, HooksDiagnosis, HooksReport };

/** 读一份 `settings.json` ＋ 几次 stat ＋ 回程。 */
const HOOKS_DIAG_BUDGET_MS = 30_000;

/** 问 `origin` 那台后端要一份成品（本机逐字 `LOCAL_ORIGIN`）。问不出来 ⇒ 抛（带那台的原话）。 */
export async function fetchHooksReport(origin: Origin): Promise<HooksReport> {
  try {
    return await callTyped(origin, "hooks-diag", {}, budgetWithin(HOOKS_DIAG_BUDGET_MS));
  } catch (e) {
    throw new Error(saidOf(e));
  }
}

/** 一态 → 那句话 ＋ 语气。**`path-missing` 绝不能说成「已装」**；`unknown` 要中性（既不说已装也不说未装）。 */
export function describeState(st: HookState): { text: string; tone: "ok" | "bad" | "unknown" } {
  switch (st.kind) {
    case "not-installed":
      return { text: copyText("ccBusHooks.state.missing"), tone: "bad" };
    case "installed-via-path":
      return { text: copyText("ccBusHooks.state.onPath"), tone: "ok" };
    case "installed-at-path":
      return { text: copyText("ccBusHooks.state.explicit", { path: st.path }), tone: "ok" };
    case "path-missing":
      return { text: copyText("ccBusHooks.state.brokenPath", { path: st.path }), tone: "bad" };
    case "unknown":
      // 命令里出现了目标程序，但它包在 `sh -c` / `env` / `timeout` 之类里：猜「未装」和猜「已装」一样是猜。
      return { text: copyText("ccBusHooks.state.indirect", { command: st.command }), tone: "unknown" };
  }
}
