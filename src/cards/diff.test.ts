/**
 * 纯 diff 核心（diff.ts 上半，DOM-free 层）的断言脚本。
 *
 * 跑法：`npx tsx src/cards/diff.test.ts` 或 `npm run test:diff`。
 * 不是 CI 接线的测试套件（前端无 JS test runner，详 DEVELOPMENT.md）——
 * 这是守护 diff 唯一非平凡逻辑的**手动 pre-push 门禁**。tsc --noEmit 会自动
 * 类型检查本文件（在 src/ 下），所以类型漂移 CI 也能挡。
 *
 * **零依赖刻意**：不 import `node:assert` / 不用 `process`（项目无 @types/node，
 * 那会让 tsc/`npm run build` 红）。用自带的 JSON 深比较断言；失败时 throw 让
 * tsx 进程以非零码退出，作为门禁信号。`console`/`Date` 来自 DOM/ES2020 lib，无需 node types。
 */

import {
  diffLines,
  normalizeEditInput,
  normalizeWriteInput,
  normalizeMultiEditInput,
  diffSegments,
  isDiffTool,
  type DiffResult,
} from "./diff.ts";

let passed = 0;
let failed = 0;

function eq(actual: unknown, expected: unknown, msg?: string): void {
  const a = JSON.stringify(actual);
  const e = JSON.stringify(expected);
  if (a !== e) throw new Error(`${msg ?? "eq"}: expected ${e}, got ${a}`);
}
function ok(cond: boolean, msg?: string): void {
  if (!cond) throw new Error(msg ?? "expected truthy");
}
function test(name: string, fn: () => void): void {
  try {
    fn();
    passed++;
    console.log(`  ✓ ${name}`);
  } catch (e) {
    failed++;
    console.error(`  ✗ ${name}\n      ${e instanceof Error ? e.message : String(e)}`);
  }
}

/** rows → [type, text, oldNo, newNo][] 便于精确断言。 */
const sig = (r: DiffResult) => r.rows.map((x) => [x.type, x.text, x.oldNo, x.newNo]);

// (a) Edit 正常 old≠new → 精确 ops + 计数 + del 在 add 前
test("Edit normal: del-before-add ops + exact line numbers", () => {
  const r = diffLines("a\nb\nc", "a\nx\nc");
  eq(sig(r), [
    ["ctx", "a", 1, 1],
    ["del", "b", 2, null],
    ["add", "x", null, 2],
    ["ctx", "c", 3, 3],
  ]);
  eq(r.addCount, 1, "addCount");
  eq(r.delCount, 1, "delCount");
  eq(r.truncated, false, "truncated");
});

// (b) 完全相同 → 全 ctx，零增删
test("identical: all ctx, zero add/del", () => {
  const r = diffLines("a\nb", "a\nb");
  eq(sig(r), [
    ["ctx", "a", 1, 1],
    ["ctx", "b", 2, 2],
  ]);
  eq(r.addCount, 0, "addCount");
  eq(r.delCount, 0, "delCount");
});

// (c) Write 形态：normalizeWriteInput → {old:'',new:content} → 全 add
test("Write shape: all-add", () => {
  const nw = normalizeWriteInput({ content: "l1\nl2", file_path: "x" });
  eq(nw, { old: "", new: "l1\nl2" });
  const r = diffLines(nw!.old, nw!.new);
  eq(sig(r), [
    ["add", "l1", null, 1],
    ["add", "l2", null, 2],
  ]);
  eq(r.addCount, 2, "addCount");
  eq(r.delCount, 0, "delCount");
});

// (d) 真实异常 Edit（只有 file_path/replace_all）→ normalizeEditInput 返 null，不抛
test("Edit anomaly {file_path,replace_all} → null", () => {
  eq(normalizeEditInput({ file_path: "x", replace_all: true }), null);
});

// (e) 空 old → 全 add；空 new → 全 del
test("empty old → all add; empty new → all del", () => {
  const add = diffLines("", "a\nb");
  eq(add.addCount, 2, "add.addCount");
  eq(add.delCount, 0, "add.delCount");
  ok(add.rows.every((x) => x.type === "add"), "all add rows");

  const del = diffLines("a\nb", "");
  eq(del.delCount, 2, "del.delCount");
  eq(del.addCount, 0, "del.addCount");
  ok(del.rows.every((x) => x.type === "del"), "all del rows");
});

// (f) CRLF-old vs LF-new 同内容 → 归一化后相等，零增删（不误报整文件 diff）
test("CRLF vs LF same content → zero diff", () => {
  const r = diffLines("a\r\nb\r\nc", "a\nb\nc");
  eq(r.addCount, 0, "addCount");
  eq(r.delCount, 0, "delCount");
  ok(r.rows.every((x) => x.type === "ctx"), "all ctx");
});

// (g) 仅尾随换行不同 → 零虚假行
test("trailing-newline-only difference → zero spurious rows", () => {
  const r1 = diffLines("a\nb\n", "a\nb");
  eq(r1.addCount, 0, "r1.addCount");
  eq(r1.delCount, 0, "r1.delCount");
  const r2 = diffLines("a\nb", "a\nb\n");
  eq(r2.addCount, 0, "r2.addCount");
  eq(r2.delCount, 0, "r2.delCount");
});

