/**
 * 「需手动」里计划那一项直达那一格：账照后端那一项带的格交出（`cell {id, label}`，没有格 ⇒ `null`）；
 * 浮层计划一条一行，第一行是后端写好的「编号 标题」（没有格 ⇒ 片名），点它交出那一条的下标；计划页照 `cell.id` 选中（那一片上没有那一格 ⇒ 概览）。
 * 期望手写，夹具是合成的中性例子。
 */
import { describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("../../../src/frontend/ui/copy-table", () => ({
  copyText: (k: string) => k,
}));

import { NeedsBook, planCellIn } from "../../../src/frontend/ui/needs-book";
import { planRows, renderNeedsPanel, type PanelFacts } from "../../../src/frontend/ui/needs-panel";
import { LOCAL_ORIGIN } from "../../../src/frontend/ui/ipc/origin";
import type { NeedsMerged } from "../../../src/frontend/ui/generated/NeedsMerged";

const merged: NeedsMerged = {
  items: [
    { origin: LOCAL_ORIGIN, item: { kind: "plan", rank: 1, workspace: "/work/ledger", slice: "ledger", cell: { id: "A2-1", label: "A2-1 编码探测" } } },
    { origin: LOCAL_ORIGIN, item: { kind: "plan", rank: 1, workspace: "/work/ledger", slice: "ledger" } },
    { origin: LOCAL_ORIGIN, item: { kind: "plan", rank: 1, workspace: "/work/ledger", slice: "ledger", cell: { id: "A3", label: "A3 导出" } } },
  ],
  total: 3,
  unreadable: [],
  down: [],
};

async function book(): Promise<NeedsBook> {
  const b = new NeedsBook(async () => merged);
  await b.refresh(LOCAL_ORIGIN);
  return b;
}

function facts(b: NeedsBook): PanelFacts {
  return { sessions: [], plans: planRows(b.plans()), manual: [], offline: [], unreadable: [] };
}

describe("需手动的计划项直达那一格", () => {
  it("账照后端那一项交出那一格；没有格 ⇒ null", async () => {
    const b = await book();
    expect(b.plans().map((p) => p.cell)).toEqual([{ id: "A2-1", label: "A2-1 编码探测" }, null, { id: "A3", label: "A3 导出" }]);
  });

  it("★ 浮层计划一条一行：第一行是「编号 标题」（没有格 ⇒ 片名），同一片的几条不收成一行", async () => {
    const b = await book();
    const { root } = renderNeedsPanel(facts(b), { answer: () => {}, plan: () => {}, login: () => {} }, 0);
    const rows = [...root.querySelectorAll<HTMLElement>("[data-row=plan]")];
    expect(rows.length).toBe(3);
    expect(rows.map((r) => r.firstElementChild!.nextElementSibling!.firstElementChild!.textContent)).toEqual(["A2-1 编码探测", "ledger", "A3 导出"]);
  });

  it("★ 点哪一行交出哪一条的下标（计划页照它那一条的格走）", async () => {
    const b = await book();
    const got: number[] = [];
    const { root } = renderNeedsPanel(facts(b), { answer: () => {}, plan: (i) => got.push(i), login: () => {} }, 0);
    const rows = [...root.querySelectorAll<HTMLElement>("[data-row=plan]")];
    rows[2].click();
    rows[1].click();
    expect(got).toEqual([2, 1]);
  });

  it("★ 计划页照 cell.id 选中：那一片上有那一格 ⇒ 它；没有格 · 那一片上没有那一格 ⇒ 概览（null）", async () => {
    const [a, none, gone] = (await book()).plans();
    const slice = { cells: [{ id: "A2-1" }, { id: "A2" }] };
    expect(planCellIn(slice, a)).toBe("A2-1");
    expect(planCellIn(slice, none)).toBeNull();
    expect(planCellIn(slice, gone)).toBeNull();
  });
});
