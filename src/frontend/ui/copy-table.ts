/**
 * 对外文案的**唯一取文口**（`调研/设计/91 §5.1`：「要引入一层文案表。把文本都抽出来解耦」）。
 *
 * **单一来源**：`src/shared/copy/table.json`。前端经这里 `import`；Rust 侧 `include_str!`
 * 同一份文件（`91 §5.1.1` 决定 2：一份文件两侧各读，不是两份表加一条对拍）——〔DP1 · 第四波〕Rust 读口
 * `src/frontend/shell/src/copy_table.rs::copy_text` 已落地（第一批调用点是部署后端的几句拒绝），全量抽表仍在最后一波。
 *
 * 用法：`copyText("panePreview.head.title", { origin, target })`。
 * - key 必须是**字面量**：`tests/copy/copy-table.vitest.ts` 靠静态读调用点来做「表 ↔ 引用」两向相等，
 *   算出来的 key 它看不见 ⇒ 那条判据会直接报红，不会放过。
 * - 参数只递**命名**参数，与表里 `args` 相等；不在调用方拼接任何文字（`91 §5.1.1` 决定 3）。
 */
import TABLE from "../../shared/copy/table.json";

export type CopyKey = keyof typeof TABLE.entries;
export type CopyArgs = Record<string, string | number>;

interface Entry {
  kind: string;
  zh: string;
  args: string[];
}

const ENTRIES: Record<string, Entry> = TABLE.entries;

/**
 * 取一条对外文案，把 `{name}` 换成同名参数。参数缺了或多了都是调用方写错 ⇒ 直接抛。
 *
 * 两条抛错是**程序员错误**（与 Rust 的 `panic!` 同类，`copy-table.vitest.ts` 的静态对拍保证它们
 * 在生产里走不到），刻意写成英文：它们不是对外文案，不该进普查的对外全集。
 *
 * 〔P3 · `rules.json` C-L5〕值与相邻汉字之间的空格**随值定**（[`joinSeams`]）：模板里「{machine}上」与「{machine} 上」
 * 同一个意思，值是 `lx` 印「lx 上」、是「本机」印「本机上」。与 Rust `copy_core::copy_text` 同一套（金样 `copy-interpolation.golden.json`）。
 */
export function copyText(key: CopyKey, args: CopyArgs = {}): string {
  const e = ENTRIES[key];
  if (!e) throw new Error(`copyText: no entry "${key}"`);
  const given = Object.keys(args).sort().join(",");
  const want = [...e.args].sort().join(",");
  if (given !== want) throw new Error(`copyText("${key}"): wants args [${want}], got [${given}]`);
  const parts: string[] = [];
  let at = 0;
  for (const m of e.zh.matchAll(/\{([A-Za-z][A-Za-z0-9]*)\}/g)) {
    parts.push(e.zh.slice(at, m.index), String(args[m[1]]));
    at = m.index + m[0].length;
  }
  parts.push(e.zh.slice(at));
  return joinSeams(parts);
}

const HAN = /^[\u3400-\u4dbf\u4e00-\u9fff\uf900-\ufaff]$/;
const ASCII_VISIBLE = /^[!-~]$/;

/** 一道接缝两边的字：汉字挨着 ASCII 可见字符 ⇒ 要一个空格；汉字挨着汉字 ⇒ 不要；别的（全角标点 · 空白 · 其它文字）⇒ 不管。 */
function seamWants(a: string, b: string): "space" | "none" | null {
  const ha = HAN.test(a);
  const hb = HAN.test(b);
  if ((ha && ASCII_VISIBLE.test(b)) || (ASCII_VISIBLE.test(a) && hb)) return "space";
  return ha && hb ? "none" : null;
}

/**
 * 〔P3 · C-L5〕拼回一句：`parts` 是「字面 · 值 · 字面 · 值 … 字面」交替（字面可为空）。每个非空的值与它两边的**模板字**之间
 * 按 [`seamWants`] 补上或拿掉**一个**空格；模板在接缝上写了两个以上空格是排版，照留；两个值之间只隔空格的那一段不动。
 * Rust 那一份 `copy_core::join_seams` 同形（两侧各对插值金样）。
 */
function joinSeams(parts: readonly string[]): string {
  const lit = parts.map((p, i) => (i % 2 === 0 ? p : ""));
  for (let i = 1; i < parts.length; i += 2) {
    const v = parts[i];
    if (v === "") continue;
    const vs = [...v];
    // 左边：前一段字面的尾巴（恰一个空格 ＋ 前面一个非空白字，或直接一个非空白字）
    const l = [...lit[i - 1]];
    const lSpace = l.length >= 2 && l[l.length - 1] === " " && l[l.length - 2] !== " ";
    const lc = lSpace ? l[l.length - 2] : l[l.length - 1];
    if (lc !== undefined) {
      const w = seamWants(lc, vs[0]);
      if (w === "space" && !lSpace) lit[i - 1] += " ";
      if (w === "none" && lSpace) lit[i - 1] = l.slice(0, -1).join("");
    }
    // 右边：后一段字面的开头
    const r = [...lit[i + 1]];
    const rSpace = r.length >= 2 && r[0] === " " && r[1] !== " ";
    const rc = rSpace ? r[1] : r[0];
    if (rc !== undefined) {
      const w = seamWants(vs[vs.length - 1], rc);
      if (w === "space" && !rSpace) lit[i + 1] = " " + lit[i + 1];
      if (w === "none" && rSpace) lit[i + 1] = r.slice(1).join("");
    }
  }
  return parts.map((p, i) => (i % 2 === 0 ? lit[i] : p)).join("");
}
