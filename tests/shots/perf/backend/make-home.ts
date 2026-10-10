/**
 * 后端台架的合成家目录：按 Claude Code **盘上原始格式**落 `.claude/projects/<目录>/<sid>.jsonl`（type user / assistant ·
 * message.content 块 · tool_use / tool_result · toolUseResult），真后端从头解析、出成品 —— 量的是它真读盘的那一份活。
 * 正文全是模板占位，不取任何真会话（口径同网络层调研 10-10 的 `netprobe/gen.py`）。
 *
 * 一份 24 个会话，大小照原先那屋子会话（几份 5–8 MB 的长会话 ＋ 一长串几十到几百 KB 的）；按 `--scale` 复制成几十上百个（不同目录、不同 sid）。
 * 用法：tsx make-home.ts <HOME> [--scale N] [--seed N]
 */
import { closeSync, mkdirSync, openSync, writeSync } from "node:fs";
import { createHash } from "node:crypto";
import { join } from "node:path";

const home = process.argv[2];
if (!home) throw new Error("usage: make-home.ts <HOME> [--scale N] [--seed N]");
const opt = (k: string, d: number): number => {
  const i = process.argv.indexOf(k);
  return i > 0 ? Number(process.argv[i + 1]) : d;
};
const scale = opt("--scale", 4);
const seed = opt("--seed", 1);

/** 一份里各会话的目标字节数。 */
const SIZES = [8_344_207, 7_552_758, 5_960_260, 5_165_069, 731_475, 609_167, 548_510, 487_681, 426_439, 364_982, 340_565, 267_458, 243_908, 218_963, 194_536, 182_658, 169_755, 145_884, 134_115, 122_237, 108_428, 96_519, 84_768, 72_879];

/** 定种子的伪随机（mulberry32）：同一个种子造出逐字节相同的家目录。 */
function rng(s: number): (lo: number, hi: number) => number {
  let a = s >>> 0;
  return (lo, hi) => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    const x = ((t ^ (t >>> 14)) >>> 0) / 4294967296;
    return lo + Math.floor(x * (hi - lo + 1));
  };
}

function uid(...k: unknown[]): string {
  const h = createHash("md5").update(k.map(String).join("/")).digest("hex");
  return `${h.slice(0, 8)}-${h.slice(8, 12)}-4${h.slice(13, 16)}-8${h.slice(17, 20)}-${h.slice(20, 32)}`;
}

