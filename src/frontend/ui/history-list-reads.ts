/**
 * **历史页的平铺会话清单**，经通道问本机常驻后端（帧面 `history-list`，后端 `history/history_list.rs`）。
 *
 * 一台一问（`origin` 缺席 = 本机）：哪台先答先画，一台没答不挡别的台。
 * 远端那台的清单由本机后端自己问那台的常驻、记着副本（`history/remote_listing.rs`）：开页 / 「刷新」带 `fresh`（先同步一次），敲字只问本机后端。
 * 判定都在后端：显示标题 · 搜什么 · 怎么排（`at`）· 每行能做什么（`can`）· 分组。这里只做三件事：
 * 问谁、按类型收（入参 · 回包从后端类型生成，形状由握手指纹核；跨语言金样 `tests/__fixtures__/history-list.golden.json`）、
 * 把各台各自排好的行按 `at` 并成一列。
 * 回包是带键集合那一形（`common/views.rs`）：这里不带记号问，答的就是那一窗的整份（`whole`），照 `order` 从 `up` 取行；不写合并。
 */
import { budgetWithin, callTyped, ReplyUnreadable } from "./ipc/chan-caller";
import { LOCAL_ORIGIN } from "./ipc/origin";
import type { HistoryRow } from "./generated/HistoryRow";
import type { HistoryGroup } from "./generated/HistoryGroup";
import type { HistoryHead } from "./generated/HistoryHead";
import type { HistoryListReq } from "./generated/HistoryListReq";
import type { SyncPatch } from "./generated/SyncPatch";

/** 一行会话 · 按项目看时的一组（类型从后端生成）。 */
export type { HistoryRow, HistoryGroup };

/** 一台的成品：那一窗的行（照那台的先后）· 组 · 注解没并上那一句 · 整份几行 · 窗外还有没有。 */
export interface HistoryList {
  rows: HistoryRow[];
  groups: HistoryGroup[];
  notice: string | null;
  total: number;
  truncated: boolean;
}

/** 问的那几格（缺 = 后端缺省）。 */
export interface HistoryListAsk {
  /** 只要这一个会话那一行（独立查看窗；别的筛后端不看）。 */
  sid?: string;
  query?: string;
  sort?: "activity" | "created";
  withinDays?: number;
  hidden?: boolean;
  /** 远端那一台：本机后端先问那台同步一次副本再答（开页 · 「刷新」）。本机不带。 */
  fresh?: boolean;
}

/** 一问 30 秒（本机后端问远端那台时盖住一次同步：没变只回一个记号，头一次是整份）。 */
const LIST_BUDGET_MS = 30_000;

/** 回包（带键集合，不带记号问 ⇒ 整份）摊成一台的成品；不是整份 · 先后里点了没给的行 ⇒ 抛（细目只进日志）。 */
function listOf(r: SyncPatch<HistoryHead, HistoryRow>): HistoryList {
  if (!r.whole || r.order === null || r.head === null) throw new ReplyUnreadable(LOCAL_ORIGIN, "history-list answered a patch to an ask without a mark");
  const rows = r.order.map((k) => r.up[k]);
  if (rows.some((x) => x === undefined)) throw new ReplyUnreadable(LOCAL_ORIGIN, "history-list order names a row it did not send");
  return { rows, groups: r.head.info.groups, notice: r.head.info.notice, total: r.head.total, truncated: r.head.total > rows.length };
}

/** 一台的清单（`origin` 缺席 = 本机）。失败 ⇒ 抛（给人看的那句 `kit/detail.ts::saidOf` 取）。 */
export async function fetchList(origin: string | undefined, ask: HistoryListAsk): Promise<HistoryList> {
  // 只带给了的那几格（缺 = 后端缺省）。
  const args: HistoryListReq = {
    ...(origin ? { origin } : {}),
    ...(origin && ask.fresh ? { fresh: true } : {}),
    ...(ask.sid ? { sid: ask.sid } : {}),
    ...(ask.query ? { query: ask.query } : {}),
    ...(ask.sort ? { sort: ask.sort } : {}),
    ...(ask.withinDays ? { within_days: ask.withinDays } : {}),
    ...(ask.hidden ? { hidden: true } : {}),
  };
  return listOf(await callTyped(LOCAL_ORIGIN, "history-list", args, budgetWithin(LIST_BUDGET_MS)));
}

/** 各台各自按 `key` 倒序排好的一列 ⇒ 并成一列（同值保持台的先后，稳定）。只并、不另判。 */
function mergeBy<T>(lists: readonly (readonly T[])[], key: (x: T) => number): T[] {
  const out: T[] = [];
  const at = lists.map(() => 0);
  for (;;) {
    let best = -1;
    for (let i = 0; i < lists.length; i++) {
      const x = lists[i][at[i]];
      if (x !== undefined && (best < 0 || key(x) > key(lists[best][at[best]]))) best = i;
    }
    if (best < 0) return out;
    out.push(lists[best][at[best]++]);
  }
}

/** 各台的行按 `at` 并成一列。 */
export function mergeByAt(lists: readonly (readonly HistoryRow[])[]): HistoryRow[] {
  return mergeBy(lists, (r) => r.at);
}

/** 各台的组按 `order` 并成一列。 */
export function mergeGroups(lists: readonly (readonly HistoryGroup[])[]): HistoryGroup[] {
  return mergeBy(lists, (g) => g.order);
}
