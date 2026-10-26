/**
 * 大纲左边的圈槽（covers 画圈不连线）：照此刻看得见的行摆，一条规矩固定一列，一段起头带上钩、末行带下钩；
 * 选中写了 covers 的那一格 ⇒ 它那一圈亮、别的淡、圈住的行铺圈色。期望手写，夹具是合成的中性例子。
 */
import { describe, expect, it } from "vitest";

import { ringLanes, ringsOf, ringRules } from "../../../../src/frontend/ui/views/plan-model.ts";
import type { PlanCell, PlanSlice } from "../../../../src/frontend/ui/plan-reads.ts";

const cell = (id: string, extra: Partial<PlanCell> = {}): PlanCell => ({ id, children: [], covers: [], coveredBy: [], ...extra }) as unknown as PlanCell;

/** 两条规矩：R1 圈 A1（连 A1-1 · A1-2）与 A3；R2 圈 A1-2 与 A2。R9 只写 project ⇒ 不画。 */
const slice = (): Pick<PlanSlice, "cells"> => ({
  cells: [
    cell("A1", { children: ["A1-1", "A1-2"], coveredBy: ["R1"] }),
    cell("A1-1", { coveredBy: ["R1"] }),
    cell("A1-2", { coveredBy: ["R1", "R2"] }),
    cell("A2", { coveredBy: ["R2"] }),
    cell("A3", { coveredBy: ["R1"] }),
    cell("R1", { covers: ["A1", "A3"] }),
    cell("R2", { covers: ["A1-2", "A2"] }),
    cell("R9", { covers: ["project"] }),
  ],
});

const lanes = (rows: string[], focus: string | null = null, folded: ReadonlySet<string> = new Set()) => {
  const s = slice();
  return ringLanes(s, rows, folded, focus);
};

describe("圈槽怎么摆", () => {
  it("只写 project 的规矩不画圈", () => {
    expect(ringRules(slice()).map((g) => g.rule)).toEqual(["R1", "R2"]);
  });

  it("一圈连着几行 ⇒ 第一行带上钩、末一行带下钩，中间的不带", () => {
    const out = lanes(["A1", "A1-1", "A1-2", "A2", "A3"]);
    const r1 = out.map((row) => row.marks.find((m) => m.rule === "R1") ?? null);
    expect(r1.map((m) => (m ? [m.start, m.end] : null))).toEqual([[true, false], [false, false], [false, true], null, [true, true]]);
  });

  it("一条规矩固定一列：一格在两圈里 ⇒ 两道各在自己那一列，只在 R2 里的行 R2 也在第二列", () => {
    const out = lanes(["A1", "A1-1", "A1-2", "A2", "A3"]);
    expect(out[2].marks.map((m) => [m.rule, m.col])).toEqual([["R1", 0], ["R2", 1]]);
    expect(out[3].marks.map((m) => [m.rule, m.col])).toEqual([["R2", 1]]);
  });

  it("过滤掉的行不占位：隔开两段的 A2 不在了 ⇒ R1 连成一段", () => {
    const out = lanes(["A1", "A1-2", "A3"]);
    const r1 = out.map((row) => row.marks.find((m) => m.rule === "R1")!);
    expect(r1.map((m) => [m.start, m.end])).toEqual([[true, false], [false, false], [false, true]]);
  });

  it("折起的格：底下被圈住的 ⇒ 圈那个框", () => {
    const s = { cells: [cell("B1", { children: ["B1-1"] }), cell("B1-1", { coveredBy: ["R1"] }), cell("R1", { covers: ["B1-1"] })] };
    const out = ringLanes(s, ["B1"], new Set(["B1"]), null);
    expect(out[0].marks.map((m) => [m.rule, m.start, m.end])).toEqual([["R1", true, true]]);
  });

  it("圈色照 ringsOf 那一份", () => {
    const slots = new Map([...ringsOf(slice()).get("A1-2")!].map((g) => [g.rule, g.slot]));
    const out = lanes(["A1-2"]);
    expect(out[0].marks.map((m) => m.slot)).toEqual([slots.get("R1"), slots.get("R2")]);
  });
});

describe("选中一条规矩", () => {
  it("没选规矩 ⇒ 不亮不淡、不铺色", () => {
    const out = lanes(["A1", "A2"], "A1");
    expect(out.every((r) => r.tint === null && r.marks.every((m) => m.look === null))).toBe(true);
  });

  it("选中 R2 ⇒ R2 那几道亮、R1 的淡；R2 圈住的行铺 R2 的色，别的行不铺", () => {
    const out = lanes(["A1", "A1-2", "A2", "A3"], "R2");
    const r2slot = ringRules(slice()).find((g) => g.rule === "R2")!.slot;
    expect(out.map((r) => r.marks.map((m) => [m.rule, m.look]))).toEqual([[["R1", "dim"]], [["R1", "dim"], ["R2", "hi"]], [["R2", "hi"]], [["R1", "dim"]]]);
    expect(out.map((r) => r.tint)).toEqual([null, r2slot, r2slot, null]);
  });
});
