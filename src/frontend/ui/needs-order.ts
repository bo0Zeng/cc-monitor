/**
 * 「需手动」那几个会话的先后（`Ctrl+J` · 「需手动」浮层）：问后端，界面不排。
 * 各台问 `sessions-needs`（那台排好的），按台原样交本机后端 `sessions-needs-merge` 合成一个先后（判定只住核心 `facts_query::needs_first`）。
 * 界面只把条上的对上去：后端没列到的（那台问不到 · 活动信号先到）排在后面、照条上的顺序。
 */
import { budgetWithin, callTyped } from "./ipc/chan-caller";
import { LOCAL_ORIGIN, type Origin } from "./ipc/origin";
import type { NeedsMachine } from "./generated/NeedsMachine";
import type { Tab } from "./tab-model";

/** 问一台的清单 · 本机合一份：都是读内存 ＋ 回程。 */
const ASK_BUDGET_MS = 10_000;

/** 问 `origins` 那几台，回后端排好的会话（先答的在前）；问不到的那台不在里面。一台都没问到 ⇒ 空。`call` 缺省走通道（测试换成假的）。 */
export async function askNeedsOrder(origins: readonly Origin[], call: typeof callTyped = callTyped): Promise<string[]> {
  const answered = await Promise.all(
    origins.map(async (origin): Promise<NeedsMachine | null> => {
      try {
        const { waiting } = await call(origin, "sessions-needs", {}, budgetWithin(ASK_BUDGET_MS));
        return { origin, waiting };
      } catch (e) {
        console.warn(`[needs] ${origin} 的需手动清单问不到：`, e);
        return null;
      }
    }),
  );
  const machines = answered.filter((m): m is NeedsMachine => m !== null);
  if (machines.length === 0) return [];
  const { order } = await call(LOCAL_ORIGIN, "sessions-needs-merge", { machines }, budgetWithin(ASK_BUDGET_MS));
  return order.map((x) => x.sid);
}

/** 条上在等的那几个（按条上的顺序给）照后端那一份排：列到的在前、照它的先后；没列到的跟在后面、照条上的顺序。 */
export function orderNeeds(needing: readonly Tab[], ranked: readonly string[]): string[] {
  const here = new Set(needing.map((t) => t.sessionId));
  const first = ranked.filter((sid) => here.has(sid));
  const listed = new Set(first);
  return [...first, ...needing.map((t) => t.sessionId).filter((sid) => !listed.has(sid))];
}
