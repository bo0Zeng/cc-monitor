/**
 * remote-health.ts 节流纯逻辑（shouldShowHealthToast）断言脚本。issue #32（SS-F）。
 *
 * 跑法：`node src/remote-health.test.ts` 或 `npm run test:remote-health`。
 * 同 api-error.test.ts：零 node 依赖、失败 throw 非零退出作 pre-push 门禁；
 * tsc --noEmit 自动类型检查本文件。
 *
 * 为什么值得锁：拥塞期一台远端会连发 overflow，节流键 (origin,kind) + 间隔判定是
 * 「不刷屏」的唯一防线；边界（首次必弹 / 恰好到点弹 / 间隔内压制）易错。
 */

import { shouldShowHealthToast } from "./remote-health-throttle.ts";

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

console.log("remote-health.test.ts");

test("first time (no prior) always shows", () => {
  eq(shouldShowHealthToast(undefined, 1000, 10_000), true);
});

test("within interval is suppressed", () => {
  eq(shouldShowHealthToast(1000, 5000, 10_000), false, "5s < 10s gap");
  eq(shouldShowHealthToast(1000, 10_999, 10_000), false, "just under interval");
});

test("exactly at interval shows", () => {
  eq(shouldShowHealthToast(1000, 11_000, 10_000), true, "now-last == interval");
});

test("past interval shows", () => {
  eq(shouldShowHealthToast(1000, 99_999, 10_000), true);
});

test("zero interval always shows", () => {
  eq(shouldShowHealthToast(1000, 1000, 0), true);
});

if (failed > 0) {
  console.error(`\n${failed} remote-health test(s) failed`);
  throw new Error(`remote-health.test.ts: ${failed} failed`);
}
console.log("\nall remote-health tests passed");
