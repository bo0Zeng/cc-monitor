/**
 * 签收流（稿 03 第 12 张）与顶块的退回对象：期望手写，夹具是合成的中性例子。
 */
import { describe, expect, it, vi } from "vitest";

vi.mock("../../../src/frontend/ui/copy-table", () => ({ copyText: (k: string) => k }));

import { signsNewestFirst } from "../../../src/frontend/ui/views/plan-model.ts";
import { topTarget, cellTarget } from "../../../src/frontend/ui/views/plan-review.ts";
import type { PlanCell, PlanNeed, PlanSlice, PlanWho } from "../../../src/frontend/ui/plan-reads.ts";

const who = (sid: string): PlanWho => ({ id: sid, kind: "session", sid, alive: true, activity: "idle", needs: null }) as PlanWho;
const sign = (at: string, reason: string) => ({ at, atText: at, by: who("s1"), reason, refs: [] });
const cell = (id: string, title: string, signs: ReturnType<typeof sign>[], extra: Partial<PlanCell> = {}): PlanCell =>
  ({ id, title, kind: "模块", parent: null, children: [], signs, statusCode: "done", owner: who("s2"), signer: null, ...extra }) as unknown as PlanCell;

const slice = {
  name: "alpha",
  done: false,
  top: ["A1", "A2"],
  blocks: [{ id: "project", cells: ["project"], owner: who("lead") }],
  cells: [
    cell("A1", "读入模块", [sign("2026-01-01T00:00:00Z", "第一次"), sign("2026-01-03T00:00:00Z", "再签")]),
    cell("A2", "写出模块", [sign("2026-01-02T00:00:00Z", "写完")]),
    cell("A3", "空着", []),
  ],
} as unknown as PlanSlice;

describe("签收流：整片排成一条、新的在上、按标题找", () => {
  it("全片的签收按时刻倒排（同一格签两次两条都在）", () => {
    expect(signsNewestFirst(slice).map((x) => `${x.cell.id}:${x.sign.reason}`)).toEqual(["A1:再签", "A2:写完", "A1:第一次"]);
  });
  it("按标题找只留那几格的签收；对不上 ⇒ 空", () => {
    expect(signsNewestFirst(slice, "读入").map((x) => x.sign.reason)).toEqual(["再签", "第一次"]);
    expect(signsNewestFirst(slice, "没有这个")).toEqual([]);
  });
});

describe("退回对象：顶块与格", () => {
  it("顶块：编号是那一块的 id、标题是片名、送给顶块接手、没有签它的那一项、框叫「退回」", () => {
    const need = { key: "top:r", kind: "top", block: "project", cell: null, sid: null, acked: false } as unknown as PlanNeed;
    const t = topTarget(slice, need);
    expect(t).toMatchObject({ id: "project", title: "alpha", done: true, signer: false });
    expect(t?.owner?.sid).toBe("lead");
    expect(topTarget(slice, { ...need, block: "B" })).toBeNull();
  });
  it("格：签它的照 pb 给的（没给 ⇒ null，不是 false）", () => {
    const t = cellTarget(slice, slice.cells[1]);
    expect(t).toMatchObject({ id: "A2", title: "写出模块", done: true, signer: null });
  });
});
