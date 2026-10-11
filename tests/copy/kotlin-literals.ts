/**
 * 手机端生产 Kotlin 里的**含汉字的字符串字面量**：逐个读出来，连同它落在哪个调用里（`Log.w(…)` · `testTag(…)` · `require(…) { … }`）。
 * 读者：`mobile-literals.vitest.ts`（手机生产源码不许写死界面中文）。按词法读，不是编译器解析（同 `kotlin-refs.ts`）。
 *
 * 认得：`"…"`（含 `\` 转义）· `"""…"""` 原样串 · 字符串模板 `$x` / `${…}`（模板里的表达式也照样往里读，里头再嵌的字面量单独算一条）·
 * `//` 行注释 · `/* … *\/` 块注释（Kotlin 许嵌套）· 字符字面 `'x'`。注释里的字一概不算。
 * 字面量的文本里，模板那一截记成 `{…}`（登记例外按这个形状对，行号挪了不影响）。
 */

const HAN = /[㐀-䶿一-鿿豈-﫿]/;

export interface KtLiteral {
  /** 1 起的行号。 */
  line: number;
  /** 字面量文本；模板那一截记 `{…}`。 */
  text: string;
  /** 它直接落在哪个调用里：调用名（`Log.w` · `testTag` · `require` …）；不在调用里 ⇒ ""。 */
  call: string;
}

/** 一份 `.kt` 里全部含汉字的字面量。 */
export function hanLiteralsIn(src: string): KtLiteral[] {
  const out: { at: number; text: string }[] = [];
  const n = src.length;

  /** `i` 指在开头引号之后；返回 [结束位置（收尾引号之后）, 文本]。 */
  const readString = (start: number, raw: boolean): [number, string] => {
    let i = start;
    let buf = "";
    while (i < n) {
      if (raw && src.startsWith('"""', i)) {
        let j = i + 3;
        while (j < n && src[j] === '"') j++; // `""""` 结尾：多出来的引号属于串
        return [j, buf + src.slice(i, j - 3)];
      }
      const c = src[i];
      if (!raw && c === '"') return [i + 1, buf];
      if (!raw && c === "\\") {
        buf += src.slice(i, i + 2);
        i += 2;
        continue;
      }
      if (c === "$" && src[i + 1] === "{") {
        let depth = 0;
        let j = i + 1;
        while (j < n) {
          const d = src[j];
          if (d === "{") depth++;
          else if (d === "}") {
            depth--;
            if (depth === 0) break;
          } else if (d === '"') {
            const r = src.startsWith('"""', j);
            const s0 = j;
            const [end, s] = readString(j + (r ? 3 : 1), r);
            out.push({ at: s0, text: s });
            j = end;
            continue;
          }
          j++;
        }
        buf += "{…}";
        i = j + 1;
        continue;
      }
      if (c === "$" && /[A-Za-z_]/.test(src[i + 1] ?? "")) {
        let j = i + 1;
        while (j < n && /[A-Za-z0-9_]/.test(src[j])) j++;
        buf += "{…}";
        i = j;
        continue;
      }
      buf += c;
      i++;
    }
    return [i, buf];
  };

  let i = 0;
  while (i < n) {
    if (src.startsWith("//", i)) {
      const e = src.indexOf("\n", i);
      i = e < 0 ? n : e;
      continue;
    }
    if (src.startsWith("/*", i)) {
      let depth = 1;
      let j = i + 2;
      while (j < n && depth > 0) {
        if (src.startsWith("/*", j)) {
          depth++;
          j += 2;
        } else if (src.startsWith("*/", j)) {
          depth--;
          j += 2;
        } else j++;
      }
      i = j;
      continue;
    }
    if (src[i] === "'") {
      const j = src[i + 1] === "\\" ? i + 3 : i + 2;
      if (src[j] === "'") {
        i = j + 1;
        continue;
      }
    }
    if (src[i] === '"') {
      const raw = src.startsWith('"""', i);
      const [end, s] = readString(i + (raw ? 3 : 1), raw);
      out.push({ at: i, text: s });
      i = end;
      continue;
    }
    i++;
  }

  const masked = maskStringsAndComments(src);
  return out
    .filter((x) => HAN.test(x.text))
    .sort((a, b) => a.at - b.at)
    .map((x) => ({ line: src.slice(0, x.at).split("\n").length, text: x.text, call: enclosingCall(masked, x.at) }));
}

