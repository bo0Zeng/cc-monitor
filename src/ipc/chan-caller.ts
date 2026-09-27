/**
 * 〔C4a · 第四波〕**通道的调用方那一侧** —— 调用方与通道之间那一薄层：期限怎么造 ＋ 后端的帧命令说 JSON。
 *
 * - **期限**（[`budgetWithin`]）：`设计/05 §3.3.2`「值归调用方 · 执行归通信层」—— 绝对时刻由调用方造，
 *   通信层（`src/ipc/chan.ts`）只执行、从不「重新 `now() + …`」（`X5`）。⇒ 造的那一手住这里，不住成员里。
 * - **JSON**：通道只搬不透明字节，**不知道**载荷长什么样（`§2`：载荷是不透明字节）。
 *   而后端帧面上的每一条命令（`src/backend/inbound.rs` / `read_face.rs`）收一个 JSON 对象、回一个 JSON 值 ——
 *   那是**后端的**约定，不是通道的。⇒ 这件事住这里，与 monitor 侧 `chan/host.rs` 那一句
 *   「这个后端说 JSON：载荷在这里才第一次被当成 JSON 读」对称。
 *
 * ⚠ 本文件**不是**通信层成员（它造期限、解释载荷）；也不认识任何一条具体命令 —— 那是各调用方的事。
 */
import { ChanError, type Budget, type CallError } from "./chan";
import { copyText } from "../copy-table";

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

/** 对端「不行」的那份体（后端的拒绝信封 `{code, message}`）⇒ `(code, message)`；解不出 ⇒ `null`。 */
export function refusalOf(body: Uint8Array): { code: string; message: string } | null {
  try {
    const v = readJson(body);
    if (v !== null && typeof v === "object") {
      const { code, message } = v as { code?: unknown; message?: unknown };
      if (typeof code === "string" && typeof message === "string") return { code, message };
    }
  } catch {
    // 体不是 JSON ⇒ 当成没说原因
  }
  return null;
}

/**
 * 〔C4b · 第四波 4B〕一次经通道的查询失败了 ⇒ 给人看的那句话（`设计/05 §3.3.2`「说法归调用方」）。
 *
 * 各调用方共用这一份「按层说」，只各自给出「那台后端比这条查询老」时那句话（它们说的功能不同）。
 * 不是 `ChanError` 的（调用方自己抛的，如应答形状不对）原样用它的 `message`。
 */
export function saidOf(e: unknown, oldBackendSays: string): string {
  if (!(e instanceof ChanError)) return e instanceof Error ? e.message : String(e);
  const err = e.error;
  switch (err.layer) {
    case "peer": {
      if (err.why === "unsupported") return oldBackendSays;
      const r = refusalOf(err.body);
      return r ? copyText("chanCaller.said.errorCoded", { code: r.code, message: r.message }) : copyText("chanCaller.said.error");
    }
    case "hop":
      return err.why === "Overrun"
        ? copyText("chanCaller.said.timeout")
        : copyText("chanCaller.said.unreachable");
    case "ours":
      if (err.why !== "Cancelled") return copyText("chanCaller.said.internal");
      // 〔NET2 · `05 §3.3.3`〕那台对这一条不认撤 ⇒ 说它可能还在跑。
      return err.runsOn === true ? copyText("chanCaller.said.withdrawnRunsOn") : copyText("chanCaller.said.withdrawn");
  }
}

/**
 * 〔C4e · 第四波 4C〕这一次失败能不能**证明一个字节都没到对端**（F14 那条安全判定：只有这时才许换一条路重做）。
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
