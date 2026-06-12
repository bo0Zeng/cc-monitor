/**
 * api-error.ts 纯逻辑（describeRetryError 双 shape 解析）断言脚本。issue #21。
 *
 * 跑法：`node src/cards/api-error.test.ts` 或 `npm run test:api-error`。
 * 同 diff.test.ts / branching.test.ts：零 node 依赖、失败 throw 非零退出作
 * pre-push 门禁；tsc --noEmit 自动类型检查本文件。
 *
 * 为什么值得锁：error 对象 shape 随 CLI 版本漂移（旧 ≤v2.1.150 嵌套
 * error.error.message；新 ≥v2.1.156 有现成 formatted），且工程审计实测
 * "serde null 穿过 undefined 判定" 这类 bug 恰好是这种纯函数测试能抓的。
 */

import { describeRetryError } from "./api-error.ts";

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

console.log("api-error.test.ts");

// 新 shape（CLI ≥v2.1.156）：formatted 现成文案优先
test("new shape: formatted wins", () => {
  eq(describeRetryError({ formatted: "529 Overloaded", status: 529 }), "529 Overloaded");
});

// 新 shape 连接错误：无 status、connection 非空
test("new shape: connection error (no status)", () => {
  eq(
    describeRetryError({ connection: { code: "ECONNRESET", message: "socket hang up" }, isNetworkDown: false }),
    "ECONNRESET socket hang up",
  );
});

// 旧 shape（≤v2.1.150）：status + 嵌套 error.error.message
test("old shape: status + nested message", () => {
  eq(
    describeRetryError({ status: 529, error: { error: { message: "Overloaded" } } }),
    "529 Overloaded",
  );
});

// 全缺 / null / 非对象 → 通用降级文案，绝不 throw
test("degenerate inputs fall back gracefully", () => {
  const fallback = "网络/服务异常";
  eq(describeRetryError(null), fallback, "null");
  eq(describeRetryError(undefined), fallback, "undefined");
  eq(describeRetryError("oops"), fallback, "string");
  eq(describeRetryError({}), fallback, "empty object");
  eq(describeRetryError({ formatted: "", connection: null, error: null }), fallback, "all empty");
});

// 字段类型漂移：formatted 非 string / connection 字段非 string → 不炸、降级
test("type-drifted fields do not throw", () => {
  eq(describeRetryError({ formatted: 42 }), "网络/服务异常");
  eq(describeRetryError({ connection: { code: 7, message: null } }), "网络/服务异常");
  eq(describeRetryError({ status: "529" }), "网络/服务异常", "status 非 number 不入串");
});

if (failed > 0) {
  console.error(`\n${failed} api-error test(s) failed`);
  throw new Error(`api-error.test.ts: ${failed} failed`);
}
console.log("\nall api-error tests passed");
