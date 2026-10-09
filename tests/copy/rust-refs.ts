/**
 * 生产 Rust 源码里的取文调用点（`copy_text("…", &[("名", 值), …])` · `copy_static!("…")`）：按调用形状读，不是编译器解析。
 * 两个读者：文案表 ↔ 代码两向相等（`copy-table.vitest.ts`）· C-W18「原话不上句子」按值认原话型占位（`copy-rules.vitest.ts`）。
 */
import { productionRsFiles } from "../test-support/production-sources.ts";
import type { Ref } from "./copy-support.ts";

/**
 * 一个参数的值是不是下层原话：`&e.to_string()` · `&err.to_string()` · `&format!("{e}")` 一类（错误值直接变成字串）。
 * 按值认、不按占位名认 —— 占位名叫 `kind` 也照样是原话（审计二第 2 条：C-W18 只看名字时这一族绿着放过）。
 */
export function isRawValue(v: string): boolean {
  const t = v.trim();
  return /^&\s*(?:e|err|error)\.to_string\(\)$/.test(t) || /^&\s*format!\(\s*"\{(?:e|err|error)(?::\?)?\}"\s*\)$/.test(t);
}

/** Rust 取文口自己住的文件：它里头的 `copy_text` 是定义，不是引用。 */
export const RS_HOME = "src/frontend/shell/src/copy_table.rs";
const RS_FN = "copy_text";
/** 同一个取文口的 `&'static str` 形（`copy-core` 的宏）。 */
const RS_STATIC = "copy_static";
/**
 * 取文口的**定义**住的两份文件：`copy-core` 的实现，与 monitor 那一层转发（`copy_core::copy_text(key, args)`，
 * key 不是字面量 —— 它是转发，不是引用）。它们不进「引用」一侧。
 */
export const RS_DEFINITIONS = new Set([RS_HOME, "src/common/copy-core/src/lib.rs"]);

/** 剥掉 `//` 行注释（字符串里的 `//` 不算）。块注释本仓生产段不用来写代码，按行注释剥足够。 */
function stripRustLineComments(text: string): string {
  return text
    .split("\n")
    .map((line) => {
      let inStr = false;
      for (let i = 0; i < line.length; i++) {
        const c = line[i];
        if (c === "\\" && inStr) {
          i++;
          continue;
        }
        if (c === '"') inStr = !inStr;
        if (!inStr && c === "/" && line[i + 1] === "/") return line.slice(0, i);
      }
      return line;
    })
    .join("\n");
}

