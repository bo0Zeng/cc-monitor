/**
 * 〔CP2b · 全量抽表〕一份源码**经文案表说的那些话**：它里头每个 `copyText("key", …)` 取的表条目原文（`zh`，占位符原样）。
 *
 * 为什么要有它：抽表之前，好几条判据是「从某个文件的字符串字面量里抠出一句话再判它」
 * （「不指定账号」那句的真伪 · tab 层零命中 · 成功 toast 只住一个文件 …）。抽表之后那句话住
 * `src/shared/copy/table.json`，文件里只剩 `copyText("key")` —— 只读字面量的判据会对着空集零命中地绿。
 * ⇒ 这些判据的人群改成「字面量 ＋ 本函数」。key 表里没有 ⇒ 回一个显眼的占位（`copy-table.vitest.ts` 在上游拦它）。
 */
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import { REPO_ROOT } from "./repo-root.ts";

let table: Record<string, { zh: string }> | undefined;

export function copyTableTextsIn(src: string): string[] {
  table ??= (
    JSON.parse(readFileSync(resolve(REPO_ROOT, "src", "shared", "copy", "table.json"), "utf8")) as {
      entries: Record<string, { zh: string }>;
    }
  ).entries;
  const t = table;
  return [...src.matchAll(/\bcopyText\(\s*"([^"]+)"/g)].map((m) => t[m[1]]?.zh ?? `〔表里没有 ${m[1]}〕`);
}
