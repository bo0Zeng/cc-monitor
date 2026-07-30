/**
 * F88a（#52）用量 pivot 的纯逻辑——把后端 `aggregate_usage_all` 流回的 per-(会话,模型,天) token 桶
 * 按 会话/天/项目/模型 四维聚合。纯逻辑模块（只 import 同为纯模块的 `pricing`，node 轨可断言；视图才依赖 Tauri/DOM）。
 *
 * **只 token 不 $**（用户 2026-07-17 拍板）。硬边界：这是「已花费」，非「配额剩余」——UI 标死。
 */

import { normalizeModel, equivalentInputTokens } from "./pricing";

/** 与后端 `usage.rs::UsageTotals` wire 对齐（camelCase）。 */
// C03：改成从生成物 re-export（源：`usage.rs::UsageTotals`）。
// 四个 token 字段在 Rust 侧是 `u64`（头注「u64 防大历史累加溢出」是刻意选择），
// 由 `#[ts(type = "number")]` 显式收窄——上限按 token 量算：2^53-1 ≈ 9×10^15，
// 按每天 10^7 tokens 算是 9 亿天。**这不是套用字节数那条「8 PB」，是单独算的。**
import type { UsageTotals } from "../generated/UsageTotals";
export type { UsageTotals };
// C04d 批 3：这两个也换成生成物（源 `usage.rs`）。**账本第 2 行的状态订正**：
// C03 只把 `UsageTotals` 换掉了，`UsageBucket`/`SessionUsageRow` 一直还是手写
// ——那一行标「已完成」是高估了，本批次才真正达成。
//
// **本批次抓到的漂移**：手写版写 `origin?: string`，而 Rust 是 `#[serde(default)] Option<String>`
// 且**没有** `skip_serializing_if` ⇒ 线上恒有该键、值可能是 `null`（不是省略）。
// 生成物给的 `origin: string | null` 才是实情。
import type { SessionUsageRow } from "../generated/SessionUsageRow";
import type { UsageBucket } from "../generated/UsageBucket";

export type { SessionUsageRow, UsageBucket };

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

/** #67:可排序的列。`key` = 维度键（「按天」时是 ISO `yyyy-mm-dd`,字典序即日历序）;其余为数值列。 */
export type SortCol =
  | "key"
  | "input"
  | "cacheCreation"
  | "cacheRead"
  | "output"
  | "total"
  | "equiv"
  | "msgs";
export type SortDir = "asc" | "desc";

function sortValue(r: PivotRow, col: SortCol): number | string {
  switch (col) {
    case "key":
      // #67 审计:排的必须是**用户看到的那一列**(`label`)。`key` 在 project 维是全路径、session 维是
      // uuid,按它排会得到「名字列看着没排」——正是 #67 报的同类症状。day/model 维 label≡key(空 day
      // 的 label 是 `(无日期)`,`(`<数字,与原空串行为同侧),日历序不受影响。
      return r.label;
    case "input":
      return r.totals.input;
    case "cacheCreation":
      return r.totals.cacheCreation;
    case "cacheRead":
      return r.totals.cacheRead;
    case "output":
      return r.totals.output;
    case "total":
      return totalTokens(r.totals);
    case "equiv":
      return equivalentInputTokens(r.totals);
    case "msgs":
      return r.totals.msgs;
  }
}

/**
 * #67:按指定列 / 方向排序 pivot 行（纯函数,不改入参）。平手回退「等效∑降序」保持稳定可预期次序
 * （= 本视图原默认序）。`key` 列走字符串比较——「按天」的 key 是 ISO `yyyy-mm-dd`,字典序即日历序。
 */
export function sortPivotRows(rows: PivotRow[], col: SortCol, dir: SortDir): PivotRow[] {
  const sign = dir === "asc" ? 1 : -1;
  return [...rows].sort((a, b) => {
    const va = sortValue(a, col);
    const vb = sortValue(b, col);
    const cmp =
      typeof va === "string" && typeof vb === "string"
        ? // #67 审计:用 localeCompare 而非 `<`/`>` 的 UTF-16 码元序——否则大写全排小写前、中文垫底
          // (`Apple,Docs,Zebra,apps,banana,文档`),用户肉眼判为「没排」。ISO 日期串走 localeCompare 仍是日历序。
          va.localeCompare(vb)
        : (va as number) - (vb as number);
    const primary = cmp * sign;
    // NaN 防御:比较值异常时不返回 NaN(次序未定义),落到平手回退。
    if (Number.isFinite(primary) && primary !== 0) return primary;
    // 平手 → 回退等效∑,**同样跟随 sign**:否则主键全平手时(如 `回复` 全=1、Codex 的 `cache 写` 全=0)
    // asc 与 desc 渲染完全相同——箭头翻了行却一动不动,用户读作「排序坏了」(#67 审计 建议)。
    // 写成「升序形态 × sign」与主键同构:desc(sign=-1) → 等效∑大的在上;asc → 小的在上。
    return sign * (equivalentInputTokens(a.totals) - equivalentInputTokens(b.totals));
  });
}

/**
 * #67:各维度的**默认排序**。「按天」默认按**日期降序**(最近在最上)——修掉「选了按天、却仍按等效∑排、
 * 日期乱跳」;其余维度沿用「等效∑降序」(哪儿最烧在最上,F88d-fix 的立意)。
 */
export function defaultSortForDim(dim: UsageDim): { col: SortCol; dir: SortDir } {
  return dim === "day" ? { col: "key", dir: "desc" } : { col: "equiv", dir: "desc" };
}

/** 全部会话的总用量（视图页脚合计 / HUD 今日过滤后复用）。 */
export function sumAll(rows: SessionUsageRow[]): UsageTotals {
  const t = emptyTotals();
  for (const row of rows) for (const b of row.buckets) addInto(t, b.totals);
  return t;
}
