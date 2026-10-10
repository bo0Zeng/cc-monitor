/**
 * 后端台架的合成家目录：把 `perf/world.ts` 那一屋子会话按 Claude Code 落盘的样子写成 `.claude/projects/<目录>/<sid>.jsonl`，
 * 再按 `--scale` 复制成几十上百个会话（不同目录、不同 sid）。正文全是模板占位，不取任何真会话。
 * 用法：tsx make-home.ts <HOME> [--scale N]
 */
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { perfWorld } from "../world";

const home = process.argv[2];
if (!home) throw new Error("usage: make-home.ts <HOME> [--scale N]");
const si = process.argv.indexOf("--scale");
const scale = si > 0 ? Number(process.argv[si + 1]) : 4;

// 解析之后才有的那几格（产品成品），原文里没有 ⇒ 落盘前去掉。
const DERIVED = ["timeText", "userText", "toolResults", "toolCards", "childRuns", "apiReason", "toolSteps"];
const slug = (cwd: string) => cwd.replace(/[^A-Za-z0-9]/g, "-");

const w = perfWorld();
let files = 0;
let bytes = 0;
for (let copy = 0; copy < scale; copy++) {
  for (const s of w.sessions) {
    const cwd = copy === 0 ? s.cwd : `${s.cwd}-c${copy}`;
    const sid = copy === 0 ? s.sid : s.sid.replace(/^(.{8})/, (m) => (parseInt(m, 16) ^ (copy << 20)).toString(16).padStart(8, "0"));
    const dir = join(home, ".claude", "projects", slug(cwd));
    mkdirSync(dir, { recursive: true });
    const text = s.records
      .map((r) => {
        const o: Record<string, unknown> = { ...(r as Record<string, unknown>) };
        for (const k of DERIVED) delete o[k];
        if ("sessionId" in o) o.sessionId = sid;
        if ("cwd" in o) o.cwd = cwd;
        return JSON.stringify(o);
      })
      .join("\n") + "\n";
    writeFileSync(join(dir, `${sid}.jsonl`), text);
    files++;
    bytes += text.length;
  }
}
mkdirSync(join(home, ".claude", "sessions"), { recursive: true });
console.log(JSON.stringify({ files, mb: +(bytes / 1e6).toFixed(1) }));
