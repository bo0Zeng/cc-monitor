/**
 * F88a（#52）用量 pivot 的纯逻辑——把后端 `aggregate_usage_all` 流回的 per-(会话,模型,天) token 桶
 * 按 会话/天/项目/模型 四维聚合。纯逻辑模块（只 import 同为纯模块的 `pricing`，node 轨可断言；视图才依赖 Tauri/DOM）。
 *
 * **只 token 不 $**（用户 2026-07-17 拍板）。硬边界：这是「已花费」，非「配额剩余」——UI 标死。
 */

import { normalizeModel, equivalentInputTokens } from "./pricing";

/** 与后端 `usage.rs::UsageTotals` wire 对齐（camelCase）。 */
export interface UsageTotals {
  input: number;
  cacheCreation: number;
  cacheRead: number;
  output: number;
  msgs: number;
}
export interface UsageBucket {
  model: string;
  day: string;
  totals: UsageTotals;
}
export interface SessionUsageRow {
  sessionId: string;
  projectPath: string;
  projectName: string;
  buckets: UsageBucket[];
  origin?: string; // 远端 host；本地 undefined
}

export type UsageDim = "session" | "day" | "project" | "model";
export interface PivotRow {
  key: string;
  label: string;
  totals: UsageTotals;
}

export function emptyTotals(): UsageTotals {
  return { input: 0, cacheCreation: 0, cacheRead: 0, output: 0, msgs: 0 };
}

function addInto(a: UsageTotals, b: UsageTotals): void {
  a.input += b.input;
  a.cacheCreation += b.cacheCreation;
  a.cacheRead += b.cacheRead;
  a.output += b.output;
  a.msgs += b.msgs;
}

/** 桶的四类 token 合计（排序/展示用）。 */
export function totalTokens(t: UsageTotals): number {
  return t.input + t.cacheCreation + t.cacheRead + t.output;
}

/** 把所有会话的桶按 `dim` 聚合，返回按总 token 降序的行。 */
export function pivotUsage(rows: SessionUsageRow[], dim: UsageDim): PivotRow[] {
  const map = new Map<string, PivotRow>();
  for (const row of rows) {
    for (const b of row.buckets) {
      let key: string;
      let label: string;
      switch (dim) {
        case "session":
          key = row.origin ? `${row.origin}\u0000${row.sessionId}` : row.sessionId;
          label =
            (row.origin ? `[${row.origin}] ` : "") +
            (row.projectName ? `${row.projectName} · ` : "") +
            row.sessionId.slice(0, 8);
          break;
        case "day":
          key = b.day;
          label = b.day || "(无日期)";
          break;
        case "project":
          key = row.origin ? `${row.origin}\u0000${row.projectPath}` : row.projectPath;
          label =
            (row.origin ? `[${row.origin}] ` : "") +
            (row.projectName || row.projectPath || "(未知项目)");
          break;
        case "model":
          // 业务二审 gap#6：归一化模型串（剥 [1m]/-fast/尾部日期快照），否则同一模型的不同日期快照碎成多行。
          key = normalizeModel(b.model);
          label = normalizeModel(b.model);
          break;
      }
      let pr = map.get(key);
      if (!pr) {
        pr = { key, label, totals: emptyTotals() };
        map.set(key, pr);
      }
      addInto(pr.totals, b.totals);
    }
  }
  // F88d-fix（batch18 审计修）：按**等效成本**降序排——「哪儿最烧」直接排在最上（原按 raw totalTokens
  // 排，会把狂读 cache（×0.1 便宜）的项目排最上、output 重（×5 贵）的排下面，与列的立意相悖）。
  // 同权重打平时回退 raw tokens 稳定次序。
  return [...map.values()].sort(
    (a, b) =>
      equivalentInputTokens(b.totals) - equivalentInputTokens(a.totals) ||
      totalTokens(b.totals) - totalTokens(a.totals),
  );
}

/** 全部会话的总用量（视图页脚合计 / HUD 今日过滤后复用）。 */
export function sumAll(rows: SessionUsageRow[]): UsageTotals {
  const t = emptyTotals();
  for (const row of rows) for (const b of row.buckets) addInto(t, b.totals);
  return t;
}
