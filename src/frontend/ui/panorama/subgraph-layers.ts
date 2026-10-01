/**
 * P7b（全景 P4，#79）：把**邻域 / 影响面**按距根的跳数分层列出来。
 *
 * # CP1：跳数不在这里算（「前端不算图」）
 *
 * 两种都是那台机器上算好的：影响面的 `depth` 是上游 `impact` 给的反向距离；邻域的 `depth` 是小程序
 * `neighborhood` op 按上游 `subgraph` 的 depth 口径给的。这里只做**呈现**：按 `depth` 分组、层内去重、每层截断并如实回报。
 * 〔墓碑 —— 此前邻域那一半在这里对 `subgraph` 的边做无向 BFS 自己算跳数。〕
 *
 * # 为什么是分层列表而不是一张点线图
 *
 * 分层列表对「谁调了谁」的可读性不输点线图，而点线图在几十个节点上就糊了。⇒ 零新依赖。
 *
 * # 为什么上界是本模块的正题之一
 *
 * 邻域是**双向**的 —— depth 每加一跳，节点数按扇出幂增；真实仓里一个被广泛调用的符号，depth=3 就可能上千个节点。
 * ⚠ 截断**必须说清**：`byte_cap_registry` 的 `ALLOWED_SEMANTICS` 里**没有「静默截断」这一项** ——
 * 截了不说，用户会把半份当成全部。所以回的是 `{ ids, truncated }` 而不是一个截好的数组。
 */

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
 * 按那台给的 `depth` 分层（影响面 `ImpactSet.affected` 与邻域 `Neighborhood.reached` 同形）。
 *
 * ⚠ 影响面与 `callers` **不是一件事**：`ImpactSet` 是**传递闭包**（反向可达的全部），`callers` 只有一跳。
 * · 根不进结果（它是圆心）；同一层重复的 id 只留一份；脏项丢掉；层按深度升序（不跟着输入顺序走）。
 */
export function layerByDepth(
  root: string,
  items: readonly { id: string; depth: number }[],
  maxPerLayer: number = MAX_PER_LAYER,
): Layer[] {
  const byDepth = new Map<number, string[]>();
  for (const a of items ?? []) {
    if (!a || typeof a.id !== "string" || !Number.isFinite(a.depth)) continue;
    if (a.id === root) continue;
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
