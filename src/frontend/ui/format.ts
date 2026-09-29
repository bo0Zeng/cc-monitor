/**
 * 时间 / 字节 格式化（P2.2）。
 *
 * 之前 formatTime 有两份（cards/index.ts 接 ISO string、views/history.ts 接 ms
 * number）、formatBytes 有两份（data-section / diagnostics-section 精度不同）。
 * 集中收口避免后续漂移。
 *
 * 保留两套时间语义：
 * - **formatTimestampShort**：消息卡片显示时间戳，永远 `hh:mm`
 * - **formatTimestampSmart**：会话活动时间，当天 `hh:mm`，跨天 `yyyy-MM-dd hh:mm`
 */

/** 输入 ISO 字符串或 unix ms，返回 `hh:mm`（解析失败返原值字符串）。 */
import { copyText } from "./copy-table";

export function formatTimestampShort(input: string | number): string {
  try {
    const d = typeof input === "number" ? new Date(input) : new Date(input);
    if (Number.isNaN(d.getTime())) return String(input);
    return d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  } catch {
    return String(input);
  }
}

/** 输入 unix ms，当天显示 `hh:mm`，跨天显示完整 `yyyy-MM-dd hh:mm`；0/NaN 返 "—"。 */
export function formatTimestampSmart(ms: number): string {
  if (!ms) return copyText("format.formatTimestampSmart.empty");
  try {
    const d = new Date(ms);
    if (Number.isNaN(d.getTime())) return String(ms);
    const today = new Date();
    const sameDay =
      d.getFullYear() === today.getFullYear() &&
      d.getMonth() === today.getMonth() &&
      d.getDate() === today.getDate();
    if (sameDay) {
      return d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
    }
    return d.toLocaleString([], {
      year: "numeric",
      month: "2-digit",
      day: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
    });
  } catch {
    return String(ms);
  }
}

