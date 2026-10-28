/**
 * **往一条会话说一句 ＝ 投进那条会话的输入队**（`session-queue-put`）：这一端先铸一个编号（就是队去重用的 id），
 * 交出去、会话那台落了盘才回执；没回执（断线 · 期限到了 · 那台离线）⇒ 那一句留在待发，那台连上照同一个编号再交，那台按编号只收一条。
 * 那台拒了（队满 · 太长 · 附件已不在）⇒ 回那台写好的那一句，字留着、可改了再送。
 */
import { LOCAL_ORIGIN } from "./backend-policy";
import { budgetWithin, callTyped, refusalIn } from "./ipc/chan-caller";
import type { Origin } from "./ipc/origin";
import type { QueuePut } from "./generated/QueuePut";
import type { SessionQueuePutReq } from "./generated/SessionQueuePutReq";
import { saidOf } from "./kit/detail";

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
