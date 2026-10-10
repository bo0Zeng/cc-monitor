/**
 * format.ts 纯函数断言脚本：sizeText（大小的那一个读口）。
 *
 * 跑法：`node tests/frontend/ui/format.test.ts` 或 `npm run test:format`。
 * 同 remote-health.test.ts：零 node 依赖、失败 throw 非零退出作 pre-push 门禁；tsc --noEmit 类型检查。
 *
 * 为什么值得锁：大小的写法从前有三份（界面 GB 两位 · 文件窗口 GB 一位带 TB · 卡片自己除 1024），收成这一个读口与核心一份。
 */

import { sizeText } from "../../../src/frontend/ui/format.ts";

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
function eq(actual: unknown, expected: unknown, msg?: string): void {
  if (actual !== expected) {
    throw new Error(`${msg ?? "eq"}: expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`);
  }
}

console.log("format.test.ts");

// === sizeText（全确定性边界；整份对拍在 tests/copy/size-text.vitest.ts，读金样 size-text.golden.json）===
test("sizeText: B 段（< 1024）", () => {
  eq(sizeText(0), "0 B");
  eq(sizeText(1), "1 B");
  eq(sizeText(1023), "1023 B");
});
test("sizeText: KB 段（1 位小数）", () => {
  eq(sizeText(1024), "1.0 KB");
  eq(sizeText(1536), "1.5 KB");
  eq(sizeText(1024 * 1024 - 1), "1.0 MB", "舍入到 1024.0 的进一档");
});
test("sizeText: MB · GB 段（都是 1 位小数）", () => {
  eq(sizeText(1024 * 1024), "1.0 MB");
  eq(sizeText(5 * 1024 * 1024), "5.0 MB");
  eq(sizeText(1024 * 1024 * 1024), "1.0 GB");
  eq(sizeText(Math.round(2.5 * 1024 * 1024 * 1024)), "2.5 GB");
});
test("sizeText: 负数当 0", () => {
  eq(sizeText(-5), "0 B");
});

if (failed > 0) {
  console.error(`\n${failed} format test(s) failed`);
  throw new Error(`format.test.ts: ${failed} failed`);
}
console.log("\nall format tests passed");
