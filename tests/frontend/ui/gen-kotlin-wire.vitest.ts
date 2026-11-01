/**
 * 手机线上类型的生成器（`tests/scripts/gen-kotlin-wire.mjs`）：一族（sealed interface）里各形是嵌套类，
 * 嵌套类的名在一族体内遮住同名的顶层类型与 Kotlin 标准名（`List` · `Map` · 别的生成类型）。
 * 判：一族体内对嵌套名的引用只许写成 `族.形`（或全名），不许裸写；集合 · 别的生成类型撞了名就写全名。
 * 夹具是测试里现写的几行 TS 声明，不是真导出；另跑一遍仓里真导出的那一份。
 */
import { describe, expect, it } from "vitest";
// @ts-expect-error 纯 JS 脚本，没有类型声明
import { declsFrom, kotlinTypes, loadDecls, rustFloatFields } from "../../scripts/gen-kotlin-wire.mjs";

/** 每一族体内裸写的嵌套名（声明那一处除外）：`族: 名 @ 那一行`。 */
function shadowed(kt: string): string[] {
  const out: string[] = [];
  for (const m of kt.matchAll(/^sealed interface (\w+)[^\n]*\{\n([\s\S]*?)^\}$/gm)) {
    const [, fam, body] = m;
    const clean = body.replace(/\/\*\*[\s\S]*?\*\//g, "").replace(/"(?:[^"\\]|\\.)*"/g, '""');
    const nested = [...clean.matchAll(/^ {4}(?:data class|class|data object) (\w+)/gm)].map((x) => x[1]);
    for (const n of nested) {
      for (const line of clean.split("\n")) {
        if (new RegExp(`(?<![\\w.])(?<!(?:class|object) )${n}\\b`).test(line)) out.push(`${fam}: ${n} @ ${line.trim()}`);
      }
    }
  }
  return out;
}

describe("一族体内不被嵌套名遮住", () => {
  it("形叫 List · Map · 同名生成类型 ⇒ 体内那几处写全名", () => {
    const decls = declsFrom([
      `export type Blk = { "t": "list", items: Array<Blk>, } | { "t": "map", m: Record<string, string>, } | { "t": "p", k: Array<Word>, };`,
      `export type Word = string | Node;`,
      `export type Node = { "t": "b", x: string, } | { "t": "i", x: string, };`,
    ]);
    const kt: string = kotlinTypes(decls, new Map());
    expect(shadowed(kt)).toEqual([]);
    expect(kt).toContain("kotlin.collections.List<");
    expect(kt).toContain("kotlin.collections.Map<");
    expect(kt).toMatch(/val value: [\w.]+\.gen\.Node,/);
    expect(kt).toMatch(/\{ [\w.]+\.gen\.Node\.ADAPTER \}/);
  });

  it("没撞名的一族照旧写短名", () => {
    const decls = declsFrom([`export type Box = { "t": "one", xs: Array<string>, } | { "t": "two" };`]);
    const kt: string = kotlinTypes(decls, new Map());
    expect(kt).toContain("val xs: List<String>,");
    expect(kt).not.toContain("kotlin.collections.");
  });

  it("形名撞了一族骨架要用的名（String · Ordered …）⇒ 生成不了，报出是哪一族哪一形", () => {
    const decls = declsFrom([`export type Bad = { "t": "string", x: string, } | { "t": "b" };`]);
    expect(() => kotlinTypes(decls, new Map())).toThrow(/Bad\.String/);
  });

  it("仓里真导出的那一份：每一族体内都没有裸写的嵌套名", () => {
    expect(shadowed(kotlinTypes(loadDecls(), rustFloatFields()))).toEqual([]);
  });
});
