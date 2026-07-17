/**
 * history-prefs.ts 纯函数断言脚本：来源折叠默认/解析/反污染 + 持久化防御性解析。
 *
 * 跑法：`node src/views/history-prefs.test.ts` 或 `npm run test:history-prefs`。
 * 同 history-cache.test.ts：零 node 依赖、失败 throw 非零退出作门禁；tsc --noEmit 类型检查。
 *
 * 为什么值得锁：F86（#45）折叠偏好的判定核心。「首见默认 vs 用户显式偏好」的反污染写入
 * （nextOverrides）最易错——判错就是「用户展开后又被默认折叠盖掉」或「首见折叠污染成偏好」，
 * 且宿主 history.ts 零 TS 单测。
 */

import {
  defaultOriginOpen,
  resolveOriginOpen,
  nextOverrides,
  sameOverrides,
  normalizeOverrides,
  normalizeOriginKeys,
} from "./history-prefs.ts";

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
function deepEq(actual: unknown, expected: unknown, msg?: string): void {
  const a = JSON.stringify(actual);
  const e = JSON.stringify(expected);
  if (a !== e) throw new Error(`${msg ?? "deepEq"}: expected ${e}, got ${a}`);
}

console.log("history-prefs.test.ts");

// === defaultOriginOpen：本地展开、远端折叠 ===
test("默认：本地(undefined) → 展开", () => {
  eq(defaultOriginOpen(undefined), true);
});
test("默认：远端 → 折叠", () => {
  eq(defaultOriginOpen("hostA"), false);
  eq(defaultOriginOpen(""), false); // 非 undefined 一律远端语义（本地传的是 undefined）
});

// === resolveOriginOpen：搜索 > 覆盖 > 默认 ===
test("解析：searchActive 一律展开（含 override=false 的远端）", () => {
  eq(resolveOriginOpen(false, "hostA", true), true);
  eq(resolveOriginOpen(undefined, "hostA", true), true);
});
test("解析：无 override → 走默认（远端折叠 / 本地展开）", () => {
  eq(resolveOriginOpen(undefined, "hostA", false), false);
  eq(resolveOriginOpen(undefined, undefined, false), true);
});
test("解析：有 override → 尊重之", () => {
  eq(resolveOriginOpen(true, "hostA", false), true); // 用户展开了远端
  eq(resolveOriginOpen(false, undefined, false), false); // 用户折叠了本地
});

// === nextOverrides：反污染核心 ===
test("远端展开（true≠默认false）→ 存 {host:true}", () => {
  deepEq(nextOverrides({}, "hostA", "hostA", true), { hostA: true });
});
test("远端从展开折回（false==默认）→ 删键、表空", () => {
  deepEq(nextOverrides({ hostA: true }, "hostA", "hostA", false), {});
});
test("本地折叠（false≠默认true）→ 存 {'':false}", () => {
  deepEq(nextOverrides({}, "", undefined, false), { "": false });
});
test("首见默认折叠的程序化态（远端 newOpen=false==默认）→ 不写键（防污染）", () => {
  deepEq(nextOverrides({}, "hostA", "hostA", false), {});
});
test("本地从折叠回到展开（true==默认）→ 删键", () => {
  deepEq(nextOverrides({ "": false }, "", undefined, true), {});
});
test("返回新对象、不 mutate 入参", () => {
  const cur = { hostA: true };
  const out = nextOverrides(cur, "hostB", "hostB", true);
  deepEq(cur, { hostA: true }, "入参未变");
  deepEq(out, { hostA: true, hostB: true });
});

// === 防御性解析 ===
test("normalizeOverrides：脏数据（null/数组/非 bool 项）→ 只留合法 bool 项", () => {
  deepEq(normalizeOverrides(null), {});
  deepEq(normalizeOverrides([1, 2]), {});
  deepEq(normalizeOverrides("x"), {});
  deepEq(normalizeOverrides({ a: true, b: "yes", c: false }), { a: true, c: false });
});
test("normalizeOriginKeys：非数组→空；只留字符串项", () => {
  deepEq(normalizeOriginKeys(null), []);
  deepEq(normalizeOriginKeys({}), []);
  deepEq(normalizeOriginKeys(["a", 1, "b", null]), ["a", "b"]);
});

// === sameOverrides（写放大守卫核心）===
test("sameOverrides：空表 == 空表", () => {
  eq(sameOverrides({}, {}), true);
});
test("sameOverrides：同键同值 → true", () => {
  eq(sameOverrides({ hostA: true, "": false }, { "": false, hostA: true }), true);
});
test("sameOverrides：键数不同 → false", () => {
  eq(sameOverrides({ hostA: true }, {}), false);
  eq(sameOverrides({ hostA: true }, { hostA: true, hostB: false }), false);
});
test("sameOverrides：同键不同值 → false", () => {
  eq(sameOverrides({ hostA: true }, { hostA: false }), false);
});
test("sameOverrides：键不同（数同）→ false", () => {
  eq(sameOverrides({ hostA: true }, { hostB: true }), false);
});

if (failed > 0) {
  console.error(`\n${failed} history-prefs test(s) failed`);
  throw new Error(`history-prefs.test.ts: ${failed} failed`);
}
console.log("\nall history-prefs tests passed");
