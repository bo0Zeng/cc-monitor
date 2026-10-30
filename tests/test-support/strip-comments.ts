/**
 * 剥注释——**守卫用**，测试里唯一的一份（Rust · TS · Kotlin · CSS 共用一个扫描器，方言差异由 `lang` 决定）。
 * 别的测试不再各写一份正则剥法：两份剥法语义一漂，就是静默削弱守卫。
 *
 * **只被测试引用 ⇒ 进不了 bundle**：没有任何生产文件 import `tests/test-support/`
 * （`generated-boundary-guard.vitest.ts` 的「生产代码不许 import test-support」机检）。
 *
 * ## 为什么是个状态机，而不是两条正则
 *
 * `src.replace(/\/\*[\s\S]*?\*\//g, "").replace(/\/\/.*$/gm, "")` 那一形认不出字符串：
 * `"~/.local/*\/bin"` 里的 `/*` 会被当成块注释开始、一路吃到下一个 `*\/`；`"https://…"` 里的 `//` 会把那一行剩下的代码剥掉。
 * 危险程度按断言方向分两档：对肯定式断言（差集、等号计数）过剥 ⇒ 假红，会叫；
 * 对否定式断言（`.not.toContain(…)`）过剥 ⇒ 假绿，永远不叫。⇒ 字符串、字符字面量、TS 正则字面量里的注释起始符一律不算。
 *
 * ## 各方言认什么
 *
 * | 方言 | 行注释 | 块注释 | 当不透明字面量跳过的 |
 * |---|---|---|---|
 * | `rust` | `//` | `/* *\/`（不认嵌套） | `"…"` · 原始串 `r#"…"#` · 字符字面量 `'x'` / `'\n'`（生命周期 `'a` 不算） |
 * | `ts` | `//` | `/* *\/` | `"…"` · `'…'` · 模板串 `` `…` ``（整段，不回到 `${…}` 里解析）· 正则字面量 `/…/flags` |
 * | `kotlin` | `//` | `/* *\/`（认嵌套） | `"…"` · 原始串 `"""…"""` · 字符字面量 `'x'` |
 * | `css` | 无（`//` 在 CSS 里不是注释） | `/* *\/` | `"…"` · `'…'` |
 *
 * 刻意的取舍：
 * 1. Rust 块注释不认嵌套（第一个 `*\/` 收尾）：嵌套注释会欠剥（留下 `*\/` 当代码），方向是假红、会叫。
 * 2. TS 模板串整体当不透明字符串：`` `${await invoke("x")}` `` 里的命令名扫不到。
 * 3. TS 正则字面量按「`/` 前面那个有意义的字符」认：前面是标识符 · 数字 · `)` · `]` ⇒ 除号；
 *    前面是运算符 · 开括号 · 逗号 · 行首 · `return` 这类关键字 ⇒ 正则开头。正则不跨行，碰到换行就当除号退回。
 * 4. Rust 的 `'` 只有 `'<单个字符或一个转义>'`（右引号紧跟）才算字符字面量：`&'static` · `'a>` 是生命周期，不当定界符
 *    （当了就会从生命周期一路吃到下一个 `'`）。字符字面量 `'"'` 必须先于普通字符串认，否则那个双引号会开一个幻影字符串。
 */

/** 源码方言。**必填**：给默认值会让调用方静默拿到错的方言（Rust 的 `'a` vs TS 的 `'…'`）。 */
export type SourceLang = "rust" | "ts" | "kotlin" | "css";

/** 前面是这几个词之一 ⇒ 后面的 `/` 是正则开头（TS）。 */
const REGEX_AFTER_WORDS = new Set(["return", "typeof", "case", "in", "of", "delete", "void", "throw", "new", "else", "do", "yield", "await", "instanceof"]);

/**
 * 去掉块注释与行注释，**逐字符保留行结构**（注释里的字符换成空格，换行原样留下）：行号与相邻关系逐行对齐。
 */
