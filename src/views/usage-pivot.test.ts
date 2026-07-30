/**
 * usage-pivot.ts 纯函数断言：pivotUsage 四维聚合 + sumAll + totalTokens。
 * 跑法：`node src/views/usage-pivot.test.ts` 或 `npm run test:usage-pivot`。
 * 同 history-cache.test.ts：零依赖、失败 throw 作门禁；tsc --noEmit 类型检查。
 */

import {
  pivotUsage,
  sortPivotRows,
  defaultSortForDim,
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
    // C04d 批 3：`origin` 现在是**必填** `string | null`——线上恒有该键、值可能为 null
    // （Rust 侧 `#[serde(default)] Option<String>` 且无 skip_serializing_if）。
    // 夹具此前省略它 ⇒ 在造一个线上不存在的形状。
    origin: null,
    buckets: [
      { model: "opus", day: "2026-07-17", totals: t(100, 10, 500, 20) },
      { model: "haiku", day: "2026-07-18", totals: t(5, 0, 0, 3) },
    ],
  },
  {
    sessionId: "s2",
    projectPath: "/b",
    projectName: "B",
    // C04d 批 3：`origin` 现在是**必填** `string | null`——线上恒有该键、值可能为 null
    // （Rust 侧 `#[serde(default)] Option<String>` 且无 skip_serializing_if）。
    // 夹具此前省略它 ⇒ 在造一个线上不存在的形状。
    origin: null,
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
  // F88d-fix：按**等效成本**降序（opus 等效 383 = 150 + 10×1.25 + 700×0.1 + 30×5 > haiku 等效 20 = 5 + 3×5）
  // ——此例 raw 与等效同序，opus 仍居首
  eq(p[0].key, "opus");
});

test("F88d-fix 按等效成本降序（raw 与等效不同序时以等效为准）", () => {
  // A：狂读 cache（便宜，×0.1）raw 大但等效小；B：output 重（贵，×5）raw 小但等效大。
  const rows2: SessionUsageRow[] = [
    { sessionId: "a", projectPath: "/a", projectName: "A", origin: null, buckets: [{ model: "m", day: "d", totals: t(0, 0, 10000, 0) }] }, // raw 10000 / 等效 1000
    { sessionId: "b", projectPath: "/b", projectName: "B", origin: null, buckets: [{ model: "m", day: "d", totals: t(0, 0, 0, 1000) }] }, // raw 1000 / 等效 5000
  ];
  const p = pivotUsage(rows2, "project");
  // 按 raw 应 A 先（10000>1000）；按等效应 B 先（5000>1000）→ 修后表以等效为准
  eq(p[0].key, "/b");
  eq(p[1].key, "/a");
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

test("#67 defaultSortForDim：按天=日期降序;其余维度=等效∑降序", () => {
  eq(defaultSortForDim("day").col, "key");
  eq(defaultSortForDim("day").dir, "desc");
  eq(defaultSortForDim("project").col, "equiv");
  eq(defaultSortForDim("model").col, "equiv");
  eq(defaultSortForDim("session").col, "equiv");
});

test("#67 按天 + 默认排序 → 日期降序(最近在上),而非等效∑序(区分修没修)", () => {
  const p = pivotUsage(rows, "day");
  // 底座是等效∑降序:07-17 token 远多于 07-18 → 排前。这正是用户看到的「按天却日期乱跳」。
  eq(p[0].key, "2026-07-17", "底座应是等效∑序");
  const d = defaultSortForDim("day");
  const sorted = sortPivotRows(p, d.col, d.dir);
  // ★区分:按天默认必须按日期降序 → 最近的 07-18 在最上(未修则仍是 07-17)
  eq(sorted[0].key, "2026-07-18", "按天默认应最近在上");
  eq(sorted[1].key, "2026-07-17");
});

test("#67 sortPivotRows：key 升序=日历序 / 数值列升降序 / 不改入参", () => {
  const p = pivotUsage(rows, "day");
  eq(sortPivotRows(p, "key", "asc")[0].key, "2026-07-17", "key 升序=最早在上");
  eq(sortPivotRows(p, "key", "desc")[0].key, "2026-07-18", "key 降序=最近在上");
  // input:07-17 合计 150 > 07-18 的 5
  eq(sortPivotRows(p, "input", "desc")[0].key, "2026-07-17");
  eq(sortPivotRows(p, "input", "asc")[0].key, "2026-07-18");
  eq(p[0].key, "2026-07-17", "sortPivotRows 必须是纯函数、不改入参顺序");
});

test("#67 平手回退跟随方向：主键全平手时 asc 与 desc 互为逆序(不再「点了没反应」)", () => {
  // 三天 msgs 全 = 1(主键全平手)、等效∑各不同 → asc 应恰是 desc 的逆序
  const tie: SessionUsageRow[] = [
    { sessionId: "a", projectPath: "/a", projectName: "a", origin: null, buckets: [{ model: "m", day: "2026-07-01", totals: t(10, 0, 0, 0) }] },
    { sessionId: "b", projectPath: "/b", projectName: "b", origin: null, buckets: [{ model: "m", day: "2026-07-02", totals: t(30, 0, 0, 0) }] },
    { sessionId: "c", projectPath: "/c", projectName: "c", origin: null, buckets: [{ model: "m", day: "2026-07-03", totals: t(20, 0, 0, 0) }] },
  ];
  const p = pivotUsage(tie, "day");
  const asc = sortPivotRows(p, "msgs", "asc").map((r) => r.key);
  const desc = sortPivotRows(p, "msgs", "desc").map((r) => r.key);
  eq(JSON.stringify(asc), JSON.stringify([...desc].reverse()), "asc 必须是 desc 的逆序");
  // desc 平手时等效∑大的在上(与主排序方向同构)
  eq(desc[0], "2026-07-02", "desc 平手 → 等效∑最大的(30)在上");
});

test("#67 边角：空数组 / 单行 / 空 day 串都不崩", () => {
  eq(sortPivotRows([], "key", "asc").length, 0);
  const one = pivotUsage(
    [{ sessionId: "s", projectPath: "/p", projectName: "P", origin: null, buckets: [{ model: "m", day: "2026-07-01", totals: t(1, 0, 0, 0) }] }],
    "day",
  );
  eq(sortPivotRows(one, "total", "desc").length, 1);
  const withEmpty = pivotUsage(
    [
      { sessionId: "s1", projectPath: "/p", projectName: "P", origin: null, buckets: [{ model: "m", day: "", totals: t(1, 0, 0, 0) }] },
      { sessionId: "s2", projectPath: "/p", projectName: "P", origin: null, buckets: [{ model: "m", day: "2026-07-01", totals: t(2, 0, 0, 0) }] },
    ],
    "day",
  );
  // 空 day 的 label 是 `(无日期)`;具体落位随 locale 排序规则,这里只钉「不崩、不丢行」
  eq(sortPivotRows(withEmpty, "key", "asc").length, 2);
  eq(sortPivotRows(withEmpty, "key", "desc").length, 2);
});

if (failed > 0) {
  console.error(`\n${failed} usage-pivot test(s) failed`);
  throw new Error(`usage-pivot.test.ts: ${failed} failed`);
}
console.log("\nall usage-pivot tests passed");
