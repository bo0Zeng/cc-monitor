/**
 * 首行那一件排版小件（[`firstLineOf`]）。
 *
 * 大小 · 计数 · 时刻都不在这里：给人看的字一律由后端写好（回包里 `…Text` 那几格），界面照抄、不换算。
 */

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
 * 调用方：`queue-row.ts`（排队那一行的头）。思考那一行不经这里：后端写好 `preview`（`agents::beat::thinking`），界面照抄。
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
