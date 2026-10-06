/**
 * api-error.ts 纯逻辑断言脚本（主窗口稿 §5.2.5）：原因一词按后端给的种类出（界面不读报错对象）。
 *
 * 跑法：`npm run test:api-error`（tsx，零 DOM）。失败 throw 非零退出。DOM 那几条（并条 · 结局）在 `main-window-behavior.vitest.ts`。
 */
import { reasonWord } from "../../../../src/frontend/ui/cards/api-error.ts";

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

test("六种原因各出一词、互不相同（后端给的种类，界面不读报错对象）", () => {
  const words = (["overloaded", "quota", "network", "auth", "context", "unknown"] as const).map((r) => reasonWord(r));
  eq(new Set(words).size, 6, "六个词互不相同");
  eq(words.join(" / "), "服务器过载 / 额度满 / 网络中断 / 需登录 / 上下文超长 / 原因不明");
});

test("后端没给原因（老后端）⇒ 原因不明，不猜", () => {
  eq(reasonWord(undefined), "原因不明");
});

if (failed > 0) throw new Error(`${failed} api-error test(s) failed`);
console.log("\nall passed");
