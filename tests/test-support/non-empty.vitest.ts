/** 反空真两处：`nonEmpty` 对空的那一份当场抛；生产源码遍历在人群为空时当场抛。 */
import { describe, it, expect } from "vitest";
import { nonEmpty } from "./non-empty";
import { productionTsFiles } from "./production-sources";

describe("反空真", () => {
  it("空数组 · 空集合 · 空表 · 空串 ⇒ 抛；有东西 ⇒ 原样交回", () => {
    for (const xs of [[], new Set(), new Map(), ""]) expect(() => nonEmpty(xs)).toThrow("空的");
    const xs = [1];
    expect(nonEmpty(xs)).toBe(xs);
    expect(nonEmpty(new Set([1])).size).toBe(1);
  });

  it("生产源码遍历扫到零份 ⇒ 抛，不交一个空人群", () => {
    expect(() => productionTsFiles("src/shared/copy")).toThrow("一份 .ts 生产文件都没扫到");
  });
});
