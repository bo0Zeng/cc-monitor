/**
 * **历史注解与上次账号表，经通道问常驻后端**（注解的读写者是本机常驻后端，文件原地不动）。
 *
 * 清单那一族（平铺会话清单）住 `history-list-reads.ts`。这里只剩：改注解（星标 / 改标题 / 隐藏）· 删会话后连带删注解 ·
 * 「这台上每条会话上次用哪个号起」；外加两处共用的东西：形状不对的那一种错（[`HistoryShapeError`]）与给人看的那句话（[`historyReasonOf`]）。
 *
 * # 期限（`X6`：调用点显式给）
 *
 * 注解几问 10 秒（读写一份小文件）。
 */
import { chan } from "../../comms/inward/chan";
import { budgetWithin, jsonBody, readJson, ReplyUnreadable, saidFrom } from "./ipc/chan-caller";
import { LOCAL_ORIGIN, type Origin } from "./ipc/origin";
import { isObj } from "./ipc/decode";

// ─── 成品的形状 ───

/** 一条注解（`history-annotate` 回的那一条）。 */
export interface EntryMetadata {
  starred: boolean;
  customTitle: string | null;
  hidden: boolean;
  updatedAt: number;
}

const ANNOTATION_BUDGET_MS = 10_000;

// ─── 收货验形 ───

const isStr = (v: unknown): v is string => typeof v === "string";
const isNum = (v: unknown): v is number =>
  typeof v === "number" && Number.isFinite(v);
const isBool = (v: unknown): v is boolean => typeof v === "boolean";
const orNull =
  <T>(p: (v: unknown) => v is T) =>
  (v: unknown): v is T | null =>
    v === null || p(v);

/** 形状不对（本机后端回的东西认不出）。`message` 只是细目（英文，进日志）；给人看的那句由 [`historyReasonOf`] 按码取。 */
export class HistoryShapeError extends ReplyUnreadable {
  constructor(what: string) {
    super(what);
    this.name = "HistoryShapeError";
  }
}

/** 键集合恰好是 `must` ＋（可选）`may` 里的。 */
function keysOk(
  o: Record<string, unknown>,
  must: readonly string[],
  may: readonly string[],
): boolean {
  const keys = Object.keys(o);
  return (
    must.every((k) => k in o) &&
    keys.every((k) => must.includes(k) || may.includes(k))
  );
}

const ENTRY_KEYS = [
  "starred",
  "customTitle",
  "hidden",
  "updatedAt",
] as const;

/** `history-annotate` 的成品 ⇒ 那一条注解。 */
export function decodeEntry(v: unknown): EntryMetadata {
  const e = isObj(v) && keysOk(v, ["entry"], []) ? v.entry : undefined;
  const ok =
    isObj(e) &&
    keysOk(e, ENTRY_KEYS, []) &&
    isBool(e.starred) &&
    orNull(isStr)(e.customTitle) &&
    isBool(e.hidden) &&
    isNum(e.updatedAt);
  if (!ok)
    throw new HistoryShapeError(
      `history-annotate reply has the wrong shape: ${JSON.stringify(v)}`,
    );
  return e as unknown as EntryMetadata;
}

/** `history-last-accounts` 的成品 ⇒ sid → 账号名。 */
export function decodeLastAccounts(v: unknown): Record<string, string> {
  const m = isObj(v) && keysOk(v, ["accounts"], []) ? v.accounts : undefined;
  if (!isObj(m) || !Object.values(m).every(isStr)) {
    throw new HistoryShapeError(
      `history-last-accounts reply has the wrong shape: ${JSON.stringify(v)}`,
    );
  }
  return m as Record<string, string>;
}

/** 一次失败 ⇒ 给人看的那句话（`chan-caller.ts::saidFrom`；清单与搜索都是本机后端答的，远端那台的原因由它说成人话带回来）。 */
export function historyReasonOf(e: unknown): string {
  return saidFrom(e, LOCAL_ORIGIN);
}

// ─── 问 ───

/** 改一条注解（星标 / 改名 / 隐藏）。缺格或 `null` = 不改；标题给空白串 = 清空。注解跟着会话住在那台：问的是会话所在那台（`origin`）。 */
export async function annotate(
  origin: Origin,
  sessionId: string,
  patch: {
    starred?: boolean;
    customTitle?: string | null;
    hidden?: boolean;
  },
): Promise<EntryMetadata> {
  const body = jsonBody({ sid: sessionId, patch });
  const budget = budgetWithin(ANNOTATION_BUDGET_MS);
  const reply = await chan.call(origin, "history-annotate", body, budget);
  return decodeEntry(readJson(reply));
}

/** 删会话之后连带删那一条注解（会话所在那台的；删不掉只记一行：孤儿注解不害人）。 */
export async function forgetAnnotation(origin: Origin, sessionId: string): Promise<void> {
  try {
    const body = jsonBody({ sid: sessionId });
    const budget = budgetWithin(ANNOTATION_BUDGET_MS);
    await chan.call(origin, "history-forget", body, budget);
  } catch (e) {
    console.warn(`会话 ${sessionId} 删了，它那条注解没删掉（留着不害人）:`, e);
  }
}

/** `origin` 那台记着的 sid → 上次用哪个号起（只含真记过的那几条；会话跑在哪台就问哪台）。 */
export async function lastAccounts(origin: Origin): Promise<Record<string, string>> {
  const body = jsonBody({});
  const budget = budgetWithin(ANNOTATION_BUDGET_MS);
  const reply = await chan.call(
    origin,
    "history-last-accounts",
    body,
    budget,
  );
  return decodeLastAccounts(readJson(reply));
}
