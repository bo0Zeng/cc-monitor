/**
 * **历史页的平铺会话清单**，经通道问本机常驻后端（帧面 `history-list`，后端 `history/history_list.rs`）。
 *
 * 一台一问（`origin` 缺席 = 本机；远端由本机后端去问那台、并上本机的注解）：哪台先答先画，一台没答不挡别的台。
 * 判定都在后端：显示标题 · 搜什么 · 怎么排（`at`）· 每行能做什么（`can`）· 分组。这里只做三件事：
 * 问谁、按形状严格收（多一格 / 缺一格 / 类型不对 ⇒ 抛，跨语言金样 `tests/__fixtures__/history-list.golden.json`）、
 * 把各台各自排好的行按 `at` 并成一列。
 */
import { chan } from "../../comms/inward/chan";
import { budgetWithin, jsonBody, readJson } from "./ipc/chan-caller";
import { LOCAL_ORIGIN } from "./ipc/origin";
import { HistoryShapeError } from "./history-reads";

/** 这一行能做什么（后端按这一行判好，界面照画）。 */
export interface HistoryCan {
  /** `yes` 能恢复 · `switch` 在跑（切过去）· `bg` 分身会话（要恢复主会话）。 */
  resume: "yes" | "switch" | "bg";
  /** 恢复时能不能选号（那一家有没有账号这一维）。 */
  accounts: boolean;
  /** 能不能从某一轮分叉。 */
  fork: boolean;
  /** `yes` · `live` 在跑（先结束它）· `unsure` 说不清在不在跑。 */
  delete: "yes" | "live" | "unsure";
}

/** 一行会话（成品）。 */
export interface HistoryRow {
  agent: string;
  /** 行上那一家的小牌（如 `Codex`）；默认那一家 ⇒ `null`。 */
  agentTag: string | null;
  sessionId: string;
  projectDir: string;
  projectPath: string;
  projectName: string;
  group: string;
  aiTitle: string | null;
  firstUserExcerpt: string;
  /** 原标题（改标题时「留空 = 原标题」那一个）。 */
  title: string;
  /** 显示的标题。 */
  label: string;
  /** 没改过、没标题、没第一句。 */
  untitled: boolean;
  startedAt: number;
  updatedAt: number;
  /** 这一次排序用的那个时刻（毫秒）。 */
  at: number;
  jsonlPath: string;
  messageCountApprox: number;
  isBg: boolean;
  starred: boolean;
  customTitle: string | null;
  hidden: boolean;
  forkedFromSessionId?: string;
  forkedFromMessageUuid?: string;
  lastAccount?: string;
  status: "live" | "ended" | "unknown";
  can: HistoryCan;
  /** 只因为有在列的分叉从它分出来才带上（它自己被筛掉 / 隐藏了）。 */
  context?: true;
  origin?: string;
}

/** 按项目看时的一组。 */
export interface HistoryGroup {
  key: string;
  agent: string;
  projectName: string;
  projectPath: string;
  projectDir: string;
  count: number;
  /** `null` = 有判不了活的、又没有确定在跑的。 */
  hasLive: boolean | null;
  starred: boolean;
  lastActivity: number;
  /** 几台的组并成一列时的序（大的在前；后端算好，界面只按它并）。 */
  order: number;
  /** 读不了的那个记录目录 ⇒ 那一句；别的 ⇒ `null`。 */
  failed: string | null;
  origin?: string;
}

export interface HistoryList {
  rows: HistoryRow[];
  groups: HistoryGroup[];
  total: number;
  truncated: boolean;
  notice: string | null;
}

/** 问的那几格（缺 = 后端缺省）。 */
export interface HistoryListAsk {
  /** 只要这一个会话那一行（独立查看窗；别的筛后端不看）。 */
  sid?: string;
  query?: string;
  sort?: "activity" | "created";
  withinDays?: number;
  hidden?: boolean;
  fresh?: boolean;
}

/** 一台的清单 30 秒（远端那一跳要整份扫那台）。 */
const LIST_BUDGET_MS = 30_000;

const isObj = (v: unknown): v is Record<string, unknown> =>
  v !== null && typeof v === "object" && !Array.isArray(v);
const isStr = (v: unknown): v is string => typeof v === "string";
const isNum = (v: unknown): v is number => typeof v === "number" && Number.isFinite(v);
const isBool = (v: unknown): v is boolean => typeof v === "boolean";
const orNull =
  <T>(p: (v: unknown) => v is T) =>
  (v: unknown): v is T | null =>
    v === null || p(v);
const opt =
  <T>(p: (v: unknown) => v is T) =>
  (v: unknown): v is T | undefined =>
    v === undefined || p(v);
const oneOf =
  (...words: string[]) =>
  (v: unknown): boolean =>
    isStr(v) && words.includes(v);

function keysOk(o: Record<string, unknown>, must: readonly string[], may: readonly string[]): boolean {
  return must.every((k) => k in o) && Object.keys(o).every((k) => must.includes(k) || may.includes(k));
}

