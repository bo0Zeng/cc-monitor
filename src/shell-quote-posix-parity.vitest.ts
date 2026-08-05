/**
 * G1（Phase G 变异抽样 B5 的产物）：**TS 的 `posixQuote` 与 Rust 的
 * `shell_quote_core::posix_quote` 逐例同口径** —— 而且断言**从 Rust 源码里抽**，不手抄。
 *
 * # 为什么必须有
 *
 * Phase G 的分层变异抽样里，同一个变异（「空串直接短路返回空串」）
 * **Rust 侧被杀、TS 侧存活**：
 *
 * ```
 * B4 Rust  posix_quote:  if s.is_empty() { return String::new(); }   → 被杀
 * B5 TS    posixQuote:   if (s === "") return "";                    → **存活**
 * ```
 *
 * 原因很简单：`shell-quote-core` 有 `posix_quote_breaks_single_quotes_the_posix_way`，
 * 而 TS 这一份**一个测试都没有**（仓里唯一提到 `shell-quote` 的对拍是
 * `shell-quote-deceptive-parity.vitest.ts`，它比的是 `isValidConfigDir` 的**欺骗字符表**，
 * 与引号规则无关）。
 *
 * ⚠ 空串这一格不是学术问题：`posixQuote("")` 要产 `''`。若退化成空串，
 * 拼出来的命令行会**少一个参数**（`cmd '' x` 变成 `cmd  x`）——
 * 这类错在渲染层是静默的，只有在被执行的那个 shell 里才现形。
 *
 * # 它怎么做的（照 `shell-quote-deceptive-parity.vitest.ts` 的先例）
 *
 * 从 `shell-quote-core/src/lib.rs` 的测试段里解析出每一条
 * `assert_eq!(posix_quote(<入>), <出>)`，把**同样的入**喂给 TS 的 `posixQuote`，
 * 要求**逐字节相同的出**。⇒ 两侧不许分家，而且 Rust 那边加一条新用例，
 * TS 这边**自动**跟着受约束 —— 不需要有人记得同步。
 *
 * ⚠ 抽取器自检在下面：抽到的用例数必须 ≥3（今天恰好 3），
 * 且**必须包含空串那一格**（那正是 B5 活下来的地方）。零命中地绿要防死。
 */
import { describe, expect, test } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { posixQuote } from "./shell-quote.ts";

/** 把 Rust 字符串字面量里的转义还原成真实字符（只需处理本文件用到的 `\\'` / `\\\\`）。 */
function unescapeRust(lit: string): string {
  return lit.replace(/\\(.)/g, (_, c: string) => (c === "n" ? "\n" : c === "t" ? "\t" : c));
}

function rustCases(): Array<{ input: string; expected: string }> {
  const src = readFileSync(
    resolve(__dirname, "../src-tauri/crates/shell-quote-core/src/lib.rs"),
    "utf-8",
  );
  const out: Array<{ input: string; expected: string }> = [];
  // assert_eq!(posix_quote("<入>"), "<出>");
  const re = /assert_eq!\(\s*posix_quote\("((?:[^"\\]|\\.)*)"\)\s*,\s*"((?:[^"\\]|\\.)*)"\s*\)/g;
  for (const m of src.matchAll(re)) {
    out.push({ input: unescapeRust(m[1]), expected: unescapeRust(m[2]) });
  }
  return out;
}

describe("posixQuote 与 Rust 侧逐例同口径（断言从 Rust 源码抽，不手抄）", () => {
  const cases = rustCases();

  test("抽取器自检：真的抽到了 Rust 那边的用例", () => {
    expect(cases.length).toBeGreaterThanOrEqual(3);
    // ★ 必须抽到空串那一格 —— 它正是变异 B5 活下来的地方。
    //   抽取器若把它漏了，本文件会在**恰好那一格**上零命中地绿。
    expect(cases.some((c) => c.input === "")).toBe(true);
  });

  // ⚠ 用 `$input`/`$expected` 而不是 `%j`：`test.each` 收的是**对象**，位置格式串会渲染成 `undefined`。
  test.each(cases)("posixQuote($input) === $expected（Rust 侧同款）", ({ input, expected }) => {
    expect(posixQuote(input)).toBe(expected);
  });
});
