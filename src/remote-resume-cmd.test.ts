/**
 * remote-resume-cmd.ts 纯逻辑断言脚本。issue F09。
 * 跑法：`node src/remote-resume-cmd.test.ts` 或 `npm run test:remote-resume`。
 * 同 api-error.test.ts：零 node 依赖、失败 throw 非零退出。
 */

import { buildRemoteResumeCmd } from "./remote-resume-cmd.ts";

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

console.log("remote-resume-cmd.test.ts");

test("cwd 非空 → cd + resume", () => {
  eq(
    buildRemoteResumeCmd("abc-123", "/home/pi/proj"),
    'cd "/home/pi/proj" && claude --resume abc-123',
  );
});

test("cwd 空 → 仅 resume", () => {
  eq(buildRemoteResumeCmd("abc-123", ""), "claude --resume abc-123");
  eq(buildRemoteResumeCmd("abc-123", "   "), "claude --resume abc-123", "纯空白 trim 后为空");
});

test("cwd 含空格 → 引号包裹", () => {
  eq(
    buildRemoteResumeCmd("s1", "/home/pi/my proj"),
    'cd "/home/pi/my proj" && claude --resume s1',
  );
});

test("cwd 含双引号 → 转义", () => {
  eq(
    buildRemoteResumeCmd("s1", '/home/pi/a"b'),
    'cd "/home/pi/a\\"b" && claude --resume s1',
  );
});

if (failed > 0) {
  console.error(`\n${failed} remote-resume test(s) failed`);
  throw new Error(`remote-resume-cmd.test.ts: ${failed} failed`);
}
console.log("\nall remote-resume tests passed");
