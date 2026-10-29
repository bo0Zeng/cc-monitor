/**
 * 伺服那一侧造性能世界的盘（`World.diskFrom` 那张配方）：在 node 里照配方连记录正文一起造（`world.ts::fullWorld`），出各台家目录的原始文件，
 * 另交每个会话接着往下写要的那一截（页里实时来消息那几项接着写，父链 · 号段接得上）。
 * 由 `serve.mjs` 经 vite 的 SSR 载入；页里不载它。
 */
import type { MachineDisk } from "../disk";
import type { ConvoTail } from "../fake/records";
import { diskOf } from "../fake/world";
import { fullWorld } from "./world";

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
    return { machines: diskOf(w), tails };
  } finally {
    Date.now = real;
  }
}
