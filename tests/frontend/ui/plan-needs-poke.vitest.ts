/**
 * 计划那一侧的账（`plan-needs.ts`）收到那台的需手动铃：连着来的几声合成一次 `plan-list`。
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

const { fetchPlanList } = vi.hoisted(() => ({ fetchPlanList: vi.fn(async () => ({ workspaces: [], sessions: [] })) }));
vi.mock("../../../src/frontend/ui/plan-reads", () => ({ fetchPlanList }));

import { PlanNeedsBook } from "../../../src/frontend/ui/plan-needs";
import { LOCAL } from "../../../src/frontend/ui/ipc/origin";

describe("PlanNeedsBook.poke 合批", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    fetchPlanList.mockClear();
  });
  afterEach(() => vi.useRealTimers());

  it("连着响几声 ⇒ 那台只问一次 plan-list", async () => {
    const book = new PlanNeedsBook();
    for (let i = 0; i < 5; i++) book.poke(LOCAL);
    expect(fetchPlanList).toHaveBeenCalledTimes(0);
    await vi.advanceTimersByTimeAsync(1_000);
    expect(fetchPlanList).toHaveBeenCalledTimes(1);
  });
});
