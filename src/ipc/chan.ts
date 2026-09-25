/**
 * 通道 · **webview 手里那一半** —— `设计/05 §3.3` 的 `call(origin, op, payload, budget)` 在主界面上的样子。
 *
 * # 🔴 通信层成员 `COMM-LAYER-MEMBER`〔C4a · 第四波 · 2026-09-24〕
 *
 * 这一枚标记是**盘上那一侧**的凭据（登记那一侧在 `tests/bridge/comm_boundary_registry_tests.rs::REGISTERED`，
 * 两向集合相等）。盖上它 = **上锁**：本文件从此被 `C1`–`C5` ＋ `X1`–`X6` 一起管着。
 *
 * **凭什么它属于通信层**：它与 `src/bridge/src/chan/client.rs`（进程外前端那一半）是同一件东西的两个住址 ——
 * 只认 `§2` 那四样里的三样：**地址**（`origin`）· **操作名**（不透明串）· **载荷**（不透明字节）。
 * 它不认识会话、账号、搜索；`op` 是什么、载荷长什么样，只有调用方与后端知道。
 *
 * # 形状
 *
 * ```text
 * 调用方 ──chan.call(origin, op, payload, budget)──▶ 本文件 ──Tauri IPC（包装层 chan_call）──▶ monitor
 *                                                    第 0 跳                                   第 1 跳在那边
 * ```
 *
 * - **期限**：`Budget.until` 是**绝对时刻**（`performance.now()` 钟面，单调）。过线换成「还剩多少」；
 *   已经过了 ⇒ `Hop{0 write, NotSent, Overrun}`，**一个字节都不发**（`§10.4` 那张表第一行）。
 *   第 1 跳的上界由 monitor 那一侧执行（`chan/webview.rs` → `router::settle`）⇒ **本文件零定时器**。
 * - **撤单**：`Budget.cancel`（`AbortSignal`）拨下 ⇒ 立即 `Ours{Cancelled}`（本地撤单，`§3.3.3`）。
 *   ⚠ **不买对端撤活**：monitor 那一侧照跑到「还剩多少」为止（与回环那条同一条边界）。
 * - **错误三层**（`§3.3.1`）：`hop`（传输错，带跳号 ＋ `reach`）· `peer`（对端说不认 / 说不行，后者带不透明体）·
 *   `ours`（我们自己错）。monitor 交回来的是回环那条同一份线上形状（`wire::err_to_wire`），这里解回三层；
 *   解不出来的一律 `ours/Broken`（对端说的话解不出来 = 内部不变量破了），**不猜**。
 *
 * # 买不到
 *
 * - **没有 `subscribe`**：webview 这一侧本拍零条流。
 * - **不买对端撤活**（同上）· **不买重连**（这一跳是进程内 IPC，没有「连接」可断）。
 */
import { commands } from "./commands";
import type { Origin } from "./origin";

/**
 * 一次调用的期限与撤单手柄。`until` 是绝对时刻（`performance.now()` 钟面），**调用者给，本文件不造**
 * （造它的那一手在调用方那一侧：`ipc/chan-caller.ts::budgetWithin`，`X5`：通信层里零「重新 `now() + …`」）。
 */
export interface Budget {
  readonly until: number;
  readonly cancel?: AbortSignal;
}

/** 离期限还剩多少毫秒（已过则为零）。**只在过线那一下用** —— 与 `wire.rs::Budget::remaining` 同一件事。 */
export function remaining(budget: Budget): number {
  return Math.max(0, Math.floor(budget.until - performance.now()));
}

export type HopTag = "open" | "auth" | "write" | "read" | "wait";
export type Reach = "NotSent" | "Sent" | "Unknown";
export type HopFault = "Unreachable" | "Dropped" | "Overrun";
export type OursFault = "Cancelled" | "Misuse" | "Broken";

/** `§3.3.1` 的三层。 */
export type CallError =
  | { layer: "hop"; at: { idx: number; tag: HopTag }; reach: Reach; why: HopFault }
  | { layer: "peer"; why: "unsupported" }
  | { layer: "peer"; why: "refused"; body: Uint8Array }
  | { layer: "ours"; why: OursFault };

/** `call` 失败时抛的那一个。`error` 是分好层的结局；`message` 只给日志看。 */
export class ChanError extends Error {
  constructor(readonly error: CallError) {
    super(`通道：${describeCallError(error)}`);
    this.name = "ChanError";
  }
}

