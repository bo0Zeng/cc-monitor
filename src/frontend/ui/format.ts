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

/**
 * **计数的那一个读口**（几个字 · 几条 ⇒ 给人看的一格）：整数，从个位起每三位一个英文逗号，不带单位。与核心 `copy_core::count_text` 对同一份金样
 * `tests/__fixtures__/count-text.golden.json`。只给界面里现数现写的那几处（如终端页输入框）；成品里的计数核心写好。
 */
export function countText(n: number): string {
  const digits = String(Math.max(0, Math.floor(n)));
  return digits.replace(/\B(?=(\d{3})+(?!\d))/g, ",");
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
 * `tab-session-facts.ts` 的旁路记账员，那一处没有改过来。
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
