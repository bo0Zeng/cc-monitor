/**
 * history-actions.ts 纯函数断言脚本：动作表判定（actionsFor / enabled / label）。
 *
 * 跑法：`node src/views/history-actions.test.ts` 或 `npm run test:history-actions`。
 * 同 history-cache/history-prefs.test.ts：零依赖、失败 throw 作门禁；tsc --noEmit 类型检查。
 *
 * 为什么值得锁：F96（#62）③块动作表。搜索卡片（hasEntry:false）必须只出 resume/new-session
 * ——若误出 star/hide/rename/delete，那些动作无活 entry 可施力 → 静默失败。这条边界最该钉。
 */

import {
  actionsFor,
  HISTORY_ACTION_DEFS,
  type HistoryActionCtx,
} from "./history-actions.ts";

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

console.log("history-actions.test.ts");

const base = (over: Partial<HistoryActionCtx> = {}): HistoryActionCtx => ({
  sessionId: "s1",
  jsonlPath: "/p/s1.jsonl",
  cwd: "/p",
  hasEntry: true,
  ...over,
});

const ids = (ctx: HistoryActionCtx): string[] => actionsFor(ctx).map((d) => d.id);

// === 条目行（hasEntry:true）出全套 ===
test("条目行本地：全套动作（稳定顺序）", () => {
  deepEq(ids(base()), ["resume", "new-session", "star", "rename", "hide", "delete"]);
});
test("条目行远端：也全套", () => {
  deepEq(ids(base({ origin: "hostA" })), [
    "resume",
    "new-session",
    "star",
    "rename",
    "hide",
    "delete",
  ]);
});

// === 搜索卡片（hasEntry:false）只出 resume/new-session（核心边界）===
test("搜索卡片：只出 resume + new-session（无 star/rename/hide/delete）", () => {
  deepEq(ids(base({ hasEntry: false })), ["resume", "new-session"]);
});
test("搜索卡片远端：同样只 resume + new-session", () => {
  deepEq(ids(base({ hasEntry: false, origin: "hostA" })), ["resume", "new-session"]);
});

// === new-session 本地远端都 enabled（D1：都做）===
test("new-session 本地 enabled", () => {
  eq(actionsFor(base()).some((d) => d.id === "new-session"), true);
});
test("new-session 远端 enabled", () => {
  eq(actionsFor(base({ origin: "h" })).some((d) => d.id === "new-session"), true);
});

// === label 状态感知 ===
function labelOf(ctx: HistoryActionCtx, id: string): string {
  const d = HISTORY_ACTION_DEFS.find((x) => x.id === id)!;
  return d.label(ctx);
}
test("star label 随 starred", () => {
  eq(labelOf(base({ starred: false }), "star"), "标星");
  eq(labelOf(base({ starred: true }), "star"), "取消标星");
});
test("hide label 随 hidden", () => {
  eq(labelOf(base({ hidden: false }), "hide"), "隐藏");
  eq(labelOf(base({ hidden: true }), "hide"), "取消隐藏");
});
test("delete 是 danger", () => {
  eq(HISTORY_ACTION_DEFS.find((d) => d.id === "delete")!.danger, true);
});

// === DEFS 顺序稳定 ===
test("HISTORY_ACTION_DEFS 顺序固定", () => {
  deepEq(
    HISTORY_ACTION_DEFS.map((d) => d.id),
    ["resume", "new-session", "star", "rename", "hide", "delete"],
  );
});

if (failed > 0) {
  console.error(`\n${failed} history-actions test(s) failed`);
  throw new Error(`history-actions.test.ts: ${failed} failed`);
}
console.log("\nall history-actions tests passed");
