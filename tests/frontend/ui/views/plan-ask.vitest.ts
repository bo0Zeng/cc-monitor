/**
 * 问图：一格详情头的［问这一块…］点了交给宿主；开的框照那台的 `plan-ask` 回包预填（机器锁那台 · 目录锁工作区根 ·
 * 别名预选那台记着的那条 · 开口那句是那台拼的），改选别名再问那台一次。期望手写，夹具是合成的中性例子。
 */
import { describe, expect, it, vi } from "vitest";

vi.mock("../../../../src/frontend/ui/copy-table", () => ({
  copyText: (k: string, a?: Record<string, unknown>) => (a ? `${k}(${Object.entries(a).map(([x, y]) => `${x}=${String(y)}`).join(",")})` : k),
}));

const asked = vi.hoisted(() => ({ calls: [] as unknown[][] }));
vi.mock("../../../../src/frontend/ui/plan-reads", async (orig) => ({
  ...(await orig<Record<string, unknown>>()),
  askPlan: (origin: string, a: Record<string, unknown>) => {
    asked.calls.push([origin, a]);
    return Promise.resolve({ alias: a.alias ?? null, line: `line-of-${String(a.alias)}` });
  },
}));

import { CellDetail } from "../../../../src/frontend/ui/views/plan-cell.ts";
import { askSpec } from "../../../../src/frontend/ui/plan-ask.ts";
import type { PlanCell, PlanSlice } from "../../../../src/frontend/ui/plan-reads.ts";

const cell = (): PlanCell =>
  ({
    id: "A2-1-3",
    title: "编码探测",
    kind: null,
    body: null,
    parent: null,
    children: [],
    edges: { to: [], with: [], after: [], replaces: [] },
    pointedBy: { to: [], with: [], after: [], replaces: [] },
    files: [],
    state: [],
    stateLine: null,
    evidencedBy: [],
    statusCode: null,
    undecided: false,
    whyCode: null,
    whyShort: null,
    sets: [],
    block: null,
    owner: null,
    signer: null,
    covers: [],
    coveredBy: [],
    refs: { title: [], body: [] },
    returned: null,
  }) as unknown as PlanCell;
const slice = (): PlanSlice => ({ name: "ledger", labels: { "A2-1-3": "A2-1-3 编码探测" }, cells: [cell()], blocks: [], pendingSets: [], kinds: [] }) as unknown as PlanSlice;

describe("一格详情头的［问这一块…］", () => {
  it("有那颗按钮，点了把这一片 · 这一格交给宿主", () => {
    const got: [string, string][] = [];
    const d = new CellDetail({
      select: () => {},
      who: () => document.createElement("span"),
      origin: () => "",
      workspace: () => "/work/ledger",
      switchTo: () => {},
      returnCell: () => {},
      takeover: () => {},
      ask: (sl, c) => got.push([sl.name, c.id]),
    });
    const el = d.render(slice(), cell());
    const b = [...el.querySelectorAll("button")].find((x) => x.textContent?.includes("plan.ask.open"));
    expect(b, "没有［问这一块…］").toBeTruthy();
    b!.click();
    expect(got).toEqual([["ledger", "A2-1-3"]]);
  });
});

describe("问图框交出的那一份", () => {
  it("那台 · 机器锁住 · 目录锁在工作区根 · 开口那句是那台拼的 · 别名预选那台回的那条 · 框顶那一句照那台写的", () => {
    const spec = askSpec("gpu-box", "/work/ledger", slice(), cell(), { title: "问图 · 编码探测", alias: "look", line: "读图。\n看 A2-1-3「编码探测」：" });
    expect(spec).toMatchObject({ origin: "gpu-box", lockMachine: true, cwd: "/work/ledger", lockCwd: true, prompt: "读图。\n看 A2-1-3「编码探测」：" });
    expect(spec.ask?.alias).toBe("look");
    expect(spec.ask?.title).toBe("问图 · 编码探测");
  });

  it("没问过图（那台回 null）⇒ 别名不预选", () => {
    const spec = askSpec("gpu-box", "/work/ledger", slice(), cell(), { title: "t", alias: null, line: "看 A2-1-3「编码探测」：" });
    expect(spec.ask?.alias).toBeNull();
  });

  it("改选别名 ⇒ 带上那条再问那台一次，拿回新拼的那句", async () => {
    asked.calls.length = 0;
    const spec = askSpec("gpu-box", "/work/ledger", slice(), cell(), { title: "t", alias: null, line: "x" });
    await expect(spec.ask!.relined("look")).resolves.toBe("line-of-look");
    expect(asked.calls).toEqual([["gpu-box", { workspace: "/work/ledger", slice: "ledger", id: "A2-1-3", alias: "look" }]]);
  });
});