function code(n: number, s: number): string {
  const out: string[] = [];
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

const iso = (t: number): string => new Date(t * 1000).toISOString().replace(/\.\d{3}Z$/, ".000Z");

function session(path: string, sid: string, cwd: string, target: number, r: (lo: number, hi: number) => number): number {
  const fd = openSync(path, "w");
  let n = 0;
  let parent: string | null = null;
  let t = 1759305600; // 2025-10-01
  let turn = 0;
  const base = { isSidechain: false, userType: "external", cwd, sessionId: sid, version: "2.0.30", gitBranch: "main" };
  const ts = (): string => {
    t += r(2, 40);
    return iso(t);
  };
  const w = (o: Record<string, unknown>): void => {
    const row = { parentUuid: parent, ...base, ...o };
    parent = o.uuid as string;
    const s = JSON.stringify(row) + "\n";
    writeSync(fd, s);
    n += Buffer.byteLength(s);
  };
  while (n < target) {
    const mod = `packages/mod-${turn % 23}`;
    const heavy = turn % 5 === 0;
    w({ type: "user", message: { role: "user", content: `第 ${turn} 轮：把 ${mod} 里的分页逻辑改成游标分页，顺带补测试。` }, uuid: uid(sid, turn, "u"), timestamp: ts() });
    const mid = `msg_${uid(sid, turn, "m").slice(0, 24)}`;
    const usage = { input_tokens: r(3, 20), cache_creation_input_tokens: r(200, 4000), cache_read_input_tokens: 20000 + turn * 300, output_tokens: r(100, 900), service_tier: "standard" };
    const asst = (block: Record<string, unknown>, k: number): void =>
      w({ type: "assistant", message: { id: mid, type: "message", role: "assistant", model: "claude-sonnet-4-5", content: [block], stop_reason: null, stop_sequence: null, usage }, requestId: `req_${uid(sid, turn, "r").slice(0, 24)}`, uuid: uid(sid, turn, "a", k), timestamp: ts() });
    asst({ type: "thinking", thinking: `先看 ${mod} 现在的分页实现与调用点，再决定游标放在哪一层。第 ${turn} 轮。`, signature: "sig" + "x".repeat(300) }, 0);
    asst({ type: "text", text: `我先看一下 ${mod} 的现状。` }, 1);
    const tools: [string, Record<string, unknown>, string, "read" | "edit" | null][] = [
      ["Grep", { pattern: `paginate${turn % 9}`, path: mod }, Array.from({ length: 6 }, (_, k) => `${mod}/src/file${k}.ts:${10 + k * 7}:  paginate${turn % 9}(items, ${k})`).join("\n"), null],
      ["Read", { file_path: `${cwd}/${mod}/src/page.ts` }, code(heavy ? 120 : 60, turn).split("\n").map((l, i) => `${String(i + 1).padStart(6)}\t${l}`).join("\n"), "read"],
      ["Edit", { file_path: `${cwd}/${mod}/src/page.ts`, old_string: code(heavy ? 18 : 8, turn), new_string: code(heavy ? 26 : 12, turn + 1) }, "The file has been updated.", "edit"],
      ["Bash", { command: `npm test -- ${mod}`, description: "跑测试" }, Array.from({ length: heavy ? 30 : 12 }, (_, k) => `  ✓ ${mod} case ${k} (${k * 3} ms)`).join("\n") + "\n\nTests: all passed", null],
    ];
    tools.forEach(([name, input, out, kind], j) => {
      const tid = `toolu_${uid(sid, turn, "t", j).slice(0, 24)}`;
      asst({ type: "tool_use", id: tid, name, input }, 2 + j);
      let tur: Record<string, unknown> | null = name === "Bash" ? { stdout: out, stderr: "", interrupted: false } : null;
      if (kind === "read") {
        const lines = out.split("\n").length;
        tur = { type: "text", file: { filePath: input.file_path, content: out, numLines: lines, startLine: 1, totalLines: lines } };
      }
      if (kind === "edit") {
        const o = String(input.old_string).split("\n");
        const nw = String(input.new_string).split("\n");
        tur = { filePath: input.file_path, oldString: input.old_string, newString: input.new_string, originalFile: code(60, turn), structuredPatch: [{ oldStart: 1, oldLines: 8, newStart: 1, newLines: 12, lines: [...o.map((l) => "-" + l), ...nw.map((l) => "+" + l)] }], userModified: false, replaceAll: false };
      }
      w({ type: "user", message: { role: "user", content: [{ tool_use_id: tid, type: "tool_result", content: out, is_error: false }] }, uuid: uid(sid, turn, "tr", j), timestamp: ts(), ...(tur ? { toolUseResult: tur } : {}) });
    });
    asst({ type: "text", text: `改好了：${mod} 改成游标分页，测试全过。\n\n\`\`\`ts\n${code(heavy ? 24 : 10, turn + 2)}\n\`\`\`\n` }, 9);
    turn++;
  }
  closeSync(fd);
  return n;
}

const r = rng(seed);
const slug = (cwd: string): string => cwd.replace(/[^A-Za-z0-9]/g, "-");
let files = 0;
let bytes = 0;
for (let copy = 0; copy < scale; copy++) {
  SIZES.forEach((size, i) => {
    const cwd = `/home/user/work/proj-${i}` + (copy === 0 ? "" : `-c${copy}`);
    const dir = join(home, ".claude", "projects", slug(cwd));
    mkdirSync(dir, { recursive: true });
    const sid = uid("sess", seed, copy, i);
    bytes += session(join(dir, `${sid}.jsonl`), sid, cwd, size, r);
    files++;
  });
}
mkdirSync(join(home, ".claude", "sessions"), { recursive: true });
console.log(JSON.stringify({ files, mb: +(bytes / 1e6).toFixed(1) }));
