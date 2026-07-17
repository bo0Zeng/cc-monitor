/**
 * usage-pivot.ts 纯函数断言：pivotUsage 四维聚合 + sumAll + totalTokens。
 * 跑法：`node src/views/usage-pivot.test.ts` 或 `npm run test:usage-pivot`。
 * 同 history-cache.test.ts：零依赖、失败 throw 作门禁；tsc --noEmit 类型检查。
 */

import {
  pivotUsage,
  sumAll,
  totalTokens,
  type SessionUsageRow,
} from "./usage-pivot.ts";

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

console.log("usage-pivot.test.ts");

const t = (input: number, cc: number, cr: number, output: number, msgs = 1) => ({
  input, cacheCreation: cc, cacheRead: cr, output, msgs,
});
const rows: SessionUsageRow[] = [
  {
    sessionId: "s1",
    projectPath: "/a",
    projectName: "A",
    buckets: [
      { model: "opus", day: "2026-07-17", totals: t(100, 10, 500, 20) },
      { model: "haiku", day: "2026-07-18", totals: t(5, 0, 0, 3) },
    ],
  },
  {
    sessionId: "s2",
    projectPath: "/b",
    projectName: "B",
    buckets: [{ model: "opus", day: "2026-07-17", totals: t(50, 0, 200, 10) }],
  },
];

test("totalTokens 四类相加", () => {
  eq(totalTokens(t(1, 2, 3, 4)), 10);
});

test("按 model pivot：opus 合并两会话，haiku 独立", () => {
  const p = pivotUsage(rows, "model");
  const opus = p.find((r) => r.key === "opus")!;
  eq(opus.totals.input, 150); // 100+50
  eq(opus.totals.cacheRead, 700); // 500+200
  eq(opus.totals.output, 30);
  eq(opus.totals.msgs, 2);
  const haiku = p.find((r) => r.key === "haiku")!;
  eq(haiku.totals.input, 5);
  eq(p.length, 2);
  // 按总 token 降序：opus(880) 在 haiku(8) 前
  eq(p[0].key, "opus");
});

test("按 day pivot：17 号跨会话合并", () => {
  const p = pivotUsage(rows, "day");
  const d17 = p.find((r) => r.key === "2026-07-17")!;
  eq(d17.totals.input, 150);
  eq(d17.totals.output, 30);
  const d18 = p.find((r) => r.key === "2026-07-18")!;
  eq(d18.totals.input, 5);
  eq(p.length, 2);
});

test("按 project pivot：每项目独立", () => {
  const p = pivotUsage(rows, "project");
  eq(p.length, 2);
  const a = p.find((r) => r.key === "/a")!;
  eq(a.totals.input, 105); // s1 两桶 100+5
  eq(a.label, "A");
});

test("按 session pivot：每会话所有桶合计", () => {
  const p = pivotUsage(rows, "session");
  const s1 = p.find((r) => r.key === "s1")!;
  eq(s1.totals.input, 105);
  eq(s1.totals.msgs, 2);
  eq(p.length, 2);
});

test("远端 origin 进 key/label", () => {
  const remote: SessionUsageRow[] = [
    { sessionId: "s3", projectPath: "/r", projectName: "R", origin: "pi", buckets: [{ model: "opus", day: "2026-07-17", totals: t(1, 0, 0, 1) }] },
  ];
  const p = pivotUsage(remote, "project");
  eq(p[0].key, "pi\u0000/r"); // origin 用 \u0000 命名空间分隔（同 history.ts projectKey 防撞）
  eq(p[0].label, "[pi] R");
});

test("sumAll 全部桶合计", () => {
  const s = sumAll(rows);
  eq(s.input, 155);
  eq(s.cacheRead, 700);
  eq(s.output, 33);
  eq(s.msgs, 3);
});

test("空 rows → 空 pivot / 零 sum", () => {
  eq(pivotUsage([], "model").length, 0);
  eq(sumAll([]).input, 0);
});

if (failed > 0) {
  console.error(`\n${failed} usage-pivot test(s) failed`);
  throw new Error(`usage-pivot.test.ts: ${failed} failed`);
}
console.log("\nall usage-pivot tests passed");
