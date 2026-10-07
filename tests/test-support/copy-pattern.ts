/**
 * 测试按文案键断言：文案表里一条 `zh` 编成的正则 —— 给了值的占位处放那个值，没给的放任意值；
 * 接缝上的一个空格可有可无（取文口按值补 / 删那个空格，见 `copy-table.ts::joinSeams`）。
 *
 * 用在「有的值事先算不出（时刻 · 距今）、只要那一句在」的断言上：
 * `expect(text).toMatch(copyPattern("acct.banner.allFull", { name: "team" }))`；`whole` ⇒ 整串就是这一句（`^…$`）。
 * 值都算得出的照旧写 `copyText("键", {…})`。改表值不会撞这类断言，改了键或删了键会。
 */
import TABLE from "../../src/shared/copy/table.json";
import type { CopyKey } from "../../src/frontend/ui/copy-table";

const ENTRIES: Record<string, { zh: string }> = TABLE.entries;

const esc = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

export function copyPattern(key: CopyKey, known: Record<string, string | number> = {}, opts: { whole?: boolean } = {}): RegExp {
  const e = ENTRIES[key];
  if (!e) throw new Error(`copyPattern: no entry "${key}"`);
  const parts = e.zh.split(/(\{[A-Za-z][A-Za-z0-9]*\})/);
  let body = "";
  parts.forEach((p, i) => {
    if (i % 2 === 1) {
      const name = p.slice(1, -1);
      body += " ?" + (name in known ? esc(String(known[name])) : "[\\s\\S]*?") + " ?";
      return;
    }
    let s = p;
    if (i > 0 && s.startsWith(" ") && !s.startsWith("  ")) s = s.slice(1);
    if (i < parts.length - 1 && s.endsWith(" ") && !s.endsWith("  ")) s = s.slice(0, -1);
    body += esc(s);
  });
  for (const k of Object.keys(known)) if (!e.zh.includes(`{${k}}`)) throw new Error(`copyPattern("${key}"): no {${k}}`);
  return new RegExp(opts.whole ? `^${body}$` : body);
}
