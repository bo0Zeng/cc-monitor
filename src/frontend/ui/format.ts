/**
 * 大小 · 首行 · JSON 前缀那几样排版小件（大小只经 [`sizeText`]）。
 *
 * 时刻不在这里：给人看的时刻一律由后端按看的那一台的时区写好（回包里 `…Text` 那几格），界面照抄、不换算。
 */

import { copyText } from "./copy-table";

/**
 * **大小的那一个读口**（字节 ⇒ 给人看的一格）：不满 1 KB 写整数字节；往上按 1024 进位、最多到 TB，一位小数（先化成十分位整数再四舍五入）；
 * 舍入到 1024.0 的进一档。单位格住文案表 `sizeFormat.unit.*`；与核心 `copy_core::size_text`（后端 · 壳 · 文件窗口）对同一份金样
 * `tests/__fixtures__/size-text.golden.json`。界面里别处不许自己换算大小（出口扫描判据钉着）。
 */
export function sizeText(n: number): string {
  const b = Math.max(0, Math.floor(n));
  let u = 0;
  while (u + 1 < 5 && b >= 2 ** (10 * (u + 1))) u++;
  if (u === 0) return unit(0, String(b));
  const tenths = (k: number): number => Math.floor((b * 10 + 2 ** (10 * k - 1)) / 2 ** (10 * k));
  let t = tenths(u);
  if (t >= 10_240 && u + 1 < 5) t = tenths(++u);
  return unit(u, `${Math.floor(t / 10)}.${t % 10}`);
}

/** 第 `u` 档单位（B · KB · MB · GB · TB）写上数（文案键逐档字面量）。 */
function unit(u: number, n: string): string {
  if (u === 0) return copyText("sizeFormat.unit.b", { n });
  if (u === 1) return copyText("sizeFormat.unit.kb", { n });
  if (u === 2) return copyText("sizeFormat.unit.mb", { n });
  if (u === 3) return copyText("sizeFormat.unit.gb", { n });
  return copyText("sizeFormat.unit.tb", { n });
}

/**
 * **第一条非空行**（trim 过），最多取 `max` 个字符。
 *
 * 为什么不 `text.split("\n").find(…)`：那是 O(整条) —— 单条正文最大 617 KB，
 * 而这里只要 ≤ 60 个字。本函数只走到第一条非空行为止，而且**不切整行**：
 * 行首 / 行尾的空白各扫一次，正文只切出前 `max` 个字符。
 *
 * - `line`：那一行 trim 之后的前 `max` 个字符（没有非空行 ⇒ `""`）；
 * - `more`：那一行 trim 之后比 `max` 长。
 *
 * 空白的口径与 `String.prototype.trim` 相同（JS 的 `\s` 就是 WhiteSpace ∪ LineTerminator）。
 * 调用方：`cards/index.ts::firstLinePreview`；点名的第三处（agent 标签取首行）住
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
 * 〔「只序列化头几个 key」〕`JSON.stringify(v)` 的**前缀**：
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
