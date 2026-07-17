/**
 * history-cache.ts 纯函数断言脚本：shouldRefetchRemote（TTL 门控）。
 *
 * 跑法：`node src/views/history-cache.test.ts` 或 `npm run test:history-cache`。
 * 同 format.test.ts：零 node 依赖、失败 throw 非零退出作门禁；tsc --noEmit 类型检查。
 *
 * 为什么值得锁：F76（#46）来源缓存的判定核心。TTL 边界 / null / 时钟回拨最易错——
 * 判错一侧就是「每次全量重连（没省）」或「过期不重连（陈旧）」，且宿主 history.ts 零测试。
 */

import { shouldRefetchRemote, HISTORY_REMOTE_TTL_MS } from "./history-cache.ts";

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

console.log("history-cache.test.ts");

const TTL = HISTORY_REMOTE_TTL_MS; // 30_000

// === 无缓存必抓 ===
test("null 缓存 → 必须抓", () => {
  eq(shouldRefetchRemote(null, 1_000_000, TTL), true);
});

// === TTL 内复用 ===
test("刚抓（now==loadedAt）→ 复用", () => {
  eq(shouldRefetchRemote({ loadedAt: 1_000_000 }, 1_000_000, TTL), false);
});
test("TTL 内（差 < ttl）→ 复用", () => {
  eq(shouldRefetchRemote({ loadedAt: 1_000_000 }, 1_000_000 + TTL - 1, TTL), false);
});
test("远端 fan-out 后 2 秒 reopen → 复用（不重连）", () => {
  eq(shouldRefetchRemote({ loadedAt: 5_000_000 }, 5_002_000, TTL), false);
});

// === 边界：恰好等于 TTL 视为过期 ===
test("恰好 ttl（差 == ttl）→ 重抓", () => {
  eq(shouldRefetchRemote({ loadedAt: 1_000_000 }, 1_000_000 + TTL, TTL), true);
});
test("超期（差 > ttl）→ 重抓", () => {
  eq(shouldRefetchRemote({ loadedAt: 1_000_000 }, 1_000_000 + TTL + 1, TTL), true);
});

// === 时钟回拨：保守复用，不无谓重连 ===
test("now < loadedAt（系统时间倒退）→ 差为负 < ttl → 复用", () => {
  eq(shouldRefetchRemote({ loadedAt: 9_000_000 }, 8_000_000, TTL), false);
});

// === ttlMs 参数化：小 TTL 立即过期 ===
test("ttlMs=0 → 任何非零间隔都重抓", () => {
  eq(shouldRefetchRemote({ loadedAt: 1_000 }, 1_001, 0), true);
});

if (failed > 0) {
  console.error(`\n${failed} history-cache test(s) failed`);
  throw new Error(`history-cache.test.ts: ${failed} failed`);
}
console.log("\nall history-cache tests passed");
