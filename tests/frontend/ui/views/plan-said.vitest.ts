/**
 * 计划页随数据变的那几句照后端写好的 `said` 摆：总览头 · 大纲根 · 块表「里面」· 块根那一行 ·
 * 退回框的块名 · 认不出的人。夹具里的 `said` 写成可认的「后端·…」字；照文案键现拼的那一句（mock 成「键(参数)」）不许出现。
 */
import { describe, expect, it, vi } from "vitest";

vi.mock("../../../../src/frontend/ui/copy-table", () => ({
  copyText: (k: string, a?: Record<string, unknown>) => (a ? `${k}(${Object.entries(a).map(([x, y]) => `${x}=${String(y)}`).join(",")})` : k),
}));

import { PlanView } from "../../../../src/frontend/ui/views/plan.ts";
import { CellDetail } from "../../../../src/frontend/ui/views/plan-cell.ts";
import { cellTarget, topTarget } from "../../../../src/frontend/ui/views/plan-review.ts";
import type { PlanBlock, PlanCell, PlanNeed, PlanSlice, PlanWho } from "../../../../src/frontend/ui/plan-reads.ts";

const who = (extra: Partial<PlanWho> = {}): PlanWho => ({ id: "a1b2c3d4e5f6a7b8", kind: "unknown", sid: null, needs: null, face: null as unknown as PlanWho["face"], label: "后端·认不出", ...extra });

const block = (id: string, extra: Partial<PlanBlock> = {}): PlanBlock => ({
  id,
  cells: [id],
  inside: { done: 1, of: 3 },
  dir: null,
  owner: null,
  phase: "开发",
  at: null,
  listed: true,
  handoff: null,
  handoffError: null,
  takeover: "unsure",
  groups: { groups: [], text: "—", unread: null },
  said: { name: `后端·块名${id}`, inside: "后端·里面" },
  ...extra,
});

const cell = (id: string, extra: Partial<PlanCell> = {}): PlanCell =>
  ({
    id,
    title: `标题${id}`,
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
    look: "none",
    end: null,
    quiet: null,
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
    said: { trail: null, blockRoot: null },
    ...extra,
  }) as PlanCell;

const slice = (extra: Partial<PlanSlice> = {}): PlanSlice =>
  ({
    name: "ledger",
    domain: null,
    path: null,
    current: true,
    docsOnly: false,
    error: null,
    stale: null,
    bare: false,
    kinds: [],
    phases: [],
    doneValue: "已签收",
    top: ["A1", "A2"],
    progress: { done: 1, of: 2 },
    openCells: 1,
    blocks: [block("project"), block("A2", { owner: who() })],
    unbuiltBlocks: [],
    unowned: { paths: null, why: null },
    pendingSets: [],
    check: { unreadable: [], unparsed: [], red: [], undecidable: [] },
    labels: { A1: "A1 标题A1", A2: "A2 标题A2" },
    cells: [cell("A1"), cell("A2", { said: { trail: null, blockRoot: "后端·块根" } })],
    archived: [],
    needs: [],
    needCount: 0,
    returned: null,
    said: { strip: "后端·读不成 · 过期", bare: null, readAt: "后端·读到", top: "后端·顶层", root: "后端·几/几" },
    ...extra,
  }) as PlanSlice;

/** 计划页里那几块的画法（私有的，经这一处取）。 */
type Inner = { overview(s: PlanSlice): HTMLElement; outline(s: PlanSlice): HTMLElement; whoChip(w: PlanWho): HTMLElement };
const view = (): Inner => new PlanView() as unknown as Inner;

const noKeyText = (t: string): void => {
  for (const k of ["plan.overview.top(", "plan.overview.readAt(", "plan.outline.rootProgress(", "plan.who.unknown(", "plan.whyShort.inside(", "plan.strip."]) expect(t, k).not.toContain(k);
};

describe("总览照后端写好的那几句摆", () => {
  it("头上顶层签了几格 · 读到的时刻 · 块表「里面」那一格", () => {
    const el = view().overview(slice());
    const t = el.textContent ?? "";
    expect(t).toContain("后端·顶层");
    expect(t).toContain("后端·读到");
    expect(t).toContain("后端·里面");
    noKeyText(t);
  });

  it("大纲根那一行右端：后端写好的「几/几」", () => {
    const head = view().outline(slice()).querySelector(".plan-row");
    expect(head?.textContent ?? "").toContain("后端·几/几");
    noKeyText(head?.textContent ?? "");
  });

  it("认不出的人：写后端给的那一名，不拿 id 现拼", () => {
    const t = view().whoChip(who()).textContent ?? "";
    expect(t).toBe("后端·认不出");
  });
});

describe("一格详情的块根那一行", () => {
  it("块的根节点 ⇒ 后端写好的那一句", () => {
    const d = new CellDetail({ select: () => {}, who: () => document.createElement("span"), origin: () => "", workspace: () => "/w", switchTo: () => {}, returnCell: () => {}, takeover: () => {}, ask: () => {} });
    const s = slice();
    const t = d.render(s, s.cells[1]!).textContent ?? "";
    expect(t).toContain("后端·块根");
    expect(t).not.toContain("plan.cell.blockRoot(");
  });
});

describe("退回框的块名照后端那一份", () => {
  it("块里的格 ⇒ 那一块的名；顶块 ⇒ 顶块那一块的名", () => {
    const s = slice();
    expect(cellTarget(s, s.cells[1]!).blockTitle).toBe("后端·块名A2");
    expect(cellTarget(s, s.cells[0]!).blockTitle).toBe("后端·块名project");
    const need = { key: "k", kind: "top", block: "project", cell: null, sid: null, acked: false, title: "", lines: [], pos: null } as PlanNeed;
    expect(topTarget(s, need)?.blockTitle).toBe("后端·块名project");
  });
});
