/**
 * **会话清单**，经通道问本机常驻后端。两问：
 *
 * - `history-browse`（后端 `history/history_browse.rs`）：历史页那一列 —— 几台在本机后端并成一列（行按 `at`、组按 `order`、分叉挂父会话、按项目开合），
 *   这里只问看得见的那一窗、照项画；各台读没读到在头里。远端那几台由本机后端读它记着的副本，带 `fresh` 的那几台先同步一次（开页 · 刷新）。
 * - `history-list`（后端 `history/history_list.rs`）：一台上按会话 ID 要一行（独立查看窗 · agent 窗）。
 *
 * 判定都在后端：显示标题 · 搜什么 · 怎么排 · 怎么并 · 每行能做什么（`can`）· 分组。入参 · 回包从后端类型生成，形状由握手指纹核。
 * 回包是带键集合那一形（`common/views.rs`）：这里不带记号问，答的就是那一窗的整份（`whole`），照 `order` 从 `up` 取项；不写合并。
 */
import { budgetWithin, callTyped, ReplyUnreadable } from "./ipc/chan-caller";
import { LOCAL_ORIGIN } from "./ipc/origin";
import type { HistoryRow } from "./generated/HistoryRow";
import type { HistoryGroup } from "./generated/HistoryGroup";
import type { HistoryHead } from "./generated/HistoryHead";
import type { HistoryListReq } from "./generated/HistoryListReq";
import type { HistoryBrowseReq } from "./generated/HistoryBrowseReq";
import type { HistoryBrowseHead } from "./generated/HistoryBrowseHead";
import type { HistoryLine } from "./generated/HistoryLine";
import type { HistoryMachine } from "./generated/HistoryMachine";
import type { SyncPatch } from "./generated/SyncPatch";

/** 一行会话 · 按项目看时的一组 · 历史页那一列的一项 · 一台那一格（类型从后端生成）。 */
export type { HistoryRow, HistoryGroup, HistoryLine, HistoryMachine, HistoryBrowseHead };

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
    ...(ask.sid ? { sid: ask.sid } : {}),
    ...(ask.query ? { query: ask.query } : {}),
    ...(ask.sort ? { sort: ask.sort } : {}),
    ...(ask.withinDays ? { within_days: ask.withinDays } : {}),
    ...(ask.hidden ? { hidden: true } : {}),
  };
  return listOf(await callTyped(LOCAL_ORIGIN, "history-list", args, budgetWithin(LIST_BUDGET_MS)));
}

/** 历史页那一列的一窗：第 `from` 项起的那几项 · 整列几项 · 头（各台那一格 · 段头 · 几个项目）。 */
export interface BrowseWindow {
  from: number;
  lines: HistoryLine[];
  total: number;
  head: HistoryBrowseHead;
}

/** 历史页那一列的一窗。失败 ⇒ 抛（给人看的那句 `kit/detail.ts::saidOf` 取）；一台读不到不算失败（在头里那一格）。 */
export async function fetchBrowse(ask: HistoryBrowseReq): Promise<BrowseWindow> {
  const r: SyncPatch<HistoryBrowseHead, HistoryLine> = await callTyped(LOCAL_ORIGIN, "history-browse", ask, budgetWithin(LIST_BUDGET_MS));
  if (!r.whole || r.order === null || r.head === null) throw new ReplyUnreadable(LOCAL_ORIGIN, "history-browse answered a patch to an ask without a mark");
  const lines = r.order.map((k) => r.up[k]);
  if (lines.some((x) => x === undefined)) throw new ReplyUnreadable(LOCAL_ORIGIN, "history-browse order names a line it did not send");
  return { from: ask.range.from, lines, total: r.head.total, head: r.head.info };
}
