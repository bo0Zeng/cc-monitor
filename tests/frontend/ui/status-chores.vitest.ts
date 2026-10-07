/**
 * 状态栏「要你动手 N」：N ＝ 各台 `data-report` 的 `chores` 相加（与设置窗左栏角标同一个数，读 `choresOf`，问不到的那台不算），
 * 0 不渲染；只重问一台时只问那一台；点了打开设置窗直达「文件与数据」。
 */
import { describe, it, expect, vi } from "vitest";
import { StatusChores } from "../../../src/frontend/ui/status-chores";

const flush = async (): Promise<void> => {
  for (let i = 0; i < 5; i++) await Promise.resolve();
};

describe("状态栏「要你动手 N」", () => {
  it("★ 各台相加（问不到的那台不算、不当 0）；0 不渲染；只重问一台时只问它；点了直达「文件与数据」", async () => {
    const counts = new Map<string, number | null>([["<local>", 1], ["devbox", 2], ["gpu-01", null]]);
    const chores = vi.fn(async (o: string) => counts.get(o) ?? null);
    const open = vi.fn();
    const c = new StatusChores({ machines: async () => ["<local>", "devbox", "gpu-01"], chores, open });
    expect(c.el.childElementCount, "问到之前不出").toBe(0);
    await c.refreshAll();
    await flush();
    expect([c.el.childElementCount, c.el.textContent]).toEqual([1, "要你动手 3"]);
    counts.set("devbox", 0);
    counts.set("<local>", 0);
    chores.mockClear();
    await c.refresh("devbox");
    expect(chores.mock.calls.map((x) => x[0]), "只重问那一台").toEqual(["devbox"]);
    expect(c.el.textContent).toBe("要你动手 1");
    await c.refresh("<local>");
    expect(c.el.childElementCount, "0 不渲染").toBe(0);
    counts.set("gpu-01", 4);
    await c.refresh("gpu-01");
    expect(c.el.textContent).toBe("要你动手 4");
    c.el.querySelector("button")!.click();
    expect(open).toHaveBeenCalledTimes(1);
  });
});