const ROW_KEYS = [
  "agent", "agentTag", "sessionId", "projectDir", "projectPath", "projectName", "group", "aiTitle", "firstUserExcerpt",
  "title", "label", "untitled", "startedAt", "updatedAt", "at", "jsonlPath", "messageCountApprox", "isBg",
  "starred", "customTitle", "hidden", "status", "can",
] as const;
const ROW_MAY = ["forkedFromSessionId", "forkedFromMessageUuid", "lastAccount", "context", "origin"] as const;
const GROUP_KEYS = [
  "key", "agent", "projectName", "projectPath", "projectDir", "count", "hasLive", "starred", "lastActivity", "order", "failed",
] as const;

/** `can` 那一格严格收（历史清单的行与全文搜索的命中同一张规则）。 */
export function canOk(c: unknown): c is HistoryCan {
  return (
    isObj(c) &&
    keysOk(c, ["resume", "accounts", "fork", "delete"], []) &&
    oneOf("yes", "switch", "bg")(c.resume) &&
    isBool(c.accounts) &&
    isBool(c.fork) &&
    oneOf("yes", "live", "unsure")(c.delete)
  );
}

function rowOk(r: unknown): r is HistoryRow {
  return (
    isObj(r) &&
    keysOk(r, ROW_KEYS, ROW_MAY) &&
    isStr(r.agent) &&
    orNull(isStr)(r.agentTag) &&
    isStr(r.sessionId) &&
    isStr(r.projectDir) &&
    isStr(r.projectPath) &&
    isStr(r.projectName) &&
    isStr(r.group) &&
    orNull(isStr)(r.aiTitle) &&
    isStr(r.firstUserExcerpt) &&
    isStr(r.title) &&
    isStr(r.label) &&
    isBool(r.untitled) &&
    isNum(r.startedAt) &&
    isNum(r.updatedAt) &&
    isNum(r.at) &&
    isStr(r.jsonlPath) &&
    isNum(r.messageCountApprox) &&
    isBool(r.isBg) &&
    isBool(r.starred) &&
    orNull(isStr)(r.customTitle) &&
    isBool(r.hidden) &&
    (r.forkedFromSessionId === undefined) === (r.forkedFromMessageUuid === undefined) &&
    opt(isStr)(r.forkedFromSessionId) &&
    opt(isStr)(r.forkedFromMessageUuid) &&
    opt(isStr)(r.lastAccount) &&
    oneOf("live", "ended", "unknown")(r.status) &&
    canOk(r.can) &&
    (r.context === undefined || r.context === true) &&
    opt(isStr)(r.origin)
  );
}

function groupOk(g: unknown): g is HistoryGroup {
  return (
    isObj(g) &&
    keysOk(g, GROUP_KEYS, ["origin"]) &&
    isStr(g.key) &&
    isStr(g.agent) &&
    isStr(g.projectName) &&
    isStr(g.projectPath) &&
    isStr(g.projectDir) &&
    isNum(g.count) &&
    orNull(isBool)(g.hasLive) &&
    isBool(g.starred) &&
    isNum(g.lastActivity) &&
    isNum(g.order) &&
    orNull(isStr)(g.failed) &&
    opt(isStr)(g.origin)
  );
}

/** `history-list` 的成品 ⇒ 清单。形状不对 ⇒ 抛 [`HistoryShapeError`]（细目只进日志）。 */
export function decodeList(v: unknown): HistoryList {
  if (
    !isObj(v) ||
    !keysOk(v, ["rows", "groups", "total", "truncated", "notice"], []) ||
    !Array.isArray(v.rows) ||
    !Array.isArray(v.groups) ||
    !isNum(v.total) ||
    !isBool(v.truncated) ||
    !orNull(isStr)(v.notice)
  ) {
    throw new HistoryShapeError("history-list reply is not exactly {rows, groups, total, truncated, notice}");
  }
  for (const r of v.rows) {
    if (!rowOk(r)) throw new HistoryShapeError(`history-list row has the wrong shape: ${JSON.stringify(r)}`);
  }
  for (const g of v.groups) {
    if (!groupOk(g)) throw new HistoryShapeError(`history-list group has the wrong shape: ${JSON.stringify(g)}`);
  }
  return v as unknown as HistoryList;
}

/** 一台的清单（`origin` 缺席 = 本机）。失败 ⇒ 抛（给人看的那句：`history-reads.ts::historyReasonOf`）。 */
export async function fetchList(origin: string | undefined, ask: HistoryListAsk): Promise<HistoryList> {
  // 只带给了的那几格（缺 = 后端缺省）。
  const args = {
    ...(origin ? { origin } : {}),
    ...(ask.sid ? { sid: ask.sid } : {}),
    ...(ask.query ? { query: ask.query } : {}),
    ...(ask.sort ? { sort: ask.sort } : {}),
    ...(ask.withinDays ? { within_days: ask.withinDays } : {}),
    ...(ask.hidden ? { hidden: true } : {}),
    ...(ask.fresh ? { fresh: true } : {}),
  };
  const body = jsonBody(args);
  const budget = budgetWithin(LIST_BUDGET_MS);
  const reply = await chan.call(LOCAL_ORIGIN, "history-list", body, budget);
  return decodeList(readJson(reply));
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
