/**
 * format.ts 纯函数断言脚本：formatBytes / formatTimestampSmart / formatTimestampShort。
 *
 * 跑法：`node src/format.test.ts` 或 `npm run test:format`。
 * 同 remote-health.test.ts：零 node 依赖、失败 throw 非零退出作 pre-push 门禁；tsc --noEmit 类型检查。
 *
 * 为什么值得锁：format.ts 是 formatTime/formatBytes 各两份漂移后的收口点（见文件顶部注释）——
 * 有漂移史更该钉契约。formatBytes 全确定性、零依赖；时间分支用相对偏移构造、断言不依赖时区/locale。
 */

import {
  formatBytes,
  formatTimestampSmart,
  formatTimestampShort,
} from "./format.ts";

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
function ok(cond: boolean, msg?: string): void {
  if (!cond) throw new Error(msg ?? "expected truthy");
}

console.log("format.test.ts");

// === formatBytes（全确定性边界）===
test("formatBytes: B 段（< 1024）", () => {
  eq(formatBytes(0), "0 B");
  eq(formatBytes(1), "1 B");
  eq(formatBytes(1023), "1023 B");
});
test("formatBytes: KB 段（1 位小数）", () => {
  eq(formatBytes(1024), "1.0 KB");
  eq(formatBytes(1536), "1.5 KB");
  eq(formatBytes(1024 * 1024 - 1), "1024.0 KB", "上边界仍 KB");
});
test("formatBytes: MB 段（1 位小数）", () => {
  eq(formatBytes(1024 * 1024), "1.0 MB");
  eq(formatBytes(5 * 1024 * 1024), "5.0 MB");
});
test("formatBytes: GB 段（2 位小数）", () => {
  eq(formatBytes(1024 * 1024 * 1024), "1.00 GB");
  eq(formatBytes(Math.round(2.5 * 1024 * 1024 * 1024)), "2.50 GB");
});
test("formatBytes: 负数走 B 段（当前行为，无防护）", () => {
  eq(formatBytes(-5), "-5 B");
});

// === formatTimestampSmart（相对偏移构造，避开时区/locale 硬编码）===
test("formatTimestampSmart: 0 / NaN → —（!ms 守卫）", () => {
  eq(formatTimestampSmart(0), "—");
  eq(formatTimestampSmart(Number.NaN), "—");
});
test("formatTimestampSmart: 当天只显示时间（含 hh:mm，不含日期）", () => {
  const s = formatTimestampSmart(Date.now());
  ok(/\d{1,2}:\d{2}/.test(s), `当天结果应含时间，got ${JSON.stringify(s)}`);
});
test("formatTimestampSmart: 跨天比当天更长（多了日期段），且仍含时间", () => {
  const sameDay = formatTimestampSmart(Date.now());
  const crossDay = formatTimestampSmart(Date.now() - 40 * 86_400_000); // 40 天前
  ok(/\d{1,2}:\d{2}/.test(crossDay), "跨天结果应仍含时间");
  ok(crossDay.length > sameDay.length, `跨天应含日期段更长：crossDay=${JSON.stringify(crossDay)} sameDay=${JSON.stringify(sameDay)}`);
});

// === formatTimestampShort（解析失败回退原值）===
test("formatTimestampShort: 合法 ms → hh:mm", () => {
  ok(/\d{1,2}:\d{2}/.test(formatTimestampShort(Date.now())));
});
test("formatTimestampShort: 不可解析字符串 → 原样返回", () => {
  eq(formatTimestampShort("not-a-date"), "not-a-date");
});

if (failed > 0) {
  console.error(`\n${failed} format test(s) failed`);
  throw new Error(`format.test.ts: ${failed} failed`);
}
console.log("\nall format tests passed");
