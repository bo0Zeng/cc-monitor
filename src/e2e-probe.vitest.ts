/** Batch13-F40c:抖动探针的反转计数器单测(纯函数) */
import { describe, expect, it } from "vitest";
import { countReversals } from "./e2e-probe";

describe("countReversals", () => {
  it("单调序列(贴底跟随内容增长上移)= 0 反转", () => {
    expect(countReversals([100, 90, 80, 70])).toBe(0);
    expect(countReversals([70, 80, 90])).toBe(0);
  });

  it("±0.5px 高频抖动(§21 病根形态)被计出", () => {
    expect(countReversals([100, 100.5, 100, 100.5, 100])).toBe(3);
  });

  it("阈值滤亚像素噪声:±0.2px 不算位移", () => {
    expect(countReversals([100, 100.2, 100.05, 100.15])).toBe(0);
  });

  it("空/单元素/恒定序列 = 0", () => {
    expect(countReversals([])).toBe(0);
    expect(countReversals([5])).toBe(0);
    expect(countReversals([5, 5, 5])).toBe(0);
  });

  it("单向大位移后回摆一次 = 1", () => {
    expect(countReversals([100, 50, 20, 24])).toBe(1);
  });
});
