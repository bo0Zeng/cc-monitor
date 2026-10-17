/**
 * **通道的调用方那一侧** —— 调用方与通道之间那一薄层：期限怎么造 ＋ 后端的帧命令说 JSON。
 *
 * - **期限**（[`budgetWithin`]）：「值归调用方 · 执行归通信层」—— 绝对时刻由调用方造，
 *   通信层（`src/comms/inward/chan.ts`）只执行、从不「重新 `now() + …`」（`X5`）。⇒ 造的那一手住这里，不住成员里。
 * - **JSON**：通道只搬不透明字节，**不知道**载荷长什么样（`§2`：载荷是不透明字节）。
 *   而后端帧面上的每一条命令（`src/backend/stream/inbound/mod.rs` / `read_face.rs`）收一个 JSON 对象、回一个 JSON 值 ——
 *   那是**后端的**约定，不是通道的。⇒ 这件事住这里，与 monitor 侧 `chan/host.rs` 那一句
 *   「这个后端说 JSON：载荷在这里才第一次被当成 JSON 读」对称。
 *
 * ⚠ 本文件**不是**通信层成员（它造期限、解释载荷）；也不认识任何一条具体命令 —— 那是各调用方的事。
 */
import { chan, ChanError, type Budget } from "../../../comms/inward/chan";
import { copyText } from "../copy-table";
import { isLocalOrigin, type Origin } from "./origin";
import type { TypedCommands } from "../generated/TypedCommands";

/**
 * 问一条 **typed** 帧命令：入参 · 回包的类型都从 Rust 生成（`generated/TypedCommands.ts`）。
 * 两端形状由握手时的形状指纹核过（对不上 monitor 事前就拒，码 `backend_differs`、那一句它写好），回包这里直接按类型用、不再逐格解。
 * 回的不是 JSON ⇒ 抛（`readJson`，认不出）。
 */
export async function callTyped<K extends keyof TypedCommands>(
  origin: Origin,
  cmd: K,
  req: TypedCommands[K]["req"],
  budget: Budget,
  view: object | null = null,
): Promise<TypedCommands[K]["reply"]> {
  const body = jsonBody(req as Record<string, unknown>);
  const reply = await chan.call(origin, cmd, body, budget, view);
  return readJson(reply) as TypedCommands[K]["reply"];
}

/**
 * 造一个期限：从现在起 `ms` 毫秒（`performance.now()` 钟面的绝对时刻）。
 * 「这条路该等多久」由每个调用点自己说（`X6`：调用点一律显式给期限，零处用库里的默认）。
 */
export function budgetWithin(ms: number): Budget {
  return { until: performance.now() + ms };
}

const enc = new TextEncoder();
const dec = new TextDecoder();

/** 一个 JSON 对象 ⇒ 帧命令的请求体。 */
export function jsonBody(args: Record<string, unknown>): Uint8Array {
  return enc.encode(JSON.stringify(args));
}

/** 帧命令应答体 ⇒ JSON 值。解不出来就抛（那是两端契约对不上，不是「空答案」）。 */
export function readJson(body: Uint8Array): unknown {
  return JSON.parse(dec.decode(body));
}

/**
 * 对端「不行」的那份体（后端的拒绝信封 `{code, message, data?}`）⇒ `(code, message, data)`；解不出 ⇒ `null`。
 * `data` 只有协议里按码定了形的那几个码带（今天只有 `account_unavailable`），按码收的那一侧自己核形状。
 */
export function refusalOf(body: Uint8Array): { code: string; message: string; data?: unknown } | null {
  try {
    const v = readJson(body);
    if (v !== null && typeof v === "object") {
      const { code, message, data } = v as { code?: unknown; message?: unknown; data?: unknown };
      if (typeof code === "string" && typeof message === "string") return data === undefined ? { code, message } : { code, message, data };
    }
  } catch {
    // 体不是 JSON ⇒ 当成没说原因
  }
  return null;
}

/** 一句话里怎么称呼这台机器：本机说「本机」，远端说它的名字。 */
export function machineName(origin: Origin): string {
  return isLocalOrigin(origin) ? copyText("control.machine.local") : origin;
}

/** 那台回了、回的东西认不出那一句（[`ReplyUnreadable`] 的 `message`；不抛、只要那一句的几处直接取）。 */
export function unreadableSaid(origin: Origin): string {
  return copyText("peerVersion.said.unreadable", { machine: machineName(origin) });
}

/**
 * 那台回了，但回的东西认不出（不是 JSON / 形状不对）—— 只说认不出，不猜版本。这一句是界面自己知道的事，界面说
 * （通道上的失败那一句由 monitor 写，在 `ChanError.message`）。`detail` 恒空串：屏上就是这一句、不出［复制详情］；细目只进日志。
 */
export class ReplyUnreadable extends Error {
  readonly detail = "";
  constructor(origin: Origin, why: string) {
    super(unreadableSaid(origin));
    this.name = "ReplyUnreadable";
    console.warn(`reply unreadable from ${origin}: ${why}`);
  }
}

/** 这一次失败是对端拒了 ⇒ 拒绝体（码 · 那一句 · `data`，[`refusalOf`]）；别的失败 · 体解不出 ⇒ `null`。按码分支的调用方只经这里读。 */
export function refusalIn(e: unknown): { code: string; message: string; data?: unknown } | null {
  return e instanceof ChanError && e.error.layer === "peer" && e.error.why === "refused" ? refusalOf(e.error.body) : null;
}

/**
 * 这一次失败是不是「那台后端比这条查询老」（对端**事前**就说不认这条命令）。按层判、不看文字；
 * 那一句 monitor 已经写在 `message` 里，这里只给要换样子的那几处（「需要更新」）判形。
 */
export function isOldBackend(e: unknown): boolean {
  return e instanceof ChanError && e.error.layer === "peer" && e.error.why === "unsupported";
}
