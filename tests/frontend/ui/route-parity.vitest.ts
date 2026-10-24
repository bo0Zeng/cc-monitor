/**
 * `routeMeta` 路由表单测（「收集路由 parity」）：收纳（不建卡）与渲染两条路共用这一份。
 * 标题记录（`t: "title"`）⇒ consumed（不建卡；叫会话什么是核心的账）；其余各类 ⇒ content（主线外清单是后端给的，这里不配、不喂）。
 */
import { describe, it, expect } from "vitest";
import { routeMeta } from "../../../src/frontend/ui/render-stream-record";
import type { JsonlLinePayload } from "../../../src/frontend/ui/generated/JsonlLinePayload";

function mk(record: Record<string, unknown>): JsonlLinePayload {
  return { session_id: "s", cwd: null, path: "/p", seq: 1, record: { agent: "claude", id: "r1", ...record } } as unknown as JsonlLinePayload;
}

describe("routeMeta 路由表", () => {
  it("标题（代理起的 · 人改的）→ consumed", () => {
    expect(routeMeta(mk({ t: "title", text: "甲", by: "agent" }))).toBe("consumed");
    expect(routeMeta(mk({ t: "title", text: "乙", by: "user" }))).toBe("consumed");
  });

  it("said / reply / retry → content", () => {
    for (const t of ["said", "reply", "retry"]) expect(routeMeta(mk({ t })), t).toBe("content");
  });
});

