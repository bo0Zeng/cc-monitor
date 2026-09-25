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
import type { Budget } from "./chan";

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
  if (!Array.isArray(rows)) throw new Error("应答里没有 `lines` —— 两端契约对不上");
  return rows
    .filter((r): r is string => typeof r === "string")
    .map((r) => r.trim())
    .filter((r) => r !== "");
}
