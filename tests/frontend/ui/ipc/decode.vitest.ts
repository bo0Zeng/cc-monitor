// 严格收的三个小件只住 `ipc/decode.ts` 一处：是不是对象 · 键恰好这几格 · 必有几格 ＋ 可缺几格。
import { describe, it, expect } from "vitest";
import { exactKeys, isObj, optionalKeys } from "../../../../src/frontend/ui/ipc/decode";
import { productionTsFiles, SCAN_TIMEOUT_MS } from "../../../test-support/production-sources.ts";

describe("ipc/decode", () => {
  it("isObj：只认普通对象", () => {
    expect([{}, { a: 1 }].map(isObj)).toEqual([true, true]);
    expect([null, [], "x", 1, undefined].map(isObj)).toEqual([false, false, false, false, false]);
  });
  it("exactKeys：多一格 / 缺一格都不收，顺序不论", () => {
    expect(exactKeys({ a: 1, b: 2 }, ["b", "a"])).toBe(true);
    expect(exactKeys({ a: 1 }, ["a", "b"])).toBe(false);
    expect(exactKeys({ a: 1, b: 2, c: 3 }, ["a", "b"])).toBe(false);
    expect(exactKeys({}, [])).toBe(true);
  });
  it("optionalKeys：必有的一格不缺，其余只许在可缺那几格里", () => {
    expect(optionalKeys({ a: 1 }, ["a"], ["b"])).toBe(true);
    expect(optionalKeys({ a: 1, b: 2 }, ["a"], ["b"])).toBe(true);
    expect(optionalKeys({ b: 2 }, ["a"], ["b"])).toBe(false);
    expect(optionalKeys({ a: 1, c: 3 }, ["a"], ["b"])).toBe(false);
  });
});

/** 别名配置那一份（`alias-reads.ts`）另一路正在改，合进主线之后再收进来；除它之外零处。 */
const PENDING = new Set(["src/frontend/ui/alias-reads.ts"]);

describe("严格收的小件只有一个家", { timeout: SCAN_TIMEOUT_MS }, () => {
  it("界面生产段里零处自己定义 isObj / 键集合比较（正控：decode.ts 里认得出）", () => {
    const own = /\b(const|function)\s+(isObj|sameKeys|exactKeys|exactly)\b/;
    const sortedKeys = /Object\.keys\([^)]*\)\.sort\(\)/;
    const hits: string[] = [];
    let control = false;
    for (const s of productionTsFiles("src/frontend/ui")) {
      const mine = own.test(s.text) || sortedKeys.test(s.text);
      if (s.file === "src/frontend/ui/ipc/decode.ts") control = mine;
      else if (mine && !PENDING.has(s.file)) hits.push(s.file);
    }
    expect(control, "正控：decode.ts 里没认出定义 —— 识别器坏了或住址搬了").toBe(true);
    expect(hits, "这几份又自己写了一份").toEqual([]);
  });
});
