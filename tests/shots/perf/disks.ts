/**
 * 伺服那一侧造性能世界的盘（`World.diskFrom` 那张配方）：在 node 里照配方连记录正文一起造（`world.ts::fullWorld`），出各台家目录的原始文件，
 * 另交每个会话接着往下写要的那一截（页里实时来消息那几项接着写，父链 · 号段接得上）。
 * 配方带 `big`（最长那条几十万行）⇒ 那一份由 `backend/make-big.mjs` 流式写进系统临时目录（`TMPDIR`），
 * 读回成字节交给池子（池子照字节落盘）、临时那份当场删；接着写的那一截照 make-big 交回的末一条接。
 * 由 `serve.mjs` 经 vite 的 SSR 载入；页里不载它。
 */
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { MachineDisk } from "../disk";
import { jsonlOf, type ConvoTail } from "../fake/records";
import { diskOf } from "../fake/world";
import { fullWorld } from "./world";
// @ts-expect-error 台架脚本是 .mjs、没有类型
import { record } from "./backend/make-big.mjs";

export interface RecipeDisk {
  machines: Record<string, MachineDisk>;
  tails: Record<string, ConvoTail>;
}

/** 照配方造盘；造的那一刻钟拨在 `now`（页开头拨的那一刻：盘上的时刻与页里、后端那一侧同一个起点）。 */
export function recipeDisk(recipe: string, now = Date.now()): RecipeDisk {
  const real = Date.now;
  Date.now = () => now;
  try {
    const w = fullWorld(recipe);
    const tails = Object.fromEntries(w.sessions.flatMap((s) => (s.convo ? [[s.sid, s.convo.tail()] as const] : [])));
    const machines = diskOf(w);
    const { big, table } = JSON.parse(recipe) as { big?: number; table: number[] };
    if (big) {
      const s = w.sessions[0];
      const turns = table[0];
      const dir = mkdtempSync(join(tmpdir(), "perf-big-"));
      try {
        const made = record(join(dir, "big.jsonl"), s.sid, s.cwd, turns, (big * 1e6) / turns, 1) as { parent: string; t: number };
        const rel = jsonlOf(s.cwd, s.sid);
        const m = machines[s.origin];
        // 池子落盘照 `writeFileSync`：字节照收（一个串装不下这一份）。
        (m.files as Record<string, string | Buffer>)[rel] = readFileSync(join(dir, "big.jsonl"));
        m.mtimes = { ...m.mtimes, [rel]: made.t };
        tails[s.sid] = { sid: s.sid, cwd: s.cwd, model: "claude-sonnet-4-5", agentId: null, t: made.t, toolN: 0, parent: made.parent, uid: 0 };
      } finally {
        rmSync(dir, { recursive: true, force: true });
      }
    }
    return { machines, tails };
  } finally {
    Date.now = real;
  }
}
