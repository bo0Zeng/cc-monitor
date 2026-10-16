/**
 * 读会话台架的合成记录：一份大记录 ＋ 一份小记录，按 Claude Code **盘上原始格式**落在
 * `<HOME>/.claude/projects/<目录>/<sid>.jsonl`。正文全是模板占位字，不取任何真会话（只造结构）。
 *
 * 大记录的形状（缺省值）：约 394 MB · 9 204 轮 · 每 5 轮一处回退重说（主线外那一支 ≈ 1 840 处分叉）·
 * 每轮一条排队打字（`queue-operation` 的 enqueue ＋ remove）· 每 1 000 轮一个约 1.38 MB 的工具结果（最长那一行）。
 * 同一个种子造出逐字节相同的记录。
 *
 * 用法：node make-big.mjs <HOME> [--mb 394] [--turns 9204] [--small-mb 27] [--seed 1]
 * 输出一行 JSON：两份记录的路径 · sid · 字节 · 行数 · 轮数 · 分叉数。
 */
import { closeSync, mkdirSync, openSync, writeSync } from "node:fs";
import { createHash } from "node:crypto";
import { join } from "node:path";

const home = process.argv[2];
if (!home) throw new Error("usage: make-big.mjs <HOME> [--mb N] [--turns N] [--small-mb N] [--seed N]");
const opt = (k, d) => {
  const i = process.argv.indexOf(k);
  return i > 0 ? Number(process.argv[i + 1]) : d;
};
const MB = opt("--mb", 394);
const TURNS = opt("--turns", 9204);
const SMALL_MB = opt("--small-mb", 27);
const SEED = opt("--seed", 1);

function uid(...k) {
  const h = createHash("md5").update(k.map(String).join("/")).digest("hex");
  return `${h.slice(0, 8)}-${h.slice(8, 12)}-4${h.slice(13, 16)}-8${h.slice(17, 20)}-${h.slice(20, 32)}`;
}

/** 占位的「代码」：`n` 行，按 `s` 换着写。 */
function code(n, s) {
  const out = [];
  for (let i = 0; i < n; i++) {
    const k = (s * 31 + i * 7) % 97;
    out.push(
      [
        `export function step${k}(input: Item[], limit = ${k + 3}): Result<Page> {`,
        `  const picked = input.filter((x) => x.weight > ${k} && x.tag !== "skip-${k}");`,
        `  if (picked.length === 0) return { ok: false, reason: "empty-${k}" };`,
        `  // 第 ${k} 段：按权重排序后截断到 limit，保持稳定序`,
        `  return { ok: true, value: picked.sort((a, b) => b.weight - a.weight).slice(0, limit) };`,
        "}",
      ][i % 6],
    );
  }
  return out.join("\n");
}

const iso = (t) => new Date(t * 1000).toISOString().replace(/\.\d{3}Z$/, ".000Z");

