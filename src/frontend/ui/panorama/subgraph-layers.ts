/**
 * P7b（全景 P4，#79）：把**调用子图**按距根的跳数分层。
 *
 * # 为什么是分层列表而不是一张点线图
 *
 * 「子图」最容易被读成「画一张图」。那要引一套布局（力导向 / 分层），
 * 而全景界面今天全是列表与分节。分层列表对「谁调了谁」的可读性不输点线图，
 * 而点线图在几十个节点上就糊了。⇒ 零新依赖，且真要画图时本模块不挡路。
 *
 * # 为什么上界是本模块的正题之一
 *
 * `subgraph` 是**双向邻域** —— depth 每加一跳，节点数按扇出幂增；
 * 真实仓里一个被广泛调用的符号，depth=3 就可能上千个节点。
 *
 * ⚠ 截断**必须说清**。本仓的 `byte_cap_registry` 对每一处上限逐条追问「超限怎么办」，
 * 而它的 `ALLOWED_SEMANTICS` 里**没有「静默截断」这一项** —— 截了不说，
 * 用户会把半份当成全部。所以本模块回的是 `{ ids, truncated }` 而不是一个截好的数组。
 */
import type { Edge, ImpactSet, SubGraph } from "./types";

/** 每层最多渲染多少条。超出的数目如实回给调用方去说。 */
export const MAX_PER_LAYER = 50;
/** UI 只给 1–3 跳（见模块头注的扇出爆炸）。 */
export const MAX_DEPTH = 3;

export interface Layer {
  /** 距根几跳（1 = 直接邻居）。 */
  depth: number;
  ids: string[];
  /** 这一层**没显示**的条数（0 = 全显示了）。 */
  truncated: number;
}

/** 把 depth 夹到 UI 允许的范围。非整数/越界都收拾掉，不抛。 */
export function clampDepth(d: number): number {
  if (!Number.isFinite(d)) return 1;
  return Math.min(MAX_DEPTH, Math.max(1, Math.trunc(d)));
}

/**
 * BFS 分层。
 *
 * · 边**按无向处理** —— `subgraph` 是双向邻域，一条 `a→b` 既让 b 成为 a 的邻居，
 *   也让 a 成为 b 的邻居（这正是「以某符号为心的邻域」的含义）。
 * · **根不出现在任何一层里** —— 它是圆心，不是结果。
 * · 每个 id 只出现在**最短**的那一层（BFS 天然保证）。
 */
export function layerSubGraph(
  sg: SubGraph,
  root: string,
  maxPerLayer: number = MAX_PER_LAYER,
): Layer[] {
  const adj = new Map<string, string[]>();
  const push = (a: string, b: string): void => {
    const cur = adj.get(a);
    if (cur) cur.push(b);
    else adj.set(a, [b]);
  };
  for (const e of sg.edges as Edge[]) {
    if (!e || typeof e.from !== "string" || typeof e.to !== "string") continue;
    push(e.from, e.to);
    push(e.to, e.from);
  }
  const seen = new Set<string>([root]);
  const layers: Layer[] = [];
  let frontier = [root];
  while (frontier.length) {
    const next: string[] = [];
    for (const id of frontier) {
      for (const nb of adj.get(id) ?? []) {
        if (seen.has(nb)) continue;
        seen.add(nb);
        next.push(nb);
      }
    }
    if (!next.length) break;
    layers.push({
      depth: layers.length + 1,
      ids: next.slice(0, maxPerLayer),
      truncated: Math.max(0, next.length - maxPerLayer),
    });
    // ★ **下一轮从完整的 `next` 展开，不是从截断后的 `ids`**。
    // 截断是**显示**的事；拿它去推进 BFS 会让图的形状随一个渲染上限而变
    //（第 51 个邻居的下游整片消失，而且消失得毫无痕迹）。
    //
    // ⚠ 第一版**整句忘了写** —— `frontier` 永远是 `[root]`，于是只出得来一层。
    // 判据当场逮到（`expected [1] to deeply equal [1, 2]`）。
    frontier = next;
  }
  return layers;
}

/**
 * P7b-Y3：影响面（blast radius）分层。
 *
 * ⚠ 它与 `callers` **不是一件事**：`ImpactSet` 是**传递闭包**（反向可达的全部），
 * `callers` 只有一跳。把它做成 callers 的同义词是本件最容易犯的错。
 */
export function layerImpact(set: ImpactSet, maxPerLayer: number = MAX_PER_LAYER): Layer[] {
  const byDepth = new Map<number, string[]>();
  for (const a of set.affected ?? []) {
    if (!a || typeof a.id !== "string" || !Number.isFinite(a.depth)) continue;
    if (a.id === set.root) continue; // 根不进结果，同 `layerSubGraph`
    const d = Math.trunc(a.depth);
    const cur = byDepth.get(d);
    if (cur) {
      if (!cur.includes(a.id)) cur.push(a.id);
    } else byDepth.set(d, [a.id]);
  }
  return [...byDepth.keys()]
    .sort((x, y) => x - y)
    .map((depth) => {
      const all = byDepth.get(depth)!;
      return {
        depth,
        ids: all.slice(0, maxPerLayer),
        truncated: Math.max(0, all.length - maxPerLayer),
      };
    });
}
