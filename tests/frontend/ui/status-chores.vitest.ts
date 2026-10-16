/**
 * 状态栏「待办 N」：N ＝ 各台 `data-report` 的件数交本机后端合成那一份的 `badge`（与设置窗左栏角标同一个数；问不到的那台不交），
 * 0 不渲染；只重问一台时只问那一台；点了打开设置窗直达「文件与数据」；先发的合计晚回来不盖后发的。
 */
import { describe, it, expect, vi } from "vitest";
import { StatusChores } from "../../../src/frontend/ui/status-chores";
import { copyText } from "../../../src/frontend/ui/copy-table";
import type { ChoreCounts } from "../../../src/frontend/ui/generated/ChoreCounts";

const flush = async (): Promise<void> => {
  for (let i = 0; i < 5; i++) await Promise.resolve();
};
const of = (badge: number): ChoreCounts => ({ badge, open: badge, must: badge, decide: 0, install: 0, optional: 0, done: 0 });
/** 本机后端那一问的替身：照交来的几份答一个和（数在后端，这里只看界面交了哪几份、画的是不是它答的那一个）。 */
const merged = (n: number): ChoreCounts => ({ ...of(0), badge: n });

describe("状态栏「待办 N」", () => {
  it("★ 读成的那几台交本机后端合（问不到的那台不交、不当 0）；画它答的 badge；0 不渲染；只重问一台时只问它；点了直达「文件与数据」", async () => {
    const counts = new Map<string, ChoreCounts | null>([["<local>", of(1)], ["devbox", of(2)], ["gpu-01", null]]);
    const chores = vi.fn(async (o: string) => counts.get(o) ?? null);
    const merge = vi.fn(async (ms: ChoreCounts[]) => merged(ms.reduce((a, m) => a + m.badge, 0)));
    const open = vi.fn();
    const c = new StatusChores({ machines: async () => ["<local>", "devbox", "gpu-01"], chores, merge, open });
    expect(c.el.childElementCount, "问到之前不出").toBe(0);
    await c.refreshAll();
    await flush();
    expect(merge.mock.calls.at(-1)![0], "问不到的那台不交").toEqual([of(1), of(2)]);
    expect([c.el.childElementCount, c.el.textContent]).toEqual([1, copyText("statusBar.todo.label", { n: "3" })]);
    counts.set("devbox", of(0));
    counts.set("<local>", of(0));
    chores.mockClear();
    await c.refresh("devbox");
    expect(chores.mock.calls.map((x) => x[0]), "只重问那一台").toEqual(["devbox"]);
    expect(c.el.textContent).toBe(copyText("statusBar.todo.label", { n: "1" }));
    await c.refresh("<local>");
    expect(c.el.childElementCount, "0 不渲染").toBe(0);
    counts.set("gpu-01", of(4));
    await c.refresh("gpu-01");
    expect(c.el.textContent).toBe(copyText("statusBar.todo.label", { n: "4" }));
    c.el.querySelector("button")!.click();
    expect(open).toHaveBeenCalledTimes(1);
  });

  it("★ 画的是后端答的数，不是界面自己加的；合不了 ⇒ 不出", async () => {
    const merge = vi.fn(async () => merged(7));
    const c = new StatusChores({ machines: async () => ["<local>"], chores: async () => of(1), merge, open: vi.fn() });
    await c.refreshAll();
    expect(c.el.textContent).toBe(copyText("statusBar.todo.label", { n: "7" }));
    merge.mockRejectedValueOnce(new Error("本机后端不在"));
    await c.refresh("<local>");
    expect(c.el.childElementCount).toBe(0);
  });

  it("★ 先发的合计晚回来不盖后发的", async () => {
    let release!: () => void;
    const slow = new Promise<void>((r) => (release = r));
    const merge = vi
      .fn<(ms: ChoreCounts[]) => Promise<ChoreCounts>>()
      .mockImplementationOnce(async () => (await slow, merged(9)))
      .mockImplementation(async () => merged(2));
    const c = new StatusChores({ machines: async () => ["<local>"], chores: async () => of(1), merge, open: vi.fn() });
    const first = c.refresh("<local>");
    await flush();
    await c.refresh("<local>");
    release();
    await first;
    expect(c.el.textContent).toBe(copyText("statusBar.todo.label", { n: "2" }));
  });
});
