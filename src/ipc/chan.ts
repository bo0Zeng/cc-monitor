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
 * # 〔CF2 · 第四波 4B〕`subscribe(origin, kind, from, want)`
 *
 * ```text
 * 调用方 ──chan.subscribe(origin, kind, from, want, sink)──▶ 本文件 ──包装层 chan_subscribe──▶ monitor（句柄）
 *        ◀── sink(items) ── 本文件 ◀── 窗口作用域的 Tauri 事件 `chan-items` {sub, items} ──
 *        ── sub.want(n) / sub.stop() ──▶ chan_want / chan_stop
 * ```
 *
 * - **不返回 `Result`**（`§3.3.5`）：订一台现在看不见的机器是合法的 —— 第一格 `unseen`；说不了的原位 `closed`。
 *   登记那一跳本身失败（包装层抛）⇒ 原位 `closed{ours: Broken}`，返回的 `Sub` 是一个撤了的。
 * - **编号本文件给**（每页从 1 起）：格可能先于登记那一跳的应答到达 —— 编号与 `sink` 先记在这里才不丢。
 * - **体是文本**：webview 这一跳是 Tauri 事件（JSON），二进制过不来 ⇒ 句柄的字节按 UTF-8 原样装成字符串。
 *   本文件**不解析**它（`closed{peer}` 的体同）；读它的是调用方（`events.ts`）。
 * - **credit**：`want` 由调用方给（它知道自己还能吃多少）；本文件只转交，不排队、不丢 ——
 *   丢是句柄那一侧的事，丢了它原位给 `gap`（`§3.3.4`「丢必须说」）。
 * - 解不出的一格 ⇒ 交一格 `closed{ours: Broken}` 并撤掉这条订阅（对端说的话解不出来 = 不变量破了），**不猜**。
 *
 * # 买不到
 *
 * - **不买对端撤活**（同上）· **不买重连**（这一跳是进程内 IPC，没有「连接」可断）。
 * - **`subscribe` 没有续传**：webview 页面一重载 JS 状态全没，游标无处可存 ⇒ 句柄对 `from` 原位说用法错。
 */
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
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
  | { layer: "ours"; why: OursFault; runsOn?: boolean };

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
    // 〔NET2 · additive〕本侧撤了、那台对这一条不认撤 ⇒ 它可能还在跑。
    if (typeof o.OursRunsOn === "string" && OURS_FAULTS.includes(o.OursRunsOn)) {
      return { layer: "ours", why: o.OursRunsOn as OursFault, runsOn: true };
    }
  }
  return BROKEN;
}

/**
 * 〔NET2〕那台机器的能力事实（monitor 那边的 `chan::wire::Offer` 判完之后的一份拷贝，`chan/webview.rs::OfferView`）。
 * 本侧**只查成员**，不另算：做不到的那几条带码；`stoppable` 是撤掉之后停得下的那几条。
 */
export interface Offer {
  readonly ops: readonly string[];
  readonly unavailable: readonly (readonly [string, string])[];
  readonly stoppable: readonly string[];
}

/** 这台做不到 `op` ⇒ 那个码（与那台事后会回的同一个）；做得到 / 没把握 ⇒ `null`。 */
export function unavailableCode(offer: Offer | null | undefined, op: string): string | null {
  return offer?.unavailable.find(([o]) => o === op)?.[1] ?? null;
}

/** 每台一份：第一次对它 `call` 时去问（不等它），通道那一跳出了事就作废（重连之后可能换了一份）。 */
const offers = new Map<Origin, Offer | null>();
const asking = new Map<Origin, Promise<Offer | null>>();

const strings = (v: unknown): v is string[] => Array.isArray(v) && v.every((x) => typeof x === "string");

/** monitor 交回来的那一份 ⇒ `Offer`；形状不对 ⇒ `null`（当作没把握，不猜）。 */
export function decodeOffer(raw: unknown): Offer | null {
  if (raw === null || typeof raw !== "object") return null;
  const { ops, unavailable, stoppable } = raw as Record<string, unknown>;
  const pairs =
    Array.isArray(unavailable) &&
    unavailable.every((p) => Array.isArray(p) && p.length === 2 && strings(p));
  if (!strings(ops) || !pairs || !strings(stoppable)) return null;
  return { ops, unavailable: unavailable as [string, string][], stoppable };
}