/** 注释与字面量的内容都换成空格（括号配对时不被串里的括号带偏）；引号本身留着。 */
function maskStringsAndComments(src: string): string {
  const chars = src.split("");
  let i = 0;
  const n = src.length;
  const blank = (a: number, b: number): void => {
    for (let k = a; k < b; k++) if (chars[k] !== "\n") chars[k] = " ";
  };
  while (i < n) {
    if (src.startsWith("//", i)) {
      const e = src.indexOf("\n", i);
      const end = e < 0 ? n : e;
      blank(i, end);
      i = end;
      continue;
    }
    if (src.startsWith("/*", i)) {
      let depth = 1;
      let j = i + 2;
      while (j < n && depth > 0) {
        if (src.startsWith("/*", j)) {
          depth++;
          j += 2;
        } else if (src.startsWith("*/", j)) {
          depth--;
          j += 2;
        } else j++;
      }
      blank(i, j);
      i = j;
      continue;
    }
    if (src[i] === '"') {
      // 模板表达式里的括号对外层配对无影响（自成一对），整段串连模板一起抹掉。
      const raw = src.startsWith('"""', i);
      let j = i + (raw ? 3 : 1);
      let depth = 0;
      while (j < n) {
        if (depth === 0 && raw && src.startsWith('"""', j)) {
          j += 3;
          while (j < n && src[j] === '"') j++;
          break;
        }
        if (depth === 0 && !raw && src[j] === '"') {
          j++;
          break;
        }
        if (!raw && depth === 0 && src[j] === "\\") {
          j += 2;
          continue;
        }
        if (src[j] === "$" && src[j + 1] === "{" && depth === 0) {
          depth = 1;
          j += 2;
          continue;
        }
        if (depth > 0) {
          if (src[j] === "{") depth++;
          else if (src[j] === "}") depth--;
          else if (src[j] === '"') {
            // 模板里再嵌的串：跳到它收尾（不再往深里认模板，够用）
            const r = src.startsWith('"""', j);
            const close = r ? '"""' : '"';
            let k = j + close.length;
            while (k < n && !src.startsWith(close, k)) k += src[k] === "\\" && !r ? 2 : 1;
            j = k + close.length;
            continue;
          }
        }
        j++;
      }
      blank(i + 1, j - 1);
      i = j;
      continue;
    }
    i++;
  }
  return chars.join("");
}

/** 位置 `at` 直接落在哪个调用里：往回找最近一个没闭合的 `(` / `{`，取它前面的调用名（尾随 lambda 先跨过 `(…)`）。 */
function enclosingCall(masked: string, at: number): string {
  let depth = 0;
  for (let k = at - 1; k >= 0; k--) {
    const c = masked[k];
    if (c === ")" || c === "}" || c === "]") depth++;
    else if (c === "(" || c === "{" || c === "[") {
      if (depth > 0) {
        depth--;
        continue;
      }
      if (c === "[") return "";
      let j = k - 1;
      while (j >= 0 && /\s/.test(masked[j])) j--;
      if (c === "{" && masked[j] === ")") {
        // 尾随 lambda：`require(cond) { "…" }` ⇒ 跨过 `(cond)`
        let d = 0;
        for (; j >= 0; j--) {
          if (masked[j] === ")") d++;
          else if (masked[j] === "(") {
            d--;
            if (d === 0) break;
          }
        }
        j--;
      }
      const m = /([A-Za-z_][\w.]*)\s*$/.exec(masked.slice(Math.max(0, j - 80), j + 1));
      return m ? m[1] : "";
    }
  }
  return "";
}
