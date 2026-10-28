/**
 * **往一条会话说一句 ＝ 投进那条会话的输入队**（`session-queue-put`）：这一端先铸一个编号（就是队去重用的 id），
 * 交出去、会话那台落了盘才回执；没回执（断线 · 期限到了 · 那台离线）⇒ 那一句留在待发，那台连上照同一个编号再交，那台按编号只收一条。
 * 那台拒了（队满 · 太长 · 附件已不在）⇒ 回那台写好的那一句，字留着、可改了再送。
 * 待发落本地存储（按那台 ＋ 会话一句），重开还在、接上照同一个编号再交。
 *
 * **队的名单只照会话那台那一份画**：那台推 `changed {session_queue, key: sid}` 带整份名单（`session-queue-read` 的回包），
 * 没带 ⇒ 重问；撤（`session-queue-cancel`）回撤完的整份。这一端不各自记队。
 */
import { LOCAL_ORIGIN } from "./backend-policy";
import { pushedProducts } from "./changed-stream";
import { subscribeTopics } from "./events";
import { budgetWithin, callTyped, refusalIn } from "./ipc/chan-caller";
import type { Origin } from "./ipc/origin";
import type { QueueCancelled } from "./generated/QueueCancelled";
import type { QueuePut } from "./generated/QueuePut";
import type { QueueList } from "./generated/QueueList";
import type { SessionQueuePutReq } from "./generated/SessionQueuePutReq";
import { saidOf } from "./kit/detail";
import { LS_KEYS, safeGetJson, safeSetJson } from "./local-storage";

/** 读一条会话此刻的名单（`session-queue-read`）。 */
export type QueueReadFn = (origin: Origin, sid: string) => Promise<QueueList>;
/** 撤一条（`session-queue-cancel`），回撤完的整份名单。 */
export type QueueCancelFn = (origin: Origin, sid: string, id: string) => Promise<QueueCancelled>;

/** 那台推来的一批 `changed {session_queue}`：带了名单的 ⇒ `got`；没带的 ⇒ `ask`（重问）；`all` ⇒ 期间可能漏了，看着的那几条都重问。 */
export interface QueueMoved {
  got: Map<string, QueueList>;
  ask: string[];
  all: boolean;
}

/** 订那台的 `changed {session_queue}`；回的函数退订。 */
export type QueueWatchFn = (origin: Origin, on: (m: QueueMoved) => void) => () => void;

/** 读名单 · 撤的期限。 */
const QUEUE_BUDGET_MS = 10_000;

export const CHANNEL_READ: QueueReadFn = (origin, sid) => callTyped(origin, "session-queue-read", { machine: LOCAL_ORIGIN, sid }, budgetWithin(QUEUE_BUDGET_MS));

export const CHANNEL_CANCEL: QueueCancelFn = (origin, sid, id) =>
  callTyped(origin, "session-queue-cancel", { machine: LOCAL_ORIGIN, sid, id }, budgetWithin(QUEUE_BUDGET_MS));

export const CHANNEL_WATCH: QueueWatchFn = (origin, on) => {
  let stop: (() => void) | null = null;
  let gone = false;
  void subscribeTopics([origin], ["session_queue"], (_o, changes) => {
    const c = changes.get("session_queue");
    if (c === undefined) return;
    const { got, ask, all } = pushedProducts(c, "session_queue");
    const lists = new Map<string, QueueList>();
    for (const [k, v] of got) if (k !== null) lists.set(k, v);
    on({ got: lists, ask: ask.filter((k): k is string => k !== null), all });
  }).then(
    (s) => {
      if (gone) s();
      else stop = s;
    },
    (e: unknown) => console.warn("[session-queue] 订不上那台的输入队：", e),
  );
  return () => {
    gone = true;
    stop?.();
  };
};

/** 待发在本地存储里那一格的键：那台 ＋ 会话。 */
export const pendingKey = (origin: Origin, sid: string): string => `${origin}\u0000${sid}`;

const isOutgoing = (o: unknown): o is Outgoing => {
  const v = o as Partial<Outgoing> | null;
  return typeof v?.id === "string" && typeof v.sid === "string" && typeof v.text === "string" && Array.isArray(v.attachments);
};

/** 本地存储里的待发（键 ⇒ 那几句，先交的在前）；读不懂的那几格 ⇒ 不要。 */
export function loadPending(): Map<string, Outgoing[]> {
  const v = safeGetJson<Record<string, unknown>>(LS_KEYS.queuePending);
  const out = new Map<string, Outgoing[]>();
  if (v === null || typeof v !== "object") return out;
  for (const [k, list] of Object.entries(v)) {
    const ok = Array.isArray(list) ? list.filter(isOutgoing) : [];
    if (ok.length > 0) out.set(k, ok);
  }
  return out;
}

/** 待发整份落本地存储。 */
export function savePending(m: ReadonlyMap<string, readonly Outgoing[]>): void {
  safeSetJson(LS_KEYS.queuePending, Object.fromEntries([...m].filter(([, l]) => l.length > 0)));
}

/** 投一句要的那一口（缺省走通道；测试换成假的）。 */
export type QueuePutFn = (origin: Origin, req: SessionQueuePutReq) => Promise<QueuePut>;

/** 投一句的期限。 */
const PUT_BUDGET_MS = 20_000;

export const CHANNEL_PUT: QueuePutFn = (origin, req) => callTyped(origin, "session-queue-put", req, budgetWithin(PUT_BUDGET_MS));

/** 这一端发自哪一端（入队那一条记作发自它；投递时当自报的前端过身份门）。 */
const END = "monitor";

/** 跨台那一跳说那台够不到：同断线，没回执。 */
const OFFLINE = "offline";

/** 待发里的一句：这一端铸的编号 · 那条会话 · 那一句 · 随它的附件编号。 */
export interface Outgoing {
  id: string;
  sid: string;
  text: string;
  attachments: string[];
}

/** 新铸一句待发（编号这一端唯一）。 */
export function outgoing(sid: string, text: string, attachments: string[]): Outgoing {
  return { id: `${END}-${crypto.randomUUID()}`, sid, text, attachments };
}

/** 交一次的结局：回执到了（那台写好的那一句）· 那台拒了（那台写好的那一句）· 没回执（留着，接上再交）。 */
export type PutEnd = { at: "receipt"; said: string } | { at: "refused"; said: string } | { at: "none"; said: string };

/** 交那一句（同一个编号交几次都只进一条）。 */
export async function putOnce(put: QueuePutFn, origin: Origin, o: Outgoing): Promise<PutEnd> {
  const req: SessionQueuePutReq = {
    // 问的就是交过去的那一台自己（那台眼里的本机）。
    machine: LOCAL_ORIGIN,
    sid: o.sid,
    id: o.id,
    end: END,
    atMs: Date.now(),
    text: o.text,
    ...(o.attachments.length > 0 ? { attachments: o.attachments } : {}),
  };
  try {
    const got = await put(origin, req);
    return { at: "receipt", said: got.said };
  } catch (e) {
    const no = refusalIn(e);
    return no !== null && no.code !== OFFLINE ? { at: "refused", said: no.message } : { at: "none", said: saidOf(e) };
  }
}
