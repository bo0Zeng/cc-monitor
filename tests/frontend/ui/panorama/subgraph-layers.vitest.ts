// P7b（全景 P4，#79）：邻域 / 影响面分层的**纯**那半。
// 〔PANO · CP1〕跳数由那台给（影响面：上游 `impact`；邻域：小程序 `neighborhood`，判据住 `tests/panorama-engine/cli_tests.rs`），
// 这里只钉「按给的 depth 分组」这一层的呈现规则。
import { describe, it, expect } from "vitest";
import { clampDepth, layerByDepth, MAX_DEPTH, MAX_PER_LAYER } from "../../../../src/frontend/ui/panorama/subgraph-layers";

describe("P7b clampDepth", () => {
  it("★ P7b-Y2：depth 只给 1–3，脏值收拾掉", () => {
    expect(clampDepth(0)).toBe(1);
    expect(clampDepth(-5)).toBe(1);
    expect(clampDepth(99)).toBe(MAX_DEPTH);
    expect(clampDepth(2.7)).toBe(2);
    expect(clampDepth(Number.NaN)).toBe(1);
  });
});

describe("P7b layerByDepth", () => {
  it("★ P7b-Y3：按给的深度分层（影响面是**传递闭包**，不是 callers 的同义词）", () => {
    const ls = layerByDepth("r", [
      { id: "a", depth: 1 },
      { id: "b", depth: 2 },
      { id: "c", depth: 1 },
      { id: "d", depth: 3 },
    ]);
    // 深度 > 1 的层必须真的分出来 —— 只有一跳的话它就退化成 callers 了。
    expect(ls.map((l) => l.depth)).toEqual([1, 2, 3]);
    expect([...ls[0].ids].sort()).toEqual(["a", "c"]);
    expect(ls[1].ids).toEqual(["b"]);
    expect(ls[2].ids).toEqual(["d"]);
  });

  it("★ P7b-Y3b：层要**按深度排序**，别跟着输入顺序走", () => {
    // ⚠ 变异逼出来的：depth 恰好按序插入时 `Map` 的插入序天然就是 1,2,3 ⇒ 那行 `.sort()` 不承重。
    const ls = layerByDepth("r", [
      { id: "d3", depth: 3 },
      { id: "d1", depth: 1 },
      { id: "d2", depth: 2 },
    ]);
    expect(ls.map((l) => l.depth)).toEqual([1, 2, 3]);
    expect(ls.map((l) => l.ids[0])).toEqual(["d1", "d2", "d3"]);
  });

  it("根不进结果；重复 id 只留一份；脏项丢掉；空 ⇒ 空层", () => {
    const ls = layerByDepth("r", [
      { id: "r", depth: 1 },
      { id: "a", depth: 1 },
      { id: "a", depth: 1 },
      { id: "x", depth: Number.NaN },
      null as unknown as { id: string; depth: number },
    ]);
    expect(ls).toEqual([{ depth: 1, ids: ["a"], truncated: 0 }]);
    expect(layerByDepth("r", [])).toEqual([]);
  });

  it("★ P7b-Y2：超上界要**截断并如实回报还剩多少**", () => {
    const many = Array.from({ length: MAX_PER_LAYER + 3 }, (_, i) => ({ id: `n${i}`, depth: 1 }));
    const ls = layerByDepth("r", many);
    // ⚠ 截了不说，用户会把半份当成全部 —— 所以回的是 `{ids, truncated}` 不是一个截好的数组。
    expect(ls[0].ids).toHaveLength(MAX_PER_LAYER);
    expect(ls[0].truncated).toBe(3);
  });
});