/** 读一份 `.rs` 里的全部 `copy_text` 调用点（按调用形状，见头注「不判什么」）。 */
export function rustRefsIn(file: string, text: string): { refs: Ref[]; problems: string[] } {
  const refs: Ref[] = [];
  const problems: string[] = [];
  const code = stripRustLineComments(text);
  const lineOf = (at: number): number => code.slice(0, at).split("\n").length;
  const re = new RegExp(`\\b${RS_FN}\\b`, "g");
  for (const m of code.matchAll(re)) {
    const at = m.index ?? 0;
    const before = code.slice(Math.max(0, at - 3), at);
    const after = code.slice(at + RS_FN.length);
    if (file === RS_HOME && before === "fn ") continue;
    // 别人的同名**方法**（`ui.ctx().copy_text(…)`，egui 的剪贴板）不是我们的取文口。
    if (before.endsWith(".")) continue;
    // `\buse`：原先裸 `use\s` 把「OursFault::Misuse => copy_text(…)」这一行当成了 `use` 导入 ⇒ 那个调用点
    //   从「引用」一侧漏掉、表里那条被报成死文案（现打逮到：filewin/source.rs 的 said.internal）。
    if (/^\s*[;,}]/.test(after) || /^::/.test(after) || /\buse\s[^;]*$/.test(code.slice(code.lastIndexOf("\n", at) + 1, at))) {
      if (/\buse\s[^;]*$/.test(code.slice(code.lastIndexOf("\n", at) + 1, at))) continue;
      problems.push(`${file}:${lineOf(at)}：${RS_FN} 被当值用了（不是直接调用）`);
      continue;
    }
    if (!after.startsWith("(")) {
      problems.push(`${file}:${lineOf(at)}：${RS_FN} 后面不是调用`);
      continue;
    }
    const km = /^\(\s*"([^"\\]*)"\s*(,|\))/.exec(after);
    if (!km) {
      problems.push(`${file}:${lineOf(at)}：${RS_FN} 的 key 不是字面量`);
      continue;
    }
    const args: string[] = [];
    const raw: string[] = [];
    if (km[2] === ",") {
      const rest = after.slice(km[0].length);
      const am = /^\s*&\[/.exec(rest);
      if (!am) {
        problems.push(`${file}:${lineOf(at)}：${RS_FN} 的参数不是 &[…] 数组字面量`);
        continue;
      }
      // 取到与 `&[` 配对的 `]`。
      let depth = 0;
      let end = -1;
      for (let i = am[0].length - 1; i < rest.length; i++) {
        if (rest[i] === "[" || rest[i] === "(") depth++;
        if (rest[i] === "]" || rest[i] === ")") {
          depth--;
          if (depth === 0) {
            end = i;
            break;
          }
        }
      }
      const body = rest.slice(am[0].length, end);
      // 顶层每一项都必须是 `("名", …)`。
      let d = 0;
      let start = 0;
      const items: string[] = [];
      for (let i = 0; i <= body.length; i++) {
        const c = body[i];
        if (c === "(" || c === "[") d++;
        if (c === ")" || c === "]") d--;
        if ((c === "," && d === 0) || i === body.length) {
          const item = body.slice(start, i).trim();
          if (item) items.push(item);
          start = i + 1;
        }
      }
      for (const item of items) {
        const nm = /^\(\s*"([A-Za-z][A-Za-z0-9]*)"\s*,([\s\S]*)\)$/.exec(item);
        if (nm) {
          args.push(nm[1]);
          if (isRawValue(nm[2])) raw.push(nm[1]);
        } else problems.push(`${file}:${lineOf(at)}：${RS_FN} 的参数项不是 ("名", 值)：${item}`);
      }
    }
    refs.push({ file: `${file}:${lineOf(at)}`, key: km[1], args, ...(raw.length ? { raw } : {}) });
  }
  // `copy_static!("…")`：同一条文案给成 `&'static str`（后端几处类型刻意是 `&'static str`，
  //   见 `copy-core` 那个宏的头注）。没有参数；key 必须是紧跟的字符串字面量，别的写法一律报「绕过」。
  for (const m of code.matchAll(new RegExp(`\\b${RS_STATIC}!`, "g"))) {
    const at = m.index ?? 0;
    const km = /^\(\s*"([^"\\]*)"\s*\)/.exec(code.slice(at + RS_STATIC.length + 1));
    if (!km) {
      problems.push(`${file}:${lineOf(at)}：${RS_STATIC}! 的 key 不是字面量（或带了参数）`);
      continue;
    }
    refs.push({ file: `${file}:${lineOf(at)}`, key: km[1], args: [] });
  }
  return { refs, problems };
}

/** 扫的那几棵树（常驻后端 · 两个前端 · 子 crate · 通信层）：文件 · 调用点 · 读不了的写法。取文口的定义那两份不进人群。 */
export function rustRefsOfTree(): { files: { file: string; text: string }[]; refs: Ref[]; problems: string[] } {
  const files = [
    ...productionRsFiles("src/frontend/shell/src"),
    ...productionRsFiles("src/frontend/filewin/src"), // 文件窗口独立成包（它的取文口调用点从前住上一棵）
    ...productionRsFiles("src/backend"),
    ...productionRsFiles("src/common"),
    ...productionRsFiles("src/comms"), // 通信层那两个 crate（面 A · 面 B）
  ];
  const all = files
    .filter((f) => !RS_DEFINITIONS.has(f.file))
    .filter((f) => new RegExp(`\\b(?:${RS_FN}\\b|${RS_STATIC}!)`).test(f.text))
    .map((f) => rustRefsIn(f.file, f.text));
  return { files, refs: all.flatMap((x) => x.refs), problems: all.flatMap((x) => x.problems) };
}

/** 哪几条文案的哪几个占位喂的是原话（按值认）：key → 占位名。 */
export function rawFedArgs(refs: Ref[]): Map<string, Set<string>> {
  const out = new Map<string, Set<string>>();
  for (const r of refs) for (const a of r.raw ?? []) out.set(r.key, (out.get(r.key) ?? new Set()).add(a));
  return out;
}
