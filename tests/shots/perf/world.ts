/**
 * 性能台架的合成世界：真实规模的一屋子 tab —— 二十几个会话，其中几条是几千条记录的长会话，
 * 每轮都带思考 · 一串工具调用（命令输出 · 读文件 · 改动对比 · 搜索）· 偶尔派子 agent · 大段代码。
 * 正文全是按模板拼的占位（与 `fake/world.ts` 同一套口径），不取任何真会话。
 *
 * 量法与读数住 `bench.mjs` 头注；这里只造数据。
 */
import { Convo, jsonlOf } from "../fake/records";
import { HOME } from "../disk";
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
export function turn(c: Convo, cwd: string, i: number, heavy: boolean): void {
  const mod = `packages/mod-${i % 23}`;
  c.user(`第 ${i} 轮：把 ${mod} 里的分页逻辑改成游标分页，顺带补测试。`);
  c.think(`先看 ${mod} 现在的分页实现与调用点，再决定游标放在哪一层。第 ${i} 轮。`, `我先看一下 ${mod} 的现状。`);
  c.tool("Grep", { pattern: `paginate${i % 9}`, path: mod }, Array.from({ length: 6 }, (_, k) => `${mod}/src/file${k}.ts:${10 + k * 7}:  paginate${i % 9}(items, ${k})`).join("\n"));
  c.tool("Read", { file_path: `${cwd}/${mod}/src/page.ts` }, numbered(code(heavy ? 120 : 60, i)));
  c.tool(
    "Edit",
    { file_path: `${cwd}/${mod}/src/page.ts`, old_string: code(heavy ? 18 : 8, i), new_string: code(heavy ? 26 : 12, i + 1) },
    "The file has been updated."
  );
  c.say(`调用点改完了，接着跑 ${mod} 的测试。`, 30_000 + (i % 50) * 1000);
  const fail = i % 7 === 3;
  c.tool(
    "Bash",
    { command: `npm test -- ${mod}`, description: "跑测试" },
    Array.from({ length: heavy ? 40 : 15 }, (_, k) => (fail && k === 3 ? `FAIL ${mod}/test/page.test.ts > cursor ${k}` : `  ✓ ${mod} > case ${k} (${(k * 3) % 17}ms)`)).join("\n"),
    { error: fail },
  );
  if (i % 5 === 0) {
    const ch = c.child(`t${i}`);
    ch.user(`在 ${mod} 之外找所有还在用偏移分页的地方。`);
    ch.tool("Grep", { pattern: "offset", path: "packages" }, `${mod}/src/page.ts:12`);
    ch.say(`找到 ${i % 4} 处，已列在清单里。`, 20_000, "end_turn");
    c.tool(
      "Task",
      { description: `检查 ${mod} 的同类调用`, prompt: `在 ${mod} 之外找所有还在用偏移分页的地方。`, subagent_type: "Explore" },
      `找到 ${i % 4} 处，已列在清单里。`,
      { child: ch },
    );
  }
  if (fail) c.tool("Bash", { command: `npm test -- ${mod}`, description: "重跑" }, "  ✓ all 15 passed");
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

export function perfWorld(table: readonly number[] = PERF_TURNS): World {
  const w = defaultWorld();
  const origins = [LOCAL, ...REMOTES];
  w.sessions = table.map((turns, k): SessionSpec => {
    const n = 0x100 + k;
    const origin = origins[k % origins.length];
    const cwd = `/home/user/work/w${k}t${turns}-perf`;
    return session(n, origin, cwd, convo(n, cwd, turns, turns >= 200), { activity: k % 3 === 0 ? "working" : "idle" });
  });
  // 查看窗按 sid 问历史清单要那一条的记录路径：清单是 `fake/history.ts` 里写死的那几条 ⇒ 问到台架的会话时照第一条的形状补一条
  const list = w.ops["history-list"];
  if (list) {
    w.ops["history-list"] = (origin, req, world) => {
      const out = list(origin, req, world) as { rows: Record<string, unknown>[] };
      const s = world.sessions.find((x) => x.sid === req.sid);
      if (!s || out.rows.some((r) => r.sessionId === s.sid)) return out;
      const tmpl = (list("<local>", {}, world) as { rows: Record<string, unknown>[] }).rows[0];
      const name = s.cwd.slice(s.cwd.lastIndexOf("/") + 1);
      return { ...out, rows: [{ ...tmpl, sessionId: s.sid, jsonlPath: `${HOME}/${jsonlOf(s.cwd, s.sid)}`, projectPath: s.cwd, projectName: name, aiTitle: null, firstUserExcerpt: name, title: name, label: name, messageCountApprox: s.convo?.lines.length ?? 0 }] };
    };
  }
  // 读整份会话的那几问（真后端读几 MB 的记录文件、远端还隔一跳）：照本机常见的量级给一个延迟 ——
  // 快速连切时「切走之后才回来」的那几问才演得出来（不给延迟，假后端当场答，旧切换的活全在切走之前干完了）。
  w.opDelayMs = { "history-index": 150, "history-turns": 150, "history-user-inputs": 120, "history-facts": 120, "history-page": 60 };
  return w;
}

/**
 * 主窗口其余那几项（perfA：滚动 · 实时来消息 · 查找 · 大纲 · 「需手动」· 面板 · 抽屉）的世界：同一屋子 tab，
 * 另加几条在等你的（「需手动」清单有东西）、最长那条带一串子 agent 与任务（agent / 任务抽屉有东西）。
 */
export function perfMainWorld(table: readonly number[] = PERF_TURNS): World {
  const w = perfWorld(table);
  const now = Date.now();
  [2, 5, 9, 14, 19].forEach((k, j) => {
    const s = w.sessions[k];
    s.activity = "needs_you";
    s.waitingFor = j % 2 ? "question" : "permission prompt";
    s.waitingSinceMs = now - (j + 1) * 90_000;
  });
  const longest = w.sessions[0];
  // 一串子 agent：前三个还在跑（后台派的，当场回「已起」），其余做完了（前台派的，那次调用拿到结果）。
  const c = longest.convo!;
  for (let k = 0; k < 12; k++) {
    const ch = new Convo(longest.sid, longest.cwd, new Date(now - (40 - k) * 60_000).toISOString(), undefined, `p${k}`);
    ch.user(`检查 packages/mod-${k} 的同类调用。`);
    for (let i = 0; i < (k === 0 ? 300 : 20); i++) turn(ch, longest.cwd, i + 1, false);
    if (k >= 3) ch.say("查完了。", 20_000, "end_turn");
    const input = { description: `检查 packages/mod-${k} 的同类调用`, prompt: `检查 packages/mod-${k} 的同类调用。`, subagent_type: "Explore" };
    c.tool("Task", input, k < 3 ? "Async agent launched successfully." : "查完了。", { child: ch, launched: k < 3 });
  }
  longest.tasks = Array.from({ length: 30 }, (_, k) => ({ id: String(k + 1), subject: `改 packages/mod-${k} 的分页`, status: k < 20 ? "completed" : k < 22 ? "in_progress" : "pending", blocks: [], blockedBy: [] }));
  return w;
}

/**
 * 用户报「主窗口卡渲染」的那一种规模（10-10）：一条会话 11.5 万行上下（真会话那份记录文件 349 MB），其余同性能世界那一屋子。
 * 内容照性能世界的那几种行（人话 · 思考 · 工具 · 子 agent · 带表格与代码块的结论），只取规模。
 */
export const HUGE_TURNS: number[] = [Number(new URLSearchParams(location.search).get("turns") ?? 9600), ...PERF_TURNS.slice(1)];

export function perfHugeWorld(): World {
  return perfMainWorld(HUGE_TURNS);
}
