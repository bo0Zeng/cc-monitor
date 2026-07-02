/**
 * bash.ts / slash.ts 纯逻辑断言脚本（Batch4-F16）。
 *
 * 跑法：`node src/cards/bash.test.ts` 或 `npm run test:bash`。
 * 同 api-error.test.ts / diff.test.ts：零 node 依赖、失败非零退出作 pre-push
 * 门禁；只测解析纯函数，不碰 DOM 构建（那部分归 tsc + 目检）。
 *
 * 为什么值得锁：①标签内容经 CLI HTML 实体转义（fixture 取自真实 jsonl，含
 * `&gt;`），反转义遗漏会让终端输出显示成 `&gt; vite build`；②slash 三标签
 * **顺序随 CLI 版本漂移**（新版 message→name→args），旧正则钉死旧序正是
 * 本功能修的漏渲染根因——两种顺序都必须钉住。
 */

import { parseBashInput, parseBashOutput, unescapeEntities } from "./bash.ts";
import { parseSlashCommand } from "./slash.ts";

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
    throw new Error(
      `${msg ?? "eq"}: expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`,
    );
  }
}

console.log("bash.test.ts");

// === parseBashInput（fixture：真实 jsonl 样本） ===

test("bash-input: real sample", () => {
  const r = parseBashInput("<bash-input>npm install && npm run build</bash-input>");
  eq(r?.command, "npm install && npm run build");
});

test("bash-input: leading space in command (real sample)", () => {
  const r = parseBashInput("<bash-input> sudo apt install -y gh</bash-input>");
  eq(r?.command, "sudo apt install -y gh");
});

test("bash-input: entity-escaped redirect", () => {
  const r = parseBashInput("<bash-input>echo hi 2&gt;&amp;1</bash-input>");
  eq(r?.command, "echo hi 2>&1");
});

test("bash-input: plain text falls through", () => {
  eq(parseBashInput("普通消息提到 <bash-input> 这个词"), null);
  eq(parseBashInput("bash-input 没有标签"), null);
  eq(parseBashInput(""), null);
});

test("bash-input: trailing garbage falls through", () => {
  eq(parseBashInput("<bash-input>ls</bash-input> 以及别的话"), null);
});

test("bash-input: empty/whitespace command falls through (理论边界)", () => {
  eq(parseBashInput("<bash-input></bash-input>"), null);
  eq(parseBashInput("<bash-input>   </bash-input>"), null);
});

// === parseBashOutput（fixture：真实 jsonl 样本形态） ===

test("bash-output: stdout only", () => {
  const r = parseBashOutput("<bash-stdout>added 35 packages\n\n5 vulnerabilities</bash-stdout>");
  eq(r?.stdout, "added 35 packages\n\n5 vulnerabilities");
  eq(r?.stderr, "");
});

test("bash-output: empty stdout + stderr (real sample shape)", () => {
  const r = parseBashOutput(
    "<bash-stdout></bash-stdout><bash-stderr>fatal: could not read Username</bash-stderr>",
  );
  eq(r?.stdout, "");
  eq(r?.stderr, "fatal: could not read Username");
});

test("bash-output: entity-escaped content (real sample has &gt;)", () => {
  const r = parseBashOutput("<bash-stdout>&gt; vite build\nvite v6.4.2 building...</bash-stdout>");
  eq(r?.stdout, "> vite build\nvite v6.4.2 building...");
});

test("bash-output: stderr-first tolerated", () => {
  const r = parseBashOutput("<bash-stderr>warning</bash-stderr>");
  eq(r?.stdout, "");
  eq(r?.stderr, "warning");
});

test("bash-output: unrecognized residue falls through", () => {
  eq(parseBashOutput("<bash-stdout>x</bash-stdout>还有别的"), null);
  eq(parseBashOutput("聊天里提到 <bash-stdout> 标签"), null);
});

// === unescapeEntities ===

test("entities: single-pass, no double decode", () => {
  eq(unescapeEntities("&amp;lt;"), "&lt;", "字面 &lt; 不得二次解码");
  eq(unescapeEntities("a &lt; b &gt; c &quot;d&quot; &#39;e&#39; &amp;&amp;"), 'a < b > c "d" \'e\' &&');
});

// === parseSlashCommand：新旧标签顺序都必须命中 ===

test("slash: new order message→name→args (real sample)", () => {
  const r = parseSlashCommand(
    "<command-message>full-audit</command-message>\n<command-name>/full-audit</command-name>\n<command-args>全面理解这个项目</command-args>",
  );
  eq(r?.name, "/full-audit");
  eq(r?.args, "全面理解这个项目");
});

test("slash: old order name→message→args", () => {
  const r = parseSlashCommand(
    "<command-name>/compact</command-name>\n<command-message>compact</command-message>\n<command-args></command-args>",
  );
  eq(r?.name, "/compact");
  eq(r?.args, "");
});

test("slash: entity-escaped args are decoded", () => {
  const r = parseSlashCommand(
    "<command-message>x</command-message><command-name>/run</command-name><command-args>a &gt; b &amp;&amp; c</command-args>",
  );
  eq(r?.args, "a > b && c");
});

test("slash: args optional (only name+message)", () => {
  const r = parseSlashCommand(
    "<command-message>loop</command-message><command-name>/loop</command-name>",
  );
  eq(r?.name, "/loop");
  eq(r?.args, "");
});

test("slash: leftover text falls through (正文里恰好含标签)", () => {
  eq(
    parseSlashCommand("看这段 <command-name>/x</command-name> 后面还有正文"),
    null,
  );
});

test("slash: duplicated tag leaves residue → falls through", () => {
  eq(
    parseSlashCommand(
      "<command-name>/a</command-name><command-name>/b</command-name>",
    ),
    null,
  );
});

test("slash: no name → falls through", () => {
  eq(parseSlashCommand("<command-message>x</command-message>"), null);
});

test("slash: empty name → falls through (与 bash 空命令对称)", () => {
  eq(
    parseSlashCommand("<command-name></command-name><command-message>x</command-message>"),
    null,
  );
});

if (failed > 0) {
  console.error(`\n${failed} bash/slash test(s) failed`);
  // 同 api-error.test.ts：throw 顶层异常让 node 非零退出（不引 @types/node）
  throw new Error(`bash.test.ts: ${failed} failed`);
}
console.log("all bash/slash tests passed");
