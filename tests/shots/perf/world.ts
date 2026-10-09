/**
 * 性能台架的合成世界：真实规模的一屋子 tab —— 二十几个会话，其中几条是几千条记录的长会话，
 * 每轮都带思考 · 一串工具调用（命令输出 · 读文件 · 改动对比 · 搜索）· 偶尔派子 agent · 大段代码。
 * 正文全是按模板拼的占位（与 `fake/world.ts` 同一套口径），不取任何真会话。
 *
 * 量法与读数住 `bench.mjs` 头注；这里只造数据。
 */
import { Convo } from "../fake/records";
import type { SessionSpec, World } from "../fake/types";
import { defaultWorld, LOCAL, REMOTES, session, sidOf } from "../fake/world";

/** 一段占位代码（`n` 行，按序号变出不同的行，免得整页同一行被浏览器 / 渲染器的缓存吃掉）。 */
function code(n: number, seed: number): string {
  const out: string[] = [];
  for (let i = 0; i < n; i++) {
    const k = (seed * 31 + i * 7) % 97;
    switch (i % 6) {
      case 0:
        out.push(`export function step${k}(input: Item[], limit = ${k + 3}): Result<${k % 2 ? "Batch" : "Page"}> {`);
        break;
      case 1:
        out.push(`  const picked = input.filter((x) => x.weight > ${k} && x.tag !== "skip-${k}");`);
        break;
      case 2:
        out.push(`  if (picked.length === 0) return { ok: false, reason: "empty-${k}" };`);
        break;
      case 3:
        out.push(`  // 第 ${k} 段：按权重排序后截断到 limit，保持稳定序`);
        break;
      case 4:
        out.push(`  return { ok: true, value: picked.sort((a, b) => b.weight - a.weight).slice(0, limit) };`);
        break;
      default:
        out.push("}");
    }
  }
  return out.join("\n");
}

function numbered(body: string): string {
  return body
    .split("\n")
    .map((l, i) => `${String(i + 1).padStart(6)}\t${l}`)
    .join("\n");
}

/** 一轮：你说一句 → 思考 → 一串工具 → 过程中说一两句 → 收尾的回答（带代码块）。 */
function turn(c: Convo, cwd: string, i: number, heavy: boolean): void {
  const mod = `packages/mod-${i % 23}`;
  c.user(`第 ${i} 轮：把 ${mod} 里的分页逻辑改成游标分页，顺带补测试。`);
  c.think(`先看 ${mod} 现在的分页实现与调用点，再决定游标放在哪一层。第 ${i} 轮。`, `我先看一下 ${mod} 的现状。`);
  c.tool("Grep", { pattern: `paginate${i % 9}`, path: mod }, Array.from({ length: 6 }, (_, k) => `${mod}/src/file${k}.ts:${10 + k * 7}:  paginate${i % 9}(items, ${k})`).join("\n"));
  c.tool("Read", { file_path: `${cwd}/${mod}/src/page.ts` }, numbered(code(heavy ? 120 : 60, i)), { card: "md" });
  c.tool(
    "Edit",
    { file_path: `${cwd}/${mod}/src/page.ts`, old_string: code(heavy ? 18 : 8, i), new_string: code(heavy ? 26 : 12, i + 1) },
    "The file has been updated.",
    { card: "diff" },
  );
  c.say(`调用点改完了，接着跑 ${mod} 的测试。`, 30_000 + (i % 50) * 1000);
  const fail = i % 7 === 3;
  c.tool(
    "Bash",
    { command: `npm test -- ${mod}`, description: "跑测试" },
    Array.from({ length: heavy ? 40 : 15 }, (_, k) => (fail && k === 3 ? `FAIL ${mod}/test/page.test.ts > cursor ${k}` : `  ✓ ${mod} > case ${k} (${(k * 3) % 17}ms)`)).join("\n"),
    { card: "command", error: fail },
  );
  if (i % 5 === 0) {
    c.tool(
      "Task",
      { description: `检查 ${mod} 的同类调用`, prompt: `在 ${mod} 之外找所有还在用偏移分页的地方。`, subagent_type: "Explore" },
      `找到 ${i % 4} 处，已列在清单里。`,
      { card: "agent", child: { label: `检查 ${mod} 的同类调用`, kind: "Explore" } },
    );
  }
  if (fail) c.tool("Bash", { command: `npm test -- ${mod}`, description: "重跑" }, "  ✓ all 15 passed", { card: "command" });
  c.say(
    [
      `第 ${i} 轮完成：${mod} 改成游标分页。`,
      "",
      "| 项 | 改前 | 改后 |",
      "|---|---|---|",
      `| 翻页 | offset ${i} | cursor |`,
      `| 测试 | ${10 + (i % 9)} | ${14 + (i % 9)} |`,
      "",
      "```ts",
      code(heavy ? 24 : 10, i + 7),
      "```",
    ].join("\n"),
    40_000 + (i % 60) * 1500,
    "end_turn",
  );
}

/** 一个 `turns` 轮的会话。 */
function convo(n: number, cwd: string, turns: number, heavy: boolean): Convo {
  const c = new Convo(sidOf(n), cwd, "2026-10-01T08:00:00Z");
  c.title(`${turns} 轮 · 游标分页改造 ${n}`);
  for (let i = 1; i <= turns; i++) turn(c, cwd, i, heavy);
  return c;
}

/** 轮数表：四条长会话（几千条记录）＋ 二十条中短会话。 */
export const PERF_TURNS: number[] = [420, 30, 12, 260, 8, 45, 18, 300, 6, 22, 60, 10, 380, 15, 9, 35, 14, 28, 7, 50, 11, 20, 16, 40];

export function perfWorld(): World {
  const w = defaultWorld();
  const origins = [LOCAL, ...REMOTES];
  w.sessions = PERF_TURNS.map((turns, k): SessionSpec => {
    const n = 0x100 + k;
    const origin = origins[k % origins.length];
    const cwd = `/home/user/work/w${k}t${turns}-perf`;
    return session(n, origin, cwd, convo(n, cwd, turns, turns >= 200), { activity: k % 3 === 0 ? "working" : "idle" });
  });
  // 整份扫的那几问记一份（同一问同一答）：假后端与产品同一条主线程，不记的话长会话每问都整份重扫，量进去的是假后端。
  for (const op of ["history-index", "history-turns", "history-user-inputs", "history-facts"]) {
    const inner = w.ops[op];
    if (!inner) continue;
    const memo = new Map<string, unknown>();
    w.ops[op] = (origin, req, world) => {
      const key = `${origin}\u0000${JSON.stringify(req)}`;
      if (!memo.has(key)) memo.set(key, inner(origin, req, world));
      return memo.get(key);
    };
  }
  // 读整份会话的那几问要时间（真后端读几 MB 的记录文件、远端还隔一跳）：照本机常见的量级给一个延迟 ——
  // 快速连切时「切走之后才回来」的那几问才演得出来（不给延迟，假后端当场答，旧切换的活全在切走之前干完了）。
  w.opDelayMs = { "history-index": 150, "history-turns": 150, "history-user-inputs": 120, "history-facts": 120, "history-page": 60 };
  // 真壳的重放缓冲每会话只留尾部 600 条（`event_replay.rs::REPLAY_TAIL_KEEP`）；更早的由骨架索引按偏移取。
  w.replayTail = 600;
  return w;
}