/** 写一份记录：`turns` 轮，每轮读文件那一条结果补到每轮约 `perTurn` 字节。 */
function record(path, sid, cwd, turns, perTurn, seed) {
  const fd = openSync(path, "w");
  let bytes = 0;
  let lines = 0;
  let forks = 0;
  let parent = null;
  let t = 1759305600 + seed; // 2025-10-01
  const base = { isSidechain: false, userType: "external", cwd, sessionId: sid, version: "2.0.30", gitBranch: "main" };
  const ts = () => iso((t += 7));
  const put = (row) => {
    const s = JSON.stringify(row) + "\n";
    writeSync(fd, s);
    bytes += Buffer.byteLength(s);
    lines++;
  };
  const w = (o) => {
    put({ parentUuid: parent, ...base, ...o });
    parent = o.uuid;
  };
  let pad = 0;
  for (let turn = 0; turn < turns; turn++) {
    const before = bytes;
    const mod = `packages/mod-${turn % 23}`;
    // 回退重说：先写一轮会被丢掉的（人一句 ＋ 回一句），再从它之前那一点接着写这一轮 ⇒ 那一点分叉，旧的那一支在主线外。
    if (turn > 0 && turn % 5 === 0) {
      const at = parent;
      w({ type: "user", message: { role: "user", content: `第 ${turn} 轮（作废）：先别改 ${mod}。` }, uuid: uid(sid, turn, "dead-u"), timestamp: ts() });
      w({ type: "assistant", message: { id: `msg_${uid(sid, turn, "dead-m").slice(0, 24)}`, type: "message", role: "assistant", model: "claude-sonnet-4-5", content: [{ type: "text", text: "好的，先停。" }], stop_reason: "end_turn", stop_sequence: null, usage: { input_tokens: 5, output_tokens: 4 } }, uuid: uid(sid, turn, "dead-a"), timestamp: ts() });
      parent = at;
      forks++;
    }
    const said = `第 ${turn} 轮：把 ${mod} 里的分页逻辑改成游标分页，顺带补测试。`;
    w({ type: "user", message: { role: "user", content: said }, uuid: uid(sid, turn, "u"), timestamp: ts() });
    const mid = `msg_${uid(sid, turn, "m").slice(0, 24)}`;
    const usage = { input_tokens: 9, cache_creation_input_tokens: 1200, cache_read_input_tokens: 20000 + (turn % 400) * 300, output_tokens: 400, service_tier: "standard" };
    const asst = (block, k, stop = null) =>
      w({ type: "assistant", message: { id: mid, type: "message", role: "assistant", model: "claude-sonnet-4-5", content: [block], stop_reason: stop, stop_sequence: null, usage }, requestId: `req_${uid(sid, turn, "r").slice(0, 24)}`, uuid: uid(sid, turn, "a", k), timestamp: ts() });
    asst({ type: "thinking", thinking: `先看 ${mod} 现在的分页实现与调用点。第 ${turn} 轮。`, signature: "sig" + "x".repeat(200) }, 0);
    asst({ type: "text", text: `我先看一下 ${mod} 的现状。` }, 1);
    // 这一轮跑着时人又打了一句（排队）：enqueue 一条、被取走时 remove 一条。
    const queued = `顺便看看 ${mod} 的文档（第 ${turn} 轮排队）`;
    put({ type: "queue-operation", operation: "enqueue", timestamp: ts(), sessionId: sid, content: queued });
    const readBody = code(40 + pad, turn);
    const bashOut = turn % 1000 === 999 ? "x".repeat(1_380_000) : Array.from({ length: 12 }, (_, k) => `  ✓ ${mod} case ${k} (${k * 3} ms)`).join("\n");
    const tools = [
      ["Grep", { pattern: `paginate${turn % 9}`, path: mod }, Array.from({ length: 6 }, (_, k) => `${mod}/src/file${k}.ts:${10 + k * 7}:  paginate${turn % 9}(items, ${k})`).join("\n")],
      ["Read", { file_path: `${cwd}/${mod}/src/page.ts` }, readBody],
      ["Edit", { file_path: `${cwd}/${mod}/src/page.ts`, old_string: code(8, turn), new_string: code(12, turn + 1) }, "The file has been updated."],
      ["Bash", { command: `npm test -- ${mod}`, description: "跑测试" }, bashOut],
    ];
    tools.forEach(([name, input, out], j) => {
      const tid = `toolu_${uid(sid, turn, "t", j).slice(0, 24)}`;
      asst({ type: "tool_use", id: tid, name, input }, 2 + j);
      w({ type: "user", message: { role: "user", content: [{ tool_use_id: tid, type: "tool_result", content: out, is_error: false }] }, uuid: uid(sid, turn, "tr", j), timestamp: ts() });
    });
    put({ type: "queue-operation", operation: "remove", timestamp: ts(), sessionId: sid, content: queued });
    asst({ type: "text", text: `改好了：${mod} 改成游标分页，测试全过。` }, 9, "end_turn");
    // 第一轮量出不补时多大，之后照它补到每轮约 perTurn（每行代码约 70 字节）。
    if (turn === 0) pad = Math.max(0, Math.round((perTurn - (bytes - before)) / 70));
  }
  closeSync(fd);
  return { path, sid, bytes, lines, turns, forks };
}

const slug = (cwd) => cwd.replace(/[^A-Za-z0-9]/g, "-");
const out = [];
for (const [tag, mb, turns] of [
  ["big", MB, TURNS],
  ["small", SMALL_MB, Math.max(1, Math.round((TURNS * SMALL_MB) / MB))],
]) {
  const cwd = `/home/user/work/read-${tag}`;
  const dir = join(home, ".claude", "projects", slug(cwd));
  mkdirSync(dir, { recursive: true });
  const sid = uid("read", SEED, tag);
  out.push({ tag, cwd, ...record(join(dir, `${sid}.jsonl`), sid, cwd, turns, (mb * 1e6) / turns, SEED) });
}
mkdirSync(join(home, ".claude", "sessions"), { recursive: true });
console.log(JSON.stringify(out));