/** 给日志 / 开发者看的一句（不是界面文案：界面那句话由调用方按层自己说）。 */
export function describeCallError(e: CallError): string {
  switch (e.layer) {
    case "hop":
      return `第 ${e.at.idx} 跳卡在 ${e.at.tag}：${e.why}（发出去了没有：${e.reach}）`;
    case "peer":
      return e.why === "unsupported" ? "对端不认这个操作" : "对端答「不行」";
    case "ours":
      return `本侧：${e.why}`;
  }
}

const HOP_TAGS: readonly string[] = ["open", "auth", "write", "read", "wait"];
const REACHES: readonly string[] = ["NotSent", "Sent", "Unknown"];
const HOP_FAULTS: readonly string[] = ["Unreachable", "Dropped", "Overrun"];
const OURS_FAULTS: readonly string[] = ["Cancelled", "Misuse", "Broken"];

const BROKEN: CallError = { layer: "ours", why: "Broken" };

/**
 * monitor 交回来的失败（`{ err, body }`，`err` 是 `wire::WireErr` 的线上形状）⇒ 三层。
 * 认不出的形状一律 `ours/Broken`（`wire.rs` 头注：「解不出来的头一律 Broken，不猜、不补默认值」）。
 */
export function decodeFail(raw: unknown): CallError {
  if (raw === null || typeof raw !== "object") return BROKEN;
  const { err, body } = raw as { err?: unknown; body?: unknown };
  const bytes = Array.isArray(body) && body.every((b) => Number.isInteger(b) && b >= 0 && b <= 255)
    ? Uint8Array.from(body as number[])
    : null;
  if (bytes === null) return BROKEN;
  if (err === "Unsupported") return { layer: "peer", why: "unsupported" };
  if (err === "Refused") return { layer: "peer", why: "refused", body: bytes };
  if (err !== null && typeof err === "object") {
    const o = err as Record<string, unknown>;
    const hop = o.Hop as Record<string, unknown> | undefined;
    if (hop !== undefined && hop !== null && typeof hop === "object") {
      const { idx, tag, reach, why } = hop;
      if (
        typeof idx === "number" &&
        Number.isInteger(idx) &&
        typeof tag === "string" &&
        HOP_TAGS.includes(tag) &&
        typeof reach === "string" &&
        REACHES.includes(reach) &&
        typeof why === "string" &&
        HOP_FAULTS.includes(why)
      ) {
        return {
          layer: "hop",
          at: { idx, tag: tag as HopTag },
          reach: reach as Reach,
          why: why as HopFault,
        };
      }
      return BROKEN;
    }
    if (typeof o.Ours === "string" && OURS_FAULTS.includes(o.Ours)) {
      return { layer: "ours", why: o.Ours as OursFault };
    }
  }
  return BROKEN;
}

/** 本地撤单那一格：拨下的那一刻就回（不等 monitor）。 */
function whenAborted(signal: AbortSignal): { promise: Promise<never>; dispose: () => void } {
  let onAbort: (() => void) | null = null;
  const promise = new Promise<never>((_, reject) => {
    onAbort = () => reject(new ChanError({ layer: "ours", why: "Cancelled" }));
    signal.addEventListener("abort", onAbort, { once: true });
  });
  return {
    promise,
    dispose: () => {
      if (onAbort) signal.removeEventListener("abort", onAbort);
    },
  };
}

/** 前端对通信层的说法（`§3.1`：前端只有两个动作；webview 这一侧今天只有 `call`）。 */
export const chan = {
  /**
   * 一次性请求。失败一定抛一个 [`ChanError`]，**永远不会**「返回一个空答案」。
   * 载荷两个方向都原样：本文件不 `JSON.parse`、不 `JSON.stringify`。
   */
  async call(origin: Origin, op: string, payload: Uint8Array, budget: Budget): Promise<Uint8Array> {
    if (budget.cancel?.aborted === true) throw new ChanError({ layer: "ours", why: "Cancelled" });
    const left = remaining(budget);
    if (left <= 0) {
      throw new ChanError({
        layer: "hop",
        at: { idx: 0, tag: "write" },
        reach: "NotSent",
        why: "Overrun",
      });
    }
    const sent = commands
      .chan_call({ origin, op, payload: Array.from(payload), leftMs: left })
      .then(
        (buf) => new Uint8Array(buf),
        (raw: unknown) => {
          throw new ChanError(decodeFail(raw));
        },
      );
    if (!budget.cancel) return sent;
    // 撤了之后 monitor 那一侧照跑完（不买对端撤活）；它那时的结局没人收了 —— 别让它变成一次未处理的拒绝。
    sent.catch(() => {});
    const abort = whenAborted(budget.cancel);
    try {
      return await Promise.race([sent, abort.promise]);
    } finally {
      abort.dispose();
    }
  },
};