/** 字节数 → 人类可读：B / KB(1d) / MB(1d) / GB(2d) */
export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / (1024 * 1024)).toFixed(1)} MB`;
  return `${(n / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

/**
 * 路径的最后一段（给人看的短名；兼容 `\` 分隔与尾随 `/`）。
 *
 * 〔F7b〕从退役的老 SFTP 面板（`sftp/paths.ts`）搬来 —— 今天唯一的消费者是 viewer 窗的标题
 * （`entry-viewer.ts`：会话工作目录 → 窗口名）。它不切**远端**路径给后端用：那件事归 Rust 侧
 * `filewin::source::remote_basename`，只认 `/`。
 */
export function basename(path: string): string {
  const norm = path.replace(/\\/g, "/").replace(/\/+$/, "");
  const i = norm.lastIndexOf("/");
  return i >= 0 ? norm.slice(i + 1) : norm;
}

/**
 * 〔W5-RENDER R2 · `设计/17 §2.5`〕**第一条非空行**（trim 过），最多取 `max` 个字符。
 *
 * 为什么不 `text.split("\n").find(…)`：那是 O(整条) —— 单条正文最大 617 KB（`设计/17 §1.1`），
 * 而这里只要 ≤ 60 个字。本函数只走到第一条非空行为止，而且**不切整行**：
 * 行首 / 行尾的空白各扫一次，正文只切出前 `max` 个字符。
 *
 * - `line`：那一行 trim 之后的前 `max` 个字符（没有非空行 ⇒ `""`）；
 * - `more`：那一行 trim 之后比 `max` 长。
 *
 * 空白的口径与 `String.prototype.trim` 相同（JS 的 `\s` 就是 WhiteSpace ∪ LineTerminator）。
 * 调用方：`cards/index.ts::firstLinePreview`；`设计/17 §2.5` 点名的第三处（agent 标签取首行）住
 * `tab-session-facts.ts` 的旁路记账员（STC 写区），那一处没有改过来。
 */
export function firstLineOf(text: string, max: number): { line: string; more: boolean } {
  const ws = /\s/;
  let start = 0;
  const len = text.length;
  while (start <= len) {
    const nl = text.indexOf("\n", start);
    const end = nl < 0 ? len : nl;
    let a = start;
    while (a < end && ws.test(text[a])) a++;
    if (a < end) {
      let b = end;
      while (b > a && ws.test(text[b - 1])) b--;
      const more = b - a > max;
      return { line: text.slice(a, more ? a + max : b), more };
    }
    if (nl < 0) break;
    start = nl + 1;
  }
  return { line: "", more: false };
}

/**
 * 〔W5-RENDER R2 · `设计/17 §2.5`「只序列化头几个 key」〕`JSON.stringify(v)` 的**前缀**：
 * 一旦攒够 `limit + 1` 个字符就停，返回的串逐字等于 `JSON.stringify(v)` 的开头
 * （长度 > `limit` ⇔ 完整序列化也 > `limit`）。整份序列化是 O(整条)，而调用方只要 60 个字。
 *
 * 等价的几处要想清楚（`tests/frontend/ui/cards/json-prefix.vitest.ts` 用随机值逐字对拍）：
 * - 字符串（值与键）先切前 `limit + 1` 个字符再转义：转义逐字符、保前缀。切口若劈开代理对，
 *   被转义成 `\udxxx` 的那半在输出里的位置 ≥ `limit + 1`，不进被用到的前 `limit` 个字符。
 * - 对象按 `Object.keys` 的序；值为 `undefined` / 函数 / symbol 的键跳过，数组里的这类值写 `null`
 *   （与 `JSON.stringify` 同口径）；数字走 `JSON.stringify(n)`（非有限 ⇒ `null`）。
 * - 带 `toJSON` 的对象、`bigint` 等不常见形 ⇒ 整份退回 `JSON.stringify`（它会抛就照抛，调用方原样兜）。
 * - 环 ⇒ 抛 `TypeError`（与 `JSON.stringify` 同）。
 */
export function jsonPrefix(v: unknown, limit: number): string | undefined {
  const out: string[] = [];
  let n = 0;
  const FULL = Symbol("full");
  const emit = (s: string): void => {
    out.push(s);
    n += s.length;
    if (n > limit) throw FULL;
  };
  const str = (s: string): string => JSON.stringify(s.length > limit + 1 ? s.slice(0, limit + 1) : s);
  const stack = new Set<object>();
  const skip = (x: unknown): boolean => x === undefined || typeof x === "function" || typeof x === "symbol";
  const walk = (x: unknown): void => {
    if (x === null) return emit("null");
    switch (typeof x) {
      case "string":
        return emit(str(x));
      case "number":
      case "boolean":
        return emit(JSON.stringify(x));
      case "object":
        break;
      default:
        // bigint 等：交给 JSON.stringify 自己决定（抛就抛）
        return emit(JSON.stringify(x) ?? "null");
    }
    const o = x as object;
    if (typeof (o as { toJSON?: unknown }).toJSON === "function") return emit(JSON.stringify(o) ?? "null");
    if (stack.has(o)) throw new TypeError("Converting circular structure to JSON");
    stack.add(o);
    if (Array.isArray(o)) {
      emit("[");
      for (let i = 0; i < o.length; i++) {
        if (i > 0) emit(",");
        const it = o[i] as unknown;
        if (skip(it)) emit("null");
        else walk(it);
      }
      emit("]");
    } else {
      emit("{");
      let first = true;
      for (const k of Object.keys(o)) {
        const it = (o as Record<string, unknown>)[k];
        if (skip(it)) continue;
        if (!first) emit(",");
        first = false;
        emit(str(k));
        emit(":");
        walk(it);
      }
      emit("}");
    }
    stack.delete(o);
  };
  if (skip(v)) return JSON.stringify(v);
  try {
    walk(v);
  } catch (e) {
    if (e !== FULL) throw e;
  }
  return out.join("");
}
