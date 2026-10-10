/**
 * 「需手动」的次序：会话在前、计划项在后（`Ctrl J` 与点「需手动」那一条）。期望手写。
 */
import { describe, expect, it } from "vitest";

import { nextNeedsStep } from "../../../src/frontend/ui/session-face.ts";

describe("需手动：会话在前、计划项在后", () => {
  it("站在会话上 ⇒ 下一个会话；最后一个会话之后 ⇒ 第一条计划项；没有计划项 ⇒ 绕回第一个会话", () => {
    expect(nextNeedsStep(["a", "b"], "a", null, 2)).toEqual({ sid: "b" });
    expect(nextNeedsStep(["a", "b"], "b", null, 2)).toEqual({ plan: 0 });
    expect(nextNeedsStep(["a", "b"], "b", null, 0)).toEqual({ sid: "a" });
    expect(nextNeedsStep(["a", "b"], "x", null, 2), "站的会话不在里面 ⇒ 从头").toEqual({ sid: "a" });
  });
  it("站在计划项上 ⇒ 下一条计划项；最后一条之后 ⇒ 第一个会话；没有会话 ⇒ 绕回第一条", () => {
    expect(nextNeedsStep(["a"], "a", 0, 2)).toEqual({ plan: 1 });
    expect(nextNeedsStep(["a"], "a", 1, 2)).toEqual({ sid: "a" });
    expect(nextNeedsStep([], null, 1, 2)).toEqual({ plan: 0 });
  });
  it("只有计划项 ⇒ 第一条；什么都没有 ⇒ null", () => {
    expect(nextNeedsStep([], null, null, 3)).toEqual({ plan: 0 });
    expect(nextNeedsStep([], null, null, 0)).toBeNull();
  });
});