// (h) MultiEdit 各种畸形 → null
test("MultiEdit malformed → null", () => {
  eq(normalizeMultiEditInput({}), null, "no edits");
  eq(normalizeMultiEditInput({ edits: "nope" }), null, "edits not array");
  eq(normalizeMultiEditInput({ edits: [] }), null, "edits empty");
  eq(
    normalizeMultiEditInput({ edits: [{ old_string: "a", new_string: "b" }, { old_string: "c" }] }),
    null,
    "one edit missing new_string",
  );
});

// (i) MultiEdit 合规 → N 个 {old,new}
test("MultiEdit well-formed → N pairs", () => {
  const r = normalizeMultiEditInput({
    file_path: "x",
    edits: [
      { old_string: "a", new_string: "b" },
      { old_string: "c", new_string: "d", replace_all: true },
    ],
  });
  eq(r, [
    { old: "a", new: "b" },
    { old: "c", new: "d" },
  ]);
});

// (j) 超大输入：cell-budget 守卫退化路径，时间有界，不挂不抛
test("huge 5000-line rewrite: budget-guard degenerate, time-bounded", () => {
  const old = Array.from({ length: 5000 }, (_, i) => `old line ${i}`).join("\n");
  const neu = Array.from({ length: 5000 }, (_, i) => `new line ${i}`).join("\n");
  const t0 = Date.now();
  const r = diffLines(old, neu);
  const dt = Date.now() - t0;
  ok(dt < 500, `took ${dt}ms (expected <500ms — regression to O(mn)/recursion?)`);
  eq(r.truncated, true, "truncated");
  eq(r.addCount, 5000, "addCount");
  eq(r.delCount, 5000, "delCount");
  ok(r.rows.length <= 400, "rows capped");
});

// (j2) 真实 LCS 矩阵路径（预算内）也要时间有界、迭代不爆栈
test("600x600 full rewrite: real LCS path, time-bounded", () => {
  const old = Array.from({ length: 600 }, (_, i) => `o${i}`).join("\n");
  const neu = Array.from({ length: 600 }, (_, i) => `n${i}`).join("\n");
  const t0 = Date.now();
  const r = diffLines(old, neu);
  const dt = Date.now() - t0;
  ok(dt < 500, `took ${dt}ms`);
  eq(r.addCount, 600, "addCount");
  eq(r.delCount, 600, "delCount");
});

// (k) 非字符串 / undefined → 各 normalizer 返 null，不抛
test("non-string / undefined inputs → null, no throw", () => {
  eq(normalizeEditInput(undefined), null, "edit undefined");
  eq(normalizeEditInput("str"), null, "edit string");
  eq(normalizeEditInput({ old_string: 1, new_string: "x" }), null, "edit non-string field");
  eq(normalizeWriteInput({ content: 123 }), null, "write non-string content");
  eq(normalizeWriteInput(null), null, "write null");
  eq(normalizeMultiEditInput(undefined), null, "multiedit undefined");
});

// 截断：rows 封顶 maxLines，但 addCount 仍全量
test("truncation caps rows but counts stay full", () => {
  const neu = Array.from({ length: 500 }, (_, i) => `L${i}`).join("\n");
  const r = diffLines("", neu);
  eq(r.addCount, 500, "addCount full");
  eq(r.rows.length, 400, "rows capped");
  eq(r.truncated, true, "truncated");
});

// 单行字符截断
test("per-line char cap adds ellipsis", () => {
  const r = diffLines("", "x".repeat(3000), { maxCharsPerLine: 100 });
  eq(r.rows.length, 1, "one row");
  eq(r.rows[0].text.length, 101, "capped length");
  ok(r.rows[0].text.endsWith("…"), "ellipsis");
});

// diffSegments 派发 + 未知工具/异常 → null（DOM 层的纯子集，node 可测）
test("diffSegments dispatch + NotebookEdit/unknown/anomaly → null", () => {
  eq(diffSegments("Edit", { old_string: "a", new_string: "b" }), [{ old: "a", new: "b" }]);
  eq(diffSegments("Write", { content: "x" }), [{ old: "", new: "x" }]);
  eq(diffSegments("MultiEdit", { edits: [{ old_string: "a", new_string: "b" }] }), [
    { old: "a", new: "b" },
  ]);
  eq(diffSegments("NotebookEdit", { new_source: "x" }), null, "NotebookEdit not a diff tool");
  eq(diffSegments("Bash", { command: "ls" }), null, "Bash not a diff tool");
  eq(diffSegments("Edit", { file_path: "x" }), null, "Edit anomaly");
});

test("isDiffTool covers Edit/Write/MultiEdit only", () => {
  ok(isDiffTool("Edit") && isDiffTool("Write") && isDiffTool("MultiEdit"), "the three");
  ok(!isDiffTool("NotebookEdit") && !isDiffTool("Bash") && !isDiffTool(""), "exclusions");
});

// "显示完整 diff" 契约：maxLines 控制截断；解除上限后全量、不截断
test("maxLines controls truncation (show-full contract)", () => {
  const neu = Array.from({ length: 50 }, (_, i) => `L${i}`).join("\n");
  const capped = diffLines("", neu, { maxLines: 10 });
  eq(capped.truncated, true, "capped truncated");
  eq(capped.rows.length, 10, "capped rows");
  eq(capped.addCount, 50, "capped full count");
  const full = diffLines("", neu, { maxLines: Number.MAX_SAFE_INTEGER });
  eq(full.truncated, false, "full not truncated");
  eq(full.rows.length, 50, "full rows");
});

console.log(`\n${passed} passed, ${failed} failed`);
if (failed > 0) throw new Error(`${failed} diff test(s) failed`);
