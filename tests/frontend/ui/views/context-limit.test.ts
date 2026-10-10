/**
 * context-limit.ts 纯函数断言：normalizeModel / readContextLimits（上限、百分比、字都由后端定）。
 * 跑法：`node tests/frontend/ui/views/context-limit.test.ts` 或 `npm run test:context-limit`。
 */

import {
  normalizeModel,
  readContextLimits,
} from "../../../../src/frontend/ui/views/context-limit.ts";

let failed = 0;
function test(name: string, fn: () => void): void {
  try {
    fn();
    console.log(`  ✓ ${name}`);
  } catch (e) {
    failed++;
    console.error(`  ✗ ${name}\n      ${e instanceof Error ? e.message : String(e)}`);
  }
}
function eq(a: unknown, b: unknown, msg?: string): void {
  if (JSON.stringify(a) !== JSON.stringify(b)) throw new Error(`${msg ?? "eq"}: expected ${JSON.stringify(b)}, got ${JSON.stringify(a)}`);
}

console.log("context-limit.test.ts");

test("normalizeModel: 剥 [1m]/-fast/日期后缀", () => {
  eq(normalizeModel("claude-opus-4-8[1m]"), "claude-opus-4-8");
  eq(normalizeModel("claude-sonnet-5-fast"), "claude-sonnet-5");
  eq(normalizeModel("claude-3-5-sonnet-20241022"), "claude-3-5-sonnet");
  eq(normalizeModel(null), "unknown");
  eq(normalizeModel(""), "unknown");
});

test("readContextLimits: 只收正整数的那几行", () => {
  eq(readContextLimits({ haiku: 200000, bad: -1, x: "2", "": 5, y: 1.5 }), { haiku: 200000 });
  eq(readContextLimits([1]), {});
  eq(readContextLimits(null), {});
});

if (failed > 0) {
  console.error(`\n${failed} context-limit test(s) failed`);
  throw new Error(`context-limit.test.ts: ${failed} failed`);
}
console.log("\nall context-limit tests passed");