function askOffer(origin: Origin): Promise<Offer | null> {
  const inFlight = asking.get(origin);
  if (inFlight) return inFlight;
  const p = commands
    .chan_offer({ origin })
    .then(
      (raw) => {
        const o = decodeOffer(raw);
        offers.set(origin, o);
        return o;
      },
      () => null,
    )
    .finally(() => asking.delete(origin));
  asking.set(origin, p);
  return p;
}

/** 本地撤单那一格：拨下的那一刻就回（不等 monitor）。`runsOn`：那台对这一条不认撤 ⇒ 它可能还在跑。 */
function whenAborted(signal: AbortSignal, runsOn: () => boolean): { promise: Promise<never>; dispose: () => void } {
  let onAbort: (() => void) | null = null;
  const promise = new Promise<never>((_, reject) => {
    onAbort = () =>
      reject(new ChanError(runsOn() ? { layer: "ours", why: "Cancelled", runsOn: true } : { layer: "ours", why: "Cancelled" }));
    signal.addEventListener("abort", onAbort, { once: true });
  });
  return {
    promise,
    dispose: () => {
      if (onAbort) signal.removeEventListener("abort", onAbort);
    },
  };
}

/** `§3.3.4` 流里的一格（webview 这一跳的样子：体是文本，见头注）。 */
export type Item =
  | { t: "frame"; seq: number; body: string }
  | { t: "gap"; fromSeq: number; toSeq: number }
  | { t: "unseen"; at: { idx: number; tag: HopTag }; why: HopFault }
  | { t: "seen"; from: Uint8Array | null }
  | { t: "closed"; by: { peer: string } | { ours: OursFault } };

/** 一条订阅往回说的两个动作（`§3.3.0` 的 `Sub`）。 */
export interface Sub {
  /** 背压信号：订阅方报「我还能吃多少」（credit，累加）。 */
  want(more: number): void;
  /** 撤订阅（本地撤单）：之后一格都不再交给 sink。 */
  stop(): void;
}

const BROKEN_ITEM: Item = { t: "closed", by: { ours: "Broken" } };

const isNat = (v: unknown): v is number => typeof v === "number" && Number.isInteger(v) && v >= 0;

/**
 * monitor 交来的一格（`chan/webview.rs::WebviewItem` 的线上形状，跨语言金样
 * `tests/__fixtures__/chan-webview-items.golden.json`）⇒ `Item`。认不出 ⇒ `null`（调用方按「坏了」处置）。
 */
export function decodeItem(raw: unknown): Item | null {
  if (raw === null || typeof raw !== "object") return null;
  const o = raw as Record<string, unknown>;
  switch (o.t) {
    case "frame":
      return isNat(o.seq) && typeof o.body === "string" ? { t: "frame", seq: o.seq, body: o.body } : null;
    case "gap":
      return isNat(o.from_seq) && isNat(o.to_seq) && o.from_seq <= o.to_seq
        ? { t: "gap", fromSeq: o.from_seq, toSeq: o.to_seq }
        : null;
    case "unseen":
      return isNat(o.idx) &&
        typeof o.tag === "string" &&
        HOP_TAGS.includes(o.tag) &&
        typeof o.why === "string" &&
        HOP_FAULTS.includes(o.why)
        ? { t: "unseen", at: { idx: o.idx, tag: o.tag as HopTag }, why: o.why as HopFault }
        : null;
    case "seen": {
      if (o.from === null) return { t: "seen", from: null };
      const f = o.from;
      return Array.isArray(f) && f.every((b) => Number.isInteger(b) && b >= 0 && b <= 255)
        ? { t: "seen", from: Uint8Array.from(f as number[]) }
        : null;
    }
    case "closed_by_peer":
      return typeof o.body === "string" ? { t: "closed", by: { peer: o.body } } : null;
    case "closed_by_ours":
      return typeof o.why === "string" && OURS_FAULTS.includes(o.why)
        ? { t: "closed", by: { ours: o.why as OursFault } }
        : null;
    default:
      return null;
  }
}

/** 本页的订阅：编号 ⇒ 交格的那个 sink（编号每页从 1 起，见头注）。 */
const sinks = new Map<number, (items: Item[]) => void>();
let nextSubId = 1;
let listening: Promise<unknown> | null = null;

