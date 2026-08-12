// P7b（全景 P4，#79）：调用子图分层的**纯**那半。
import { describe, it, expect } from "vitest";
import {
  clampDepth,
  layerImpact,
  layerSubGraph,
  MAX_DEPTH,
  MAX_PER_LAYER,
} from "./subgraph-layers";
import type { Edge, ImpactSet, SubGraph } from "./types";

const e = (from: string, to: string): Edge => ({
  from,
  to,
  kind: "Calls",
  call_site_line: null,
  confidence: "Exact",
});
const sg = (edges: Edge[]): SubGraph => ({ symbols: [], edges });

describe("P7b layerSubGraph", () => {
  it("★ P7b-Y1：按跳数分层，根不进任何一层，每个 id 只出现在最短那层", () => {
    // r → a → b；r → c（c 也连着 b：b 仍该算第 2 跳，因为 BFS 取最短）
    const g = sg([e("r", "a"), e("a", "b"), e("r", "c"), e("c", "b")]);
    const ls = layerSubGraph(g, "r");
    expect(ls.map((l) => l.depth)).toEqual([1, 2]);
    expect([...ls[0].ids].sort()).toEqual(["a", "c"]);
    expect(ls[1].ids).toEqual(["b"]);
    // 根是圆心，不是结果。
    expect(ls.flatMap((l) => l.ids)).not.toContain("r");
  });

  it("★ P7b-Y1b：边按**无向**处理（`subgraph` 是双向邻域）", () => {
    // 只有 a→r 这一条：r 的邻域里仍该有 a。
    const ls = layerSubGraph(sg([e("a", "r")]), "r");
    expect(ls[0].ids).toEqual(["a"]);
  });

  it("★ P7b-Y2：超上界要**截断并如实回报还剩多少**", () => {
    const edges = Array.from({ length: MAX_PER_LAYER + 7 }, (_, i) => e("r", `n${i}`));
    const ls = layerSubGraph(sg(edges), "r");
    expect(ls[0].ids).toHaveLength(MAX_PER_LAYER);
    // ⚠ 截了不说，用户会把半份当成全部 —— 所以回的是 `{ids, truncated}` 不是一个截好的数组。
    expect(ls[0].truncated).toBe(7);
  });

  it("★ P7b-Y1c：截断**不许改变图的形状** —— 第 51 个邻居的下游仍要出现", () => {
    // ⚠ 本条是**变异逼出来的**：只测「一层超上界」时，把 BFS 的推进换成截断后的结果，
    // 判据照样全绿 —— 因为那份夹具根本没有第二层。
    // 截断是**显示**的事；拿它推进 BFS，第 51 个邻居的下游会整片消失，且消失得毫无痕迹。
    const edges = Array.from({ length: MAX_PER_LAYER + 1 }, (_, i) => e("r", `n${i}`));
    const lastId = `n${MAX_PER_LAYER}`; // 恰好被截掉的那个
    edges.push(e(lastId, "deep"));
    const ls = layerSubGraph(sg(edges), "r");
    expect(ls[0].ids).toHaveLength(MAX_PER_LAYER);
    expect(ls[0].truncated).toBe(1);
    expect(ls.map((l) => l.depth), "第二跳必须还在").toEqual([1, 2]);
    expect(ls[1].ids).toEqual(["deep"]);
  });

  it("孤立根 ⇒ 空层，不是崩", () => {
    expect(layerSubGraph(sg([]), "r")).toEqual([]);
  });

  it("脏边不许把分层带偏", () => {
    const dirty = { symbols: [], edges: [null, { from: 1 }, e("r", "a")] } as unknown as SubGraph;
    expect(layerSubGraph(dirty, "r")[0].ids).toEqual(["a"]);
  });
});

describe("P7b clampDepth", () => {
  it("★ P7b-Y2：depth 只给 1–3，脏值收拾掉", () => {
    expect(clampDepth(0)).toBe(1);
    expect(clampDepth(-5)).toBe(1);
    expect(clampDepth(99)).toBe(MAX_DEPTH);
    expect(clampDepth(2.7)).toBe(2);
    expect(clampDepth(Number.NaN)).toBe(1);
  });
});

describe("P7b layerImpact", () => {
  const set = (affected: Array<{ id: string; depth: number }>): ImpactSet => ({
    root: "r",
    affected,
  });

  it("★ P7b-Y3：影响面是**传递闭包**，按反向距离分层（不是 callers 的同义词）", () => {
    const ls = layerImpact(
      set([
        { id: "a", depth: 1 },
        { id: "b", depth: 2 },
        { id: "c", depth: 1 },
        { id: "d", depth: 3 },
      ]),
    );
    // 深度 > 1 的层必须真的分出来 —— 只有一跳的话它就退化成 callers 了。
    expect(ls.map((l) => l.depth)).toEqual([1, 2, 3]);
    expect([...ls[0].ids].sort()).toEqual(["a", "c"]);
    expect(ls[1].ids).toEqual(["b"]);
    expect(ls[2].ids).toEqual(["d"]);
  });

  it("★ P7b-Y3b：层要**按深度排序**，别跟着输入顺序走", () => {
    // ⚠ 也是变异逼出来的：上一条夹具的 depth 恰好按序插入（1,2,1,3），
    // `Map` 的插入序天然就是 1,2,3 ⇒ 那行 `.sort()` 不承重，判据看不出它没了。
    const ls = layerImpact(
      set([
        { id: "d3", depth: 3 },
        { id: "d1", depth: 1 },
        { id: "d2", depth: 2 },
      ]),
    );
    expect(ls.map((l) => l.depth)).toEqual([1, 2, 3]);
    expect(ls.map((l) => l.ids[0])).toEqual(["d1", "d2", "d3"]);
  });

  it("根不进结果；重复 id 只留一份；脏项丢掉", () => {
    const ls = layerImpact(
      set([
        { id: "r", depth: 1 },
        { id: "a", depth: 1 },
        { id: "a", depth: 1 },
        { id: "x", depth: Number.NaN },
      ]),
    );
    expect(ls).toEqual([{ depth: 1, ids: ["a"], truncated: 0 }]);
  });

  it("★ P7b-Y2：影响面同样有上界且如实回报", () => {
    const many = Array.from({ length: MAX_PER_LAYER + 3 }, (_, i) => ({
      id: `n${i}`,
      depth: 1,
    }));
    const ls = layerImpact(set(many));
    expect(ls[0].ids).toHaveLength(MAX_PER_LAYER);
    expect(ls[0].truncated).toBe(3);
  });
});
