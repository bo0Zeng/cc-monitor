/**
 * pricing.ts 纯函数断言：contextLimit / normalizeModel / contextPercent。
 * 跑法：`node src/views/pricing.test.ts` 或 `npm run test:pricing`。
 */

import { contextLimit, normalizeModel, contextPercent } from "./pricing.ts";

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
  if (a !== b) throw new Error(`${msg ?? "eq"}: expected ${JSON.stringify(b)}, got ${JSON.stringify(a)}`);
}

console.log("pricing.test.ts");

test("contextLimit: [1m] 变体 → 1M（本项目模型）", () => {
  eq(contextLimit("claude-opus-4-8[1m]"), 1_000_000);
  eq(contextLimit("claude-sonnet-5[1M]"), 1_000_000); // 大小写不敏感
});
test("contextLimit: 标准 Claude 家族 → 200k", () => {
  eq(contextLimit("claude-opus-4-8"), 200_000);
  eq(contextLimit("claude-sonnet-5"), 200_000);
  eq(contextLimit("claude-haiku-4-5"), 200_000);
  eq(contextLimit("claude-3-5-sonnet-20241022"), 200_000);
});
test("contextLimit: 未知/空 → null（UI 显 ?）", () => {
  eq(contextLimit("gpt-4"), null);
  eq(contextLimit(""), null);
  eq(contextLimit(null), null);
  eq(contextLimit(undefined), null);
});

test("normalizeModel: 剥 [1m]/-fast/日期后缀", () => {
  eq(normalizeModel("claude-opus-4-8[1m]"), "claude-opus-4-8");
  eq(normalizeModel("claude-sonnet-5-fast"), "claude-sonnet-5");
  eq(normalizeModel("claude-3-5-sonnet-20241022"), "claude-3-5-sonnet");
  eq(normalizeModel(null), "unknown");
  eq(normalizeModel(""), "unknown");
});

test("contextPercent: input+cache ÷ 上限", () => {
  // 100k prompt / 200k = 50%
  eq(contextPercent("claude-opus-4-8", 100_000), 50);
  // 500k / 1M = 50%（[1m] 变体）
  eq(contextPercent("claude-opus-4-8[1m]", 500_000), 50);
  // 未知模型 → null
  eq(contextPercent("gpt-4", 100_000), null);
  eq(contextPercent(null, 100_000), null);
});

if (failed > 0) {
  console.error(`\n${failed} pricing test(s) failed`);
  throw new Error(`pricing.test.ts: ${failed} failed`);
}
console.log("\nall pricing tests passed");
