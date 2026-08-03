/**
 * S18 收口的判据：**TS 的 `isValidConfigDir` 与 Rust 的 `acct_core::is_deceptive_char`
 * 拒同一批码位** —— 而且是**行为对拍**，不是文本对拍。
 *
 * # 为什么必须有
 *
 * U7-3 把那张「视觉欺骗字符」表收进共享 crate 时，只给了两个**读 manifest** 的地方；
 * **拼命令那条路漏了**。U8c-1 把 Rust 的命令面接上并集，**TS 这边没跟** ——
 * 于是同一个含 `U+3000` 的 configDir「本机 Rust 拉起拒绝、远端 TS 拉起放行」。
 *
 * TS 没法 import 那个 Rust crate，所以这份表在 TS 侧只能是**手抄**。
 * 手抄就是下一个漂移源 ⇒ 必须有一条读 Rust 源码的对拍。
 *
 * # 它怎么做的
 *
 * 从 `acct-core/src/lib.rs` 的 `matches!` 里解析出全部码位（含 `..=` 区间），
 * 然后把**每一个**都真的塞进一条合法路径喂给 `isValidConfigDir`。
 * 不是比正则文本 —— 比的是「这个码位到底被不被拒」。
 */
import { describe, expect, test } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { isValidConfigDir } from "./shell-quote.ts";

/** 从 `is_deceptive_char` 的 `matches!` 体里解析出所有码位。 */
function rustDeceptiveCodePoints(): number[] {
  const src = readFileSync(
    resolve(__dirname, "..", "src-tauri/crates/acct-core/src/lib.rs"),
    "utf8",
  );
  const start = src.indexOf("pub fn is_deceptive_char");
  expect(start, "找不到 is_deceptive_char").toBeGreaterThan(0);
  const body = src.slice(start, src.indexOf("\n}", start));
  const out = new Set<number>();
  // 区间：'\u{2000}'..='\u{200A}'
  for (const m of body.matchAll(/'\\u\{([0-9A-Fa-f]+)\}'\s*\.\.=\s*'\\u\{([0-9A-Fa-f]+)\}'/g)) {
    for (let c = parseInt(m[1], 16); c <= parseInt(m[2], 16); c++) out.add(c);
  }
  // 去掉区间之后剩下的单点
  const singles = body.replace(/'\\u\{[0-9A-Fa-f]+\}'\s*\.\.=\s*'\\u\{[0-9A-Fa-f]+\}'/g, "");
  for (const m of singles.matchAll(/'\\u\{([0-9A-Fa-f]+)\}'/g)) out.add(parseInt(m[1], 16));
  return [...out].sort((a, b) => a - b);
}

describe("视觉欺骗字符：TS ↔ Rust 同一个集合（账本 S18）", () => {
  const points = rustDeceptiveCodePoints();

  // ★ 抽取器自检：解析不出来时，下面那条会零命中零失败地绿。
  test("真的从 Rust 源码解析出了码位（抽取器自检）", () => {
    // 地板 30 → 39（实测 39）。30 意味着「解析漏掉九个码位」不会红 ——
    // 而漏掉的那几个正是两侧会分家的地方（复盘 P3 棘的三条地板之一）。
    expect(points.length).toBeGreaterThanOrEqual(39);
    // 定点核几个：区间展开对不对、单点有没有漏。
    expect(points).toContain(0x00a0); // NBSP
    expect(points).toContain(0x2003); // 区间 2000..=200A 的中间一个
    expect(points).toContain(0x3000); // ideographic space
    expect(points).toContain(0xfeff); // BOM
  });

  test("Rust 拒的每一个码位，TS 也拒（S18 那条跨语言缝已收）", () => {
    const leaked = points
      .filter((c) => isValidConfigDir(`/home/u/.claude-accts/${String.fromCodePoint(c)}z`))
      .map((c) => `U+${c.toString(16).toUpperCase().padStart(4, "0")}`);
    expect(leaked).toEqual([]);
  });

  // 反向自检：正常路径必须**通过** —— 否则上面那条会被「什么都拒」冒充成功。
  test("合法路径照常放行（防「全拒」冒充成功）", () => {
    for (const ok of ["/home/u/.claude-accts/z", "/home/用户/带 空格/z", "/opt/a-b_c.d/z"]) {
      expect(isValidConfigDir(ok), ok).toBe(true);
    }
  });
});
