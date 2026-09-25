/**
 * 对外文案的**唯一取文口**（`调研/设计/91 §5.1`：「要引入一层文案表。把文本都抽出来解耦」）。
 *
 * **单一来源**：`src/shared/copy/table.json`。前端经这里 `import`；Rust 侧 `include_str!`
 * 同一份文件（`91 §5.1.1` 决定 2：一份文件两侧各读，不是两份表加一条对拍）——〔DP1 · 第四波〕Rust 读口
 * `src/bridge/src/copy_table.rs::copy_text` 已落地（第一批调用点是部署后端的几句拒绝），全量抽表仍在最后一波。
 *
 * 用法：`copyText("panePreview.head.title", { origin, target })`。
 * - key 必须是**字面量**：`tests/copy/copy-table.vitest.ts` 靠静态读调用点来做「表 ↔ 引用」两向相等，
 *   算出来的 key 它看不见 ⇒ 那条判据会直接报红，不会放过。
 * - 参数只递**命名**参数，与表里 `args` 相等；不在调用方拼接任何文字（`91 §5.1.1` 决定 3）。
 */
import TABLE from "./shared/copy/table.json";

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
 */
export function copyText(key: CopyKey, args: CopyArgs = {}): string {
  const e = ENTRIES[key];
  if (!e) throw new Error(`copyText: no entry "${key}"`);
  const given = Object.keys(args).sort().join(",");
  const want = [...e.args].sort().join(",");
  if (given !== want) throw new Error(`copyText("${key}"): wants args [${want}], got [${given}]`);
  return e.zh.replace(/\{([A-Za-z][A-Za-z0-9]*)\}/g, (_m, name: string) => String(args[name]));
}