export function stripComments(src: string, lang: SourceLang): string {
  const out = src.split("");
  const n = src.length;

  /** 把 [from, to) 里的非换行字符换成空格——这就是「保留行结构」的实现。 */
  const blank = (from: number, to: number): void => {
    for (let k = from; k < to && k < n; k++) if (out[k] !== "\n") out[k] = " ";
  };

  const isIdent = (ch: string | undefined): boolean => ch !== undefined && /[A-Za-z0-9_$]/.test(ch);

  /** 跳过一个带 `\` 转义的定界字符串，返回结束位置（定界符之后）。 */
  const skipQuoted = (start: number, quote: string): number => {
    let k = start + 1;
    while (k < n) {
      if (src[k] === "\\") k += 2;
      else if (src[k] === quote) return k + 1;
      else k++;
    }
    return n; // 未闭合：吃到文件尾（源码本身有语法错，守卫会以别的方式叫）
  };

  /** TS：`i` 处的 `/` 是不是正则开头（看它前面那个有意义的字符或词）。 */
  const regexStartsAt = (i: number): boolean => {
    let k = i - 1;
    while (k >= 0 && (src[k] === " " || src[k] === "\t")) k--;
    if (k < 0 || src[k] === "\n" || src[k] === "\r") return true;
    const p = src[k];
    if (p === ")" || p === "]" || p === "}" || p === "'" || p === '"' || p === "`") return false;
    if (isIdent(p)) {
      let s = k;
      while (s > 0 && isIdent(src[s - 1])) s--;
      return REGEX_AFTER_WORDS.has(src.slice(s, k + 1));
    }
    return true; // 运算符 · 开括号 · 逗号 · 分号 …
  };

  /** TS：跳过从 `i` 起的正则字面量（含 flags）；不是一个完整的单行正则 ⇒ 返回 `-1`。 */
  const skipRegex = (i: number): number => {
    let k = i + 1;
    let inClass = false;
    while (k < n) {
      const ch = src[k];
      if (ch === "\n") return -1;
      if (ch === "\\") k += 2;
      else if (inClass) {
        if (ch === "]") inClass = false;
        k++;
      } else if (ch === "[") {
        inClass = true;
        k++;
      } else if (ch === "/") {
        k++;
        while (k < n && /[a-z]/.test(src[k])) k++;
        return k;
      } else k++;
    }
    return -1;
  };

  let i = 0;
  while (i < n) {
    const c = src[i];

    // ---- 行注释（CSS 没有）----
    if (lang !== "css" && c === "/" && src[i + 1] === "/") {
      const nl = src.indexOf("\n", i);
      const end = nl < 0 ? n : nl;
      blank(i, end);
      i = end;
      continue;
    }

    // ---- 块注释（Kotlin 认嵌套，别的不认）----
    if (c === "/" && src[i + 1] === "*") {
      let end: number;
      if (lang === "kotlin") {
        let depth = 1;
        let k = i + 2;
        while (k < n && depth > 0) {
          if (src[k] === "/" && src[k + 1] === "*") {
            depth++;
            k += 2;
          } else if (src[k] === "*" && src[k + 1] === "/") {
            depth--;
            k += 2;
          } else k++;
        }
        end = k;
      } else {
        const close = src.indexOf("*/", i + 2);
        end = close < 0 ? n : close + 2;
      }
      blank(i, end);
      i = end;
      continue;
    }

    // ---- TS 正则字面量 ----
    if (lang === "ts" && c === "/" && regexStartsAt(i)) {
      const end = skipRegex(i);
      if (end > 0) {
        i = end;
        continue;
      }
    }

    // ---- Rust 原始字符串 r"…" / r#"…"# / br##"…"## ----
    if (lang === "rust" && (c === "r" || c === "b") && !isIdent(src[i - 1])) {
      const m = /^(?:br|rb|r|b)(#*)"/.exec(src.slice(i, i + 12));
      if (m && (m[1].length > 0 || m[0].startsWith("r") || m[0].startsWith("br"))) {
        const terminator = `"${m[1]}`;
        const close = src.indexOf(terminator, i + m[0].length);
        i = close < 0 ? n : close + terminator.length;
        continue;
      }
    }

    // ---- Kotlin 原始字符串 """…""" ----
    if (lang === "kotlin" && src.startsWith('"""', i)) {
      const close = src.indexOf('"""', i + 3);
      i = close < 0 ? n : close + 3;
      while (i < n && src[i] === '"') i++; // `""""` 收尾时多出来的引号属于串
      continue;
    }

    // ---- 字符字面量（Rust · Kotlin）：必须先于普通字符串认 ----
    if ((lang === "rust" || lang === "kotlin") && c === "'") {
      const m = /^'(?:\\(?:x[0-9A-Fa-f]{2}|u\{[0-9A-Fa-f]{1,6}\}|u[0-9A-Fa-f]{4}|[\s\S])|[^\\'])'/.exec(src.slice(i, i + 12));
      if (m) {
        i += m[0].length;
        continue;
      }
      // Rust：不是字符字面量 ⇒ 是生命周期，当普通字符往下走（**绝不**当定界符）。
    }

    // ---- 普通字符串 ----
    if (c === '"') {
      i = skipQuoted(i, '"');
      continue;
    }
    // TS 的 '…' 与 `…`、CSS 的 '…'。Rust · Kotlin 的 ' 上面已认（字符字面量）。
    if ((lang === "ts" && (c === "'" || c === "`")) || (lang === "css" && c === "'")) {
      i = skipQuoted(i, c);
      continue;
    }

    i++;
  }

  return out.join("");
}
