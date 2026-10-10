/**
 * 手机端生产 Kotlin 里的取文调用点（`copyText("…", "名" to 值, …)`）：按调用形状读，不是编译器解析（同 `rust-refs.ts`）。
 * 读者：文案表 ↔ 代码两向相等（`copy-table.vitest.ts`）。手机的取文口住 `KT_HOME`，从同一张 `table.json` 构建时生成条目。
 */
import { productionKtFiles } from "../test-support/production-sources.ts";
import type { Ref } from "./copy-support.ts";

/** Kotlin 取文口自己住的文件：它里头的 `copyText` 是定义，不是引用。 */
export const KT_HOME = "src/mobile/core-ui/src/main/kotlin/com/ccmonitor/mobile/core/ui/copy/CopyText.kt";
const KT_FN = "copyText";

/** 剥掉 `/* … *\/` 块注释（含 KDoc）与 `//` 行注释（字符串里的不算）。 */
function stripKotlinComments(text: string): string {
  let out = "";
  let inStr = false;
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (inStr) {
      out += c;
      if (c === "\\") {
        out += text[i + 1] ?? "";
        i++;
      } else if (c === '"') inStr = false;
      continue;
    }
    if (c === '"') {
      inStr = true;
      out += c;
      continue;
    }
    if (c === "/" && text[i + 1] === "*") {
      const end = text.indexOf("*/", i + 2);
      const cut = end < 0 ? text.slice(i) : text.slice(i, end + 2);
      out += cut.replace(/[^\n]/g, " ");
      i += cut.length - 1;
      continue;
    }
    if (c === "/" && text[i + 1] === "/") {
      const end = text.indexOf("\n", i);
      i = (end < 0 ? text.length : end) - 1;
      continue;
    }
    out += c;
  }
  return out;
}

/** 读一份 `.kt` 里的全部 `copyText` 调用点。 */
export function kotlinRefsIn(file: string, text: string): { refs: Ref[]; problems: string[] } {
  const refs: Ref[] = [];
  const problems: string[] = [];
  const code = stripKotlinComments(text);
  const lineOf = (at: number): number => code.slice(0, at).split("\n").length;
  for (const m of code.matchAll(new RegExp(`\\b${KT_FN}\\b`, "g"))) {
    const at = m.index ?? 0;
    const before = code.slice(Math.max(0, at - 4), at);
    const after = code.slice(at + KT_FN.length);
    if (before === "fun ") continue;
    if (/^\s*import\s+[\w.]*$/.test(code.slice(code.lastIndexOf("\n", at) + 1, at))) {
      if (/^\s*(as\b)/.test(after)) problems.push(`${file}:${lineOf(at)}：${KT_FN} 被改名导入`);
      continue;
    }
    if (before.endsWith("::")) {
      problems.push(`${file}:${lineOf(at)}：${KT_FN} 被当值用了（不是直接调用）`);
      continue;
    }
    if (!after.startsWith("(")) {
      problems.push(`${file}:${lineOf(at)}：${KT_FN} 后面不是调用`);
      continue;
    }
    const km = /^\(\s*"([^"\\$]*)"\s*(,|\))/.exec(after);
    if (!km) {
      problems.push(`${file}:${lineOf(at)}：${KT_FN} 的 key 不是字面量`);
      continue;
    }
    const args: string[] = [];
    if (km[2] === ",") {
      const rest = after.slice(km[0].length);
      // 取到与开头那个 `(` 配对的 `)`（km 已吃掉了它）。
      let depth = 1;
      let end = -1;
      for (let i = 0; i < rest.length; i++) {
        if (rest[i] === "(" || rest[i] === "[") depth++;
        if (rest[i] === ")" || rest[i] === "]") {
          depth--;
          if (depth === 0) {
            end = i;
            break;
          }
        }
      }
      const body = rest.slice(0, end);
      let d = 0;
      let start = 0;
      for (let i = 0; i <= body.length; i++) {
        const c = body[i];
        if (c === "(" || c === "[") d++;
        if (c === ")" || c === "]") d--;
        if ((c === "," && d === 0) || i === body.length) {
          const item = body.slice(start, i).trim();
          start = i + 1;
          if (!item) continue;
          const nm = /^"([A-Za-z][A-Za-z0-9]*)"\s+to\s+[\s\S]+$/.exec(item);
          if (nm) args.push(nm[1]);
          else problems.push(`${file}:${lineOf(at)}：${KT_FN} 的参数项不是 "名" to 值：${item}`);
        }
      }
    }
    refs.push({ file: `${file}:${lineOf(at)}`, key: km[1], args });
  }
  return { refs, problems };
}

/** 手机端生产 Kotlin 的调用点：文件 · 调用点 · 读不了的写法。取文口的定义那一份进文件列表、不进引用人群。 */
export function kotlinRefsOfTree(): { files: { file: string; text: string }[]; refs: Ref[]; problems: string[] } {
  const files = productionKtFiles("src/mobile");
  const all = files
    .filter((f) => f.file !== KT_HOME)
    .filter((f) => new RegExp(`\\b${KT_FN}\\b`).test(f.text))
    .map((f) => kotlinRefsIn(f.file, f.text));
  return { files, refs: all.flatMap((x) => x.refs), problems: all.flatMap((x) => x.problems) };
}
