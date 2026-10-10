/**
 * 出口扫描判据的规则表（`exit-judgment-scan.vitest.ts` 用；单列一份是为了让判据与它的阳性对照读同一张表）。
 *
 * 「出口零判定零格式化」（`src/doc/ARCHITECTURE.md` §2.9）：界面与壳只排版，判定与描述数据的字都由核心写。
 * 这里每条规则是那件事的一种**写法指纹**：自己按状态码取字、自己拼百分比、自己写时长单位、自己换算时刻、自己按状态排档、自己换算大小。
 * 指纹抓的是写法，不是语义：换个写法重写一遍抓不到（与 `judgment-single-home.vitest.ts` 同一类边界）；
 * 抓到的也不全是违例（界面自己的骨架词表也会撞上 `case "x": return copyText(`）——欠账名单里每条写明是哪一种。
 */

/** 一条规则：在**剥过注释**的生产代码里数命中次数。 */
export interface ExitRule {
  id: string;
  /** 抓的是什么写法（一句）。 */
  what: string;
  re: RegExp;
  /** 阳性对照：每一段都必须恰好命中一次（探测器坏了 ⇒ 先红）。 */
  hits: string[];
  /** 阴性对照：每一段都必须零命中（规则不许宽到把这些也算进来）。 */
  misses: string[];
}

/** 界面（TS）那几条。 */
export const TS_RULES: ExitRule[] = [
  {
    id: "codeWord",
    what: "按后端码（code · state · kind · status · activity · slot …）三元 / if 取字：`x.code === \"dup\" ? copyText(…)`",
    re: /(?:\.|\b)(?:code|state|kind|status|activity|doing|slot|liveness|mode|why|layer|tone)\s*===\s*"[^"\n]*"\s*\)?\s*(?:\?|return|&&)\s*copyText\(/g,
    hits: ['e.code === "dup" ? copyText("a.b.c")', 'if (slot === "5h") return copyText("a.b.c")', 'q.state === "refused" && copyText("a.b.c")'],
    misses: ['e.cell === "name" ? copyText("a.b.c")', 'x.code === "dup" ? 1 : 2'],
  },
  {
    id: "caseWord",
    what: "按码 `switch` 取字：`case \"rejected\": return copyText(…)`（含 `return { text: copyText(…) }`）",
    re: /case\s+"[^"\n]+":\s*return\s+(?:\{[^}\n]*?)?copyText\(/g,
    hits: ['case "rejected":\n      return copyText("a.b.c");', 'case "x": return { text: copyText("a.b.c"), needs: false };'],
    misses: ['case "x":\n      break;', 'case "x": return e.said;'],
  },
  {
    id: "caseRank",
    what: "按状态排档的表：`case \"needs_you\": return 0;`",
    re: /case\s+"[^"\n]+":\s*return\s+-?\d+\s*;/g,
    hits: ['case "needs_you":\n        return 0;'],
    misses: ['case "x":\n        return a - b;'],
  },
  {
    id: "pct",
    what: "自己拼百分比：模板里 `${n}%` · `+ \"%\"`",
    re: /\$\{[^}\n]*\}%|\+\s*"%"/g,
    hits: ["`${p}%`", 'n + "%"'],
    misses: ['copyText("a.b.c", { pct: p })', "`100%`"],
  },
  {
    id: "durUnit",
    what: "自己写时长单位：模板里插值后紧跟 `s` / `m` / `h` / `d` / `ms`",
    re: /\}(?:ms|s|m|h|d)(?=`|\$\{)/g,
    hits: ["`${s}s`", "`${m}m${x}`"],
    misses: ["`${a}.${b}`", "`${n} rows`"],
  },
  {
    id: "clock",
    what: "自己换算时刻：`Date.parse(` · `toLocale…(` · `toTimeString(` · `toDateString(` · `getHours(` · `new Date(`",
    re: /Date\.parse\(|\.toLocale\w*\(|\.toTimeString\(|\.toDateString\(|\.getHours\(|new Date\(/g,
    hits: ["Date.parse(at)", "d.toLocaleTimeString()", "new Date(ms)"],
    misses: ["Date.now()", "parse(at)"],
  },
  {
    id: "toFixed",
    what: "自己取小数位（时长 / 大小 / 比例的格式化）：`.toFixed(`",
    re: /\.toFixed\(/g,
    hits: ["(ms / 1000).toFixed(1)"],
    misses: ["fixed(1)"],
  },
  {
    id: "size",
    what: "自己换算大小：`/ 1024`",
    re: /\/\s*1024\b/g,
    hits: ["n / 1024", "x/1024"],
    misses: ["n / 10240", "1024"],
  },
];

/** 壳（Rust）那几条：壳自己操作的失败句是宿主本职（不在这里）；描述数据的数字格式化在这里。 */
export const RS_RULES: ExitRule[] = [
  {
    id: "pct",
    what: "自己拼百分比：`format!(\"{…}%\")`",
    re: /format!\(\s*"[^"\n]*\}%/g,
    hits: ['format!("{}%", p)', 'format!("{p:.0}%")'],
    misses: ['format!("{}", p)'],
  },
  {
    id: "durUnit",
    what: "自己写时长单位：`format!(\"{…}s\")` 一类",
    re: /format!\(\s*"[^"\n]*\}(?:ms|s|m|h|d)\b/g,
    hits: ['format!("{}s", n)', 'format!("{m}m{s}s")'],
    misses: ['format!("{} sessions", n)'],
  },
  {
    id: "size",
    what: "自己换算大小：`/ 1024`",
    re: /\/\s*1024(?:\.0)?\b/g,
    hits: ["n / 1024", "x / 1024.0"],
    misses: ["n / 10240"],
  },
];