/** 一次投递（`{sub, items}`）⇒ 解码后交给那条订阅的 sink。解不出的 ⇒ 交 `Broken` 并撤掉。 */
function dispatchDelivery(raw: unknown): void {
  if (raw === null || typeof raw !== "object") return;
  const { sub, items } = raw as { sub?: unknown; items?: unknown };
  if (!isNat(sub)) return;
  const sink = sinks.get(sub);
  if (!sink) return; // 撤了之后才到的格：本地撤单立即，之后一格都不交
  const out: Item[] = [];
  let broken = !Array.isArray(items);
  for (const it of Array.isArray(items) ? (items as unknown[]) : []) {
    const d = decodeItem(it);
    if (d === null) {
      broken = true;
      break;
    }
    out.push(d);
  }
  if (broken) {
    sinks.delete(sub);
    void commands.chan_stop({ id: sub }).catch(() => {});
    out.push(BROKEN_ITEM);
  }
  if (out.length > 0) sink(out);
}

/** 前端对通信层的说法（`§3.1`：前端只有两个动作）。 */
export const chan = {
  /** 〔NET2〕那台的能力事实：先问一次（`null` = 没有控制通道）。 */
  offer(origin: Origin): Promise<Offer | null> {
    return askOffer(origin);
  },

  /** 〔NET2〕手里那份（没问过 ⇒ `undefined`；界面画菜单时用它，同时 `offer()` 去补）。 */
  cachedOffer(origin: Origin): Offer | null | undefined {
    return offers.get(origin);
  },

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
          const e = decodeFail(raw);
          if (e.layer === "hop") offers.delete(origin);
          throw new ChanError(e);
        },
      );
    if (!offers.has(origin)) void askOffer(origin);
    if (!budget.cancel) return sent;
    // 撤了之后 monitor 那一侧照跑完（不买对端撤活）；它那时的结局没人收了 —— 别让它变成一次未处理的拒绝。
    sent.catch(() => {});
    const offer = (): Offer | null | undefined => offers.get(origin);
    const abort = whenAborted(budget.cancel, () => {
      const o = offer();
      return o !== null && o !== undefined && !o.stoppable.includes(op);
    });
    try {
      return await Promise.race([sent, abort.promise]);
    } finally {
      abort.dispose();
    }
  },

  /**
   * 订阅（`§3.3.0`）。**不失败**：说不了的在流里原位说（`§3.3.5`）。返回时 monitor 那一侧已经登记好了
   * （调用方可以放心地接着做「会触发交格」的事，比如发 `frontend-ready`）。
   * `from`：续传游标（不透明；webview 这一跳的句柄不支持，给了就原位 `closed`）。`want`：一开始能吃多少格。
   */
  async subscribe(
    origin: Origin,
    kind: string,
    from: Uint8Array | null,
    want: number,
    sink: (items: Item[]) => void,
  ): Promise<Sub> {
    listening ??= getCurrentWebviewWindow().listen<unknown>("chan-items", (e) => {
      dispatchDelivery(e.payload);
    });
    const id = nextSubId++;
    sinks.set(id, sink);
    const sub: Sub = {
      want: (more) => {
        if (!sinks.has(id) || more <= 0) return;
        // 〔W5-UI · E §3.3〕信用报不上去 ⇒ monitor 那一侧再也不会交格（实时格没信用就丢）——原先这里吞掉，
        //   流**静默停住**。按本对象的契约「说不了的在流里原位说」：撤单、交一格 `closed{ours: Broken}`
        //   （与登记那一跳失败同一形），由消费方出声。
        void commands.chan_want({ id, more }).catch(() => {
          if (!sinks.delete(id)) return;
          void commands.chan_stop({ id }).catch(() => {});
          sink([BROKEN_ITEM]);
        });
      },
      stop: () => {
        if (!sinks.delete(id)) return;
        void commands.chan_stop({ id }).catch(() => {});
      },
    };
    try {
      await listening;
      await commands.chan_subscribe({
        origin,
        kind,
        from: from === null ? null : Array.from(from),
        want,
        id,
      });
    } catch {
      if (sinks.delete(id)) sink([BROKEN_ITEM]);
    }
    return sub;
  },
};
