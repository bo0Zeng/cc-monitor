/**
 * F70 collectEditedFiles 纯函数断言（node 轨，跑法 `npx tsx src/panorama/session-files.test.ts`）。
 * 零 node 依赖（项目无 @types/node）：自带计数 + eq，失败 throw 让 tsx 非零退出。
 */
import { collectEditedFiles } from "./session-files.ts";

let passed = 0;
let failed = 0;
function test(name: string, fn: () => void): void {
  try {
    fn();
    passed += 1;
    console.log(`  ✓ ${name}`);
  } catch (e) {
    failed += 1;
    console.error(`  ✗ ${name}: ${String(e)}`);
  }
}
function eq(actual: unknown, expected: unknown, msg?: string): void {
  const a = JSON.stringify(actual);
  const e = JSON.stringify(expected);
  if (a !== e) throw new Error(`${msg ?? "eq"}: expected ${e}, got ${a}`);
}

const asst = (blocks: unknown[]): unknown => ({
  type: "assistant",
  message: { content: blocks },
});
const toolUse = (name: string, input: unknown): unknown => ({
  type: "tool_use",
  name,
  input,
});

test("Edit/Write/MultiEdit 取 file_path", () => {
  eq(collectEditedFiles(asst([toolUse("Edit", { file_path: "/a.ts" })])), ["/a.ts"]);
  eq(collectEditedFiles(asst([toolUse("Write", { file_path: "/b.rs" })])), ["/b.rs"]);
  eq(collectEditedFiles(asst([toolUse("MultiEdit", { file_path: "/c.py" })])), ["/c.py"]);
});

test("NotebookEdit 取 notebook_path", () => {
  eq(
    collectEditedFiles(asst([toolUse("NotebookEdit", { notebook_path: "/n.ipynb" })])),
    ["/n.ipynb"],
  );
});

test("非写类工具（Bash/Read/Grep）不收", () => {
  eq(
    collectEditedFiles(
      asst([
        toolUse("Bash", { command: "ls" }),
        toolUse("Read", { file_path: "/r.ts" }),
        toolUse("Grep", { pattern: "x" }),
      ]),
    ),
    [],
  );
});

test("多 tool_use 全收 + 保序（穿插非写类）", () => {
  eq(
    collectEditedFiles(
      asst([
        toolUse("Edit", { file_path: "/a" }),
        toolUse("Bash", { command: "x" }),
        toolUse("Write", { file_path: "/b" }),
      ]),
    ),
    ["/a", "/b"],
  );
});

test("非 assistant 记录 → 空（tool_use 只在 assistant）", () => {
  eq(collectEditedFiles({ type: "user", message: { content: [toolUse("Edit", { file_path: "/a" })] } }), []);
});

test("畸形/缺字段不抛，静默跳过", () => {
  eq(collectEditedFiles(asst([toolUse("Edit", {})])), [], "无 file_path");
  eq(collectEditedFiles(asst([toolUse("Edit", { file_path: 123 })])), [], "非字符串");
  eq(collectEditedFiles(asst([toolUse("Edit", { file_path: "" })])), [], "空串");
  eq(collectEditedFiles(asst([{ type: "text", text: "hi" }])), [], "非 tool_use");
  eq(collectEditedFiles({}), [], "无 content");
  eq(collectEditedFiles(null), [], "null");
  eq(collectEditedFiles({ type: "assistant", message: { content: "notarray" } }), [], "content 非数组");
});

test("Windows 绝对路径原样收（不做 / 前缀过滤，对齐交后端 to_rel）", () => {
  eq(
    collectEditedFiles(asst([toolUse("Write", { file_path: "C:\\proj\\a.ts" })])),
    ["C:\\proj\\a.ts"],
  );
});

if (failed > 0) {
  console.error(`\n${failed} session-files test(s) failed`);
  throw new Error(`session-files.test.ts: ${failed} failed`);
}
console.log("\nall session-files tests passed");
