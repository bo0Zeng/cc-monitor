/**
 * 状态栏「待办 N」：N ＝ 本机后端 `chores-merge` 问到各台、件数合成那一份的 `badge`（与设置窗左栏角标同一个数；没问到的那台后端不计），
 * 0 不渲染；一趟只问本机后端一次（交机器表那几台）；点了打开设置窗直达「文件与数据」；先发的晚回来不盖后发的。
 */
import { describe, it, expect, vi } from "vitest";
import { StatusChores } from "../../../src/frontend/ui/status-chores";
import { copyText } from "../../../src/frontend/ui/copy-table";
import type { ChoreCounts } from "../../../src/frontend/ui/generated/ChoreCounts";

const of = (badge: number): ChoreCounts => ({ badge, open: badge, must: badge, decide: 0, install: 0, optional: 0, done: 0 });

describe("状态栏「待办 N」", () => {
  it("★ 机器表那几台一次交本机后端；画它答的 badge（界面不自己加）；0 不渲染；点了直达「文件与数据」", async () => {
    let n = 3;
    const read = vi.fn(async (_ms: string[]) => ({ total: of(n) }));
    const open = vi.fn();
    const c = new StatusChores({ machines: async () => ["<local>", "devbox", "gpu-01"], read, open });
    expect(c.el.childElementCount, "问到之前不出").toBe(0);
    await c.refreshAll();
    expect(read.mock.calls.map((x) => x[0]), "一趟问一次、交整张机器表").toEqual([["<local>", "devbox", "gpu-01"]]);
    expect([c.el.childElementCount, c.el.textContent]).toEqual([1, copyText("statusBar.todo.label", { n: "3" })]);
    n = 0;
    await c.refreshAll();
    expect(c.el.childElementCount, "0 不渲染").toBe(0);
    n = 4;
    await c.refreshAll();
    c.el.querySelector("button")!.click();
    expect(open).toHaveBeenCalledTimes(1);
  });

  it("★ 本机后端问不到 ⇒ 不出（不猜）", async () => {
    const read = vi.fn(async () => ({ total: of(7) }));
    const c = new StatusChores({ machines: async () => ["<local>"], read, open: vi.fn() });
    await c.refreshAll();
    expect(c.el.textContent).toBe(copyText("statusBar.todo.label", { n: "7" }));
    read.mockRejectedValueOnce(new Error("本机后端不在"));
    await c.refreshAll();
    expect(c.el.childElementCount).toBe(0);
  });

  it("★ 先发的晚回来不盖后发的", async () => {
    let release!: () => void;
    const slow = new Promise<void>((r) => (release = r));
    const read = vi
      .fn<(ms: string[]) => Promise<{ total: ChoreCounts }>>()
      .mockImplementationOnce(async () => (await slow, { total: of(9) }))
      .mockImplementation(async () => ({ total: of(2) }));
    const c = new StatusChores({ machines: async () => ["<local>"], read, open: vi.fn() });
    const first = c.refreshAll();
    await Promise.resolve();
    await c.refreshAll();
    release();
    await first;
    expect(c.el.textContent).toBe(copyText("statusBar.todo.label", { n: "2" }));
  });
});
