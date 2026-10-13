/**
 * **历史页的平铺会话清单**，经通道问本机常驻后端（帧面 `history-list`，后端 `history/history_list.rs`）。
 *
 * 一台一问（`origin` 缺席 = 本机）：哪台先答先画，一台没答不挡别的台。
 * 远端：开页 / 「刷新」（`fresh`）先经已开着的长连接问那台常驻 `raw`（那台热缓存），再把那份原样（`listing`）交本机后端并注解 · 筛 · 排；
 * 敲字只问本机后端（它记着那份），它说 `no_listing`（没记着：本机后端重启过 / 头一次）⇒ 同开页那一套。照全文搜索的做法：扇出在这里、判定在后端。
 * 判定都在后端：显示标题 · 搜什么 · 怎么排（`at`）· 每行能做什么（`can`）· 分组。这里只做三件事：
 * 问谁（含远端那一份的装运）、按类型收（入参 · 回包从后端类型生成，形状由握手指纹核；跨语言金样 `tests/__fixtures__/history-list.golden.json`）、
 * 把各台各自排好的行按 `at` 并成一列。
 */
import { ChanError } from "../../comms/inward/chan";
import { budgetWithin, callTyped, refusalOf, saidFrom } from "./ipc/chan-caller";
import { LOCAL_ORIGIN } from "./ipc/origin";
import { HistoryShapeError } from "./history-reads";
import type { HistoryList } from "./generated/HistoryList";
import type { HistoryRow } from "./generated/HistoryRow";
import type { HistoryGroup } from "./generated/HistoryGroup";
import type { HistoryListing } from "./generated/HistoryListing";
import type { HistoryListReply } from "./generated/HistoryListReply";
import type { HistoryListReq } from "./generated/HistoryListReq";

/** 一行会话 · 按项目看时的一组 · 一台的成品（类型从后端生成）。 */
export type { HistoryList, HistoryRow, HistoryGroup };

/** 问的那几格（缺 = 后端缺省）。 */
export interface HistoryListAsk {
  /** 只要这一个会话那一行（独立查看窗；别的筛后端不看）。 */
  sid?: string;
  query?: string;
  sort?: "activity" | "created";
  withinDays?: number;
  hidden?: boolean;
  /** 远端那一台：不用本机后端记着的那份，再问一次那台（开页 · 「刷新」）。不下发给后端。 */
  fresh?: boolean;
}

/** 一问 30 秒（远端那台常驻热着那份清单；盖住一整份经长连接回来、本机后端并一遍）。 */
const LIST_BUDGET_MS = 30_000;

/** 回包两形里平时那一形（成品带 `groups`）；问的不是 `raw` 却回了清单 ⇒ 抛（细目只进日志）。 */
function listOf(r: HistoryListReply): HistoryList {
  if ("groups" in r) return r;
  throw new HistoryShapeError("history-list answered a raw listing to a non-raw ask");
}

/** 回包两形里 `raw` 那一形（这台自己的清单）；回了成品 ⇒ 抛。 */
function listingOf(r: HistoryListReply): HistoryListing {
  if (!("groups" in r)) return r;
  throw new HistoryShapeError("history-list answered a product to a raw ask");
}

/** 一台的清单（`origin` 缺席 = 本机）。失败 ⇒ 抛（给人看的那句：`history-reads.ts::historyReasonOf`）。 */
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
  if (!origin) return askLocal(args);
  if (!ask.fresh) {
    try {
      return await askLocal(args);
    } catch (e) {
      if (!notHeld(e)) throw e;
    }
  }
  // 那台常驻答的 `raw` 原样交过去（并 · 筛 · 排在本机后端）。
  const listing = await askRemoteRaw(origin);
  return askLocal({ origin, listing, ...args });
}

/** 本机后端那一问（并 · 筛 · 排都在它那里；形状由握手指纹核）。 */
async function askLocal(args: HistoryListReq): Promise<HistoryList> {
  return listOf(await callTyped(LOCAL_ORIGIN, "history-list", args, budgetWithin(LIST_BUDGET_MS)));
}

/** 经已开着的长连接问那台常驻它自己的清单。失败 ⇒ 抛那台那一句（谁老了 / 谁够不到说的是那台）。 */
async function askRemoteRaw(origin: string): Promise<HistoryListing> {
  let reply: HistoryListReply;
  try {
    reply = await callTyped(origin, "history-list", { raw: true }, budgetWithin(LIST_BUDGET_MS));
  } catch (e) {
    throw new Error(saidFrom(e, origin));
  }
  return listingOf(reply);
}

/** 本机后端说「那台的清单还没取回」。 */
function notHeld(e: unknown): boolean {
  return e instanceof ChanError && e.error.layer === "peer" && e.error.why === "refused" && refusalOf(e.error.body)?.code === "no_listing";
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
