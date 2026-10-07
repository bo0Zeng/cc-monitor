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
import { ChanError, type Budget, type CallError } from "../../../comms/inward/chan";
import { copyText } from "../copy-table";
import { isLocalOrigin, type Origin } from "./origin";

/**
 * 造一个期限：从现在起 `ms` 毫秒（`performance.now()` 钟面的绝对时刻）。
 * 「这条路该等多久」由每个调用点自己说（`X6`：调用点一律显式给期限，零处用库里的默认）。
 */
export function budgetWithin(ms: number, cancel?: AbortSignal): Budget {
  return { until: performance.now() + ms, cancel };
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
 * 按行那一族（`read_face` 的 `{"lines": [...]}`，C1 那几条）⇒ 逐行，trim 过、剔空行 ——
 * 与 monitor 侧 `frame_query::lines` 同形（那一份在迁过来的命令上已经没有调用方）。
 * 应答里没有 `lines` 数组 ⇒ 抛（两端契约对不上）。
 */
export function linesOf(body: Uint8Array): string[] {
  const v = readJson(body);
  const rows = v !== null && typeof v === "object" ? (v as { lines?: unknown }).lines : undefined;
  if (!Array.isArray(rows)) throw new Error(copyText("chanCaller.linesOf.badShape"));
  return rows
    .filter((r): r is string => typeof r === "string")
    .map((r) => r.trim())
    .filter((r) => r !== "");
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

/**
 * 因对方版本说不成的两个码，全产品各一句（文案表 `peerVersion.said.*`）：
 * - `backend_old`：那台后端事前就说不认这条命令（`peer/unsupported`）；
 * - `reply_unreadable`：那台回了，但回的东西认不出（不是 JSON / 形状不对）—— 只说认不出，不猜版本。
 */
export type PeerVersionCode = "backend_old" | "reply_unreadable";

/** 一句话里怎么称呼这台机器：本机说「本机」，远端说它的名字。 */
export function machineName(origin: Origin): string {
  return isLocalOrigin(origin) ? copyText("control.machine.local") : origin;
}

/** 码 ⇒ 那一句（唯一住址）。 */
export function peerVersionSaid(code: PeerVersionCode, origin: Origin): string {
  const machine = machineName(origin);
  return code === "backend_old"
    ? copyText("peerVersion.said.old", { machine })
    : copyText("peerVersion.said.unreadable", { machine });
}

/** 那台回的东西认不出。`message` 只是细目（进日志）；给人看的那句由 [`saidFrom`] 按码取。 */
export class ReplyUnreadable extends Error {
  constructor(detail: string) {
    super(detail);
    this.name = "ReplyUnreadable";
  }
}

/** 已经知道是哪台回的东西认不出 ⇒ 直接带那一句的 `Error`（不经 [`saidFrom`] 的那几处用）。 */
export function unreadableFrom(origin: Origin, detail: string): Error {
  console.warn(`reply unreadable from ${origin}: ${detail}`);
  return new Error(peerVersionSaid("reply_unreadable", origin));
}

/** 这一次失败落在两个码的哪一个上；都不是 ⇒ `null`（按层说别的原因）。JSON 解不出（`SyntaxError`）算认不出。 */
export function peerVersionCodeOf(e: unknown): PeerVersionCode | null {
  if (e instanceof ReplyUnreadable || e instanceof SyntaxError) return "reply_unreadable";
  if (e instanceof ChanError && unsupported(e.error)) return "backend_old";
  return null;
}

/**
 * 问 `origin` 那台的一次查询失败了 ⇒ 给人看的那句话。两个码按码取句（[`peerVersionSaid`]），其余按层说（[`saidByLayer`]）。
 */
export function saidFrom(e: unknown, origin: Origin): string {
  const code = peerVersionCodeOf(e);
  if (code !== null) {
    if (code === "reply_unreadable") console.warn(`reply unreadable from ${origin}:`, e instanceof Error ? e.message : e);
    return peerVersionSaid(code, origin);
  }
  return saidByLayer(e);
}

/**
 * 不落在两个码上的失败 ⇒ 按层说的那一句（[`saidFrom`] 的另一支）。
 * 不是 `ChanError` 的（调用方自己抛的）原样用它的 `message`。
 */
function saidByLayer(e: unknown): string {
  if (!(e instanceof ChanError)) return e instanceof Error ? e.message : String(e);
  const err = e.error;
  switch (err.layer) {
    case "peer": {
      // 「不认这条命令」由 [`saidFrom`] 先按码接走，到不了这里。
      if (unsupported(err)) return copyText("chanCaller.said.error");
      // 对端答了一个错误 ⇒ 就说它那一句（码不上屏：调用方要按码分支的自己读 `refusalOf`）。
      const r = refusalOf(err.body);
      return r && r.message.trim() !== "" ? r.message : copyText("chanCaller.said.error");
    }
    case "hop":
      return err.why === "Overrun"
        ? copyText("chanCaller.said.timeout")
        : copyText("chanCaller.said.unreachable");
    case "ours":
      if (err.why !== "Cancelled") return copyText("chanCaller.said.internal");
      // 那台对这一条不认撤 ⇒ 说它可能还在跑。
      return err.runsOn === true ? copyText("chanCaller.said.withdrawnRunsOn") : copyText("chanCaller.said.withdrawn");
  }
}

/**
 * 这一次失败是不是「那台后端比这条查询老」（对端**事前**就说不认这条命令）。按层判、不看文字；
 * 「需要更新」只许从这里来 —— 够不着 / 期限到 / 对端说不行都不是它（那几形照 [`saidByLayer`] 说查询失败的原因）。
 */
export function isOldBackend(e: unknown): boolean {
  return e instanceof ChanError && unsupported(e.error);
}

/** 那一形的唯一判法（[`peerVersionCodeOf`] 与 [`isOldBackend`] 共用；类型守卫 ⇒ 另一支里 `body` 可读）。 */
function unsupported(err: CallError): err is Extract<CallError, { why: "unsupported" }> {
  return err.layer === "peer" && err.why === "unsupported";
}

/**
 * 这一次失败能不能**证明一个字节都没到对端**（F14 那条安全判定：只有这时才许换一条路重做）。
 *
 * 判准与 Rust `backend_route::route_call_error` 那一收拢**同一条**（跨语言金样 `tests/__fixtures__/reach-collapse.golden.json`
 * 钉着两份：Rust 侧把 inbound 的每一种失败分层上线、连同它判出的「可回落」写成金样，本函数读同一份逐行判）：
 * - `hop` 且 `reach == NotSent`（没有控制通道 · 在飞上限顶满 · 期限在发之前就过了）⇒ 能证明；
 * - `peer/unsupported`（对端**事前**就说不认这条命令，一个字节没发）⇒ 能证明；
 * - 其余（`reach` 是 `Unknown` / `Sent` · 对端说了「不行」· 撤了 · 本侧坏了）⇒ **拿不准就按最坏算**，不能证明。
 *
 * ⚠ 它**不认识任何一条具体命令**：说的只是通道那一跳的归因（`D7`），不是业务判断。
 */
export function provablyNotSent(err: CallError): boolean {
  switch (err.layer) {
    case "hop":
      return err.reach === "NotSent";
    case "peer":
      return err.why === "unsupported";
    case "ours":
      return false;
  }
}
