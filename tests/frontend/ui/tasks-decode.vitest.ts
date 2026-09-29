/**
 * 〔LOC1a · 第四波 4D · C4e 批 4〕`tasks-list` 成品的界面解码器（`src/frontend/ui/tasks-panel.ts::decodeTasks`）。
 *
 * 要求住址：`设计/05 §14.3` 逐字「界面按形状严格收（多一格 / 缺一格 / 类型不对 ⇒ 抛『两端契约对不上』，不猜）；
 * 线上形状由一份跨语言金样钉住（后端测试产出 == 金样 · TS 解码器读同一份）」。
 * 异源：金样的 `product` 由后端 `feature_face_tests::the_tasks_product_matches_the_cross_language_golden` 从生产路径现算核过；
 * 本文件读同一份。夹具只造结构（占位字段），不采任何真会话正文。
 */
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { decodeTasks } from "../../../src/frontend/ui/tasks-panel";

const golden = JSON.parse(
  readFileSync(join(__dirname, "../../__fixtures__", "tasks-list.golden.json"), "utf8"),
) as { product: { tasks: Record<string, unknown>[] } };

describe("decodeTasks：金样那一份收得下", () => {
  it("逐格原样交回（可选两格缺席就是缺席）", () => {
    const got = decodeTasks(golden.product);
    expect(got.map((t) => t.id)).toEqual(["1", "2", "3"]);
    expect(got).toEqual(golden.product.tasks);
    expect("description" in got[2]).toBe(false);
  });
  it("空清单 ⇒ 空（诚实的空）", () => {
    expect(decodeTasks({ tasks: [] })).toEqual([]);
  });
});

describe("decodeTasks：形状不对一律抛，不跳过那一条", () => {
  const ok = { id: "1", subject: "s", status: "pending", blocks: [], blockedBy: [] };
  const cases: [string, unknown][] = [
    ["多一格", { tasks: [{ ...ok, extra: 1 }] }],
    ["缺必填", { tasks: [{ id: "1", subject: "s", blocks: [], blockedBy: [] }] }],
    ["id 不是串", { tasks: [{ ...ok, id: 1 }] }],
    ["blocks 是 null", { tasks: [{ ...ok, blocks: null }] }],
    ["blockedBy 里有非串", { tasks: [{ ...ok, blockedBy: [1] }] }],
    ["可选格是 null", { tasks: [{ ...ok, description: null }] }],
    ["还是旧的按行形状", { lines: ["{}"] }],
    ["顶层多一格", { tasks: [], more: 1 }],
    ["顶层是数组", []],
  ];
  for (const [what, v] of cases) {
    it(what, () => {
      expect(() => decodeTasks(v)).toThrow(/shape mismatch/);
    });
  }
});
