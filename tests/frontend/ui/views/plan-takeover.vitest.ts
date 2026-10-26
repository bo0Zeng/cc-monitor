/**
 * 权力范围那一行的「另开会话接手」：出不出、灰不灰照后端那一格（`takeover`）画，界面不看负责人自己判。
 * 节点详情里交接说明那一块出不出也照它。期望手写，夹具是合成的中性例子。
 */
import { describe, expect, it, vi } from "vitest";

vi.mock("../../../../src/frontend/ui/copy-table", () => ({
  copyText: (k: string, a?: Record<string, unknown>) => (a ? `${k}(${Object.entries(a).map(([x, y]) => `${x}=${String(y)}`).join(",")})` : k),
}));

import { CellDetail, takeoverCell, takeoverSpec, unbuiltTakeoverCell } from "../../../../src/frontend/ui/views/plan-cell.ts";
import type { PlanBlock, PlanCell, PlanSlice, PlanWho } from "../../../../src/frontend/ui/plan-reads.ts";

const HANDOFF = "你负责 ledger 的 A2 导入 csv：先 pb claim A2 <认领码>，再 pb where。认领码找上一级要";

const who = (sid: string | null): PlanWho => ({ id: "a1b2c3d4e5f6", kind: sid ? "session" : "unknown", sid, needs: null, face: null as unknown as PlanWho["face"], label: null });

const block = (extra: Partial<PlanBlock> = {}): PlanBlock => ({
  id: "A2",
  cells: ["A2"],
  inside: null,
  dir: null,
  owner: null,
  phase: "定架构",
  at: null,
  listed: true,
  handoff: HANDOFF,
  handoffError: null,
  takeover: "open",
  ...extra,
});

const buttons = (el: HTMLElement): HTMLButtonElement[] => [...el.querySelectorAll("button")];
const takeBtn = (el: HTMLElement): HTMLButtonElement | undefined => buttons(el).find((b) => b.textContent?.includes("plan.takeover.open"));

describe("权力范围那一行：另开会话接手", () => {
  it("立了没人接手（open）⇒ 有那颗按钮，点了就去开框", () => {
    let opened = 0;
    const el = takeoverCell(block(), () => opened++);
    const b = takeBtn(el);
    expect(b, "没有［另开会话接手…］").toBeTruthy();
    expect(b!.disabled).toBe(false);
    b!.click();
    expect(opened).toBe(1);
  });

  it("开框交出的：那台 · 机器锁住 · 目录锁在工作区根 · 开口那句是交接说明", () => {
    expect(takeoverSpec("gpu-box", "/work/ledger", block())).toEqual({ origin: "gpu-box", lockMachine: true, cwd: "/work/ledger", lockCwd: true, prompt: HANDOFF });
  });

  it("交接说明拿不到（handoff 为 null）⇒ 不预填开口那句", () => {
    expect(takeoverSpec("gpu-box", "/work/ledger", block({ handoff: null }))).toEqual({ origin: "gpu-box", lockMachine: true, cwd: "/work/ledger", lockCwd: true });
  });

  it("该出、交接说明拿不到（no_line）⇒ 不给按钮，写 pb 说的为什么", () => {
    const el = takeoverCell(block({ handoff: null, handoffError: "A2 不在当前仓库", takeover: "no_line" }), () => {});
    expect(takeBtn(el)).toBeUndefined();
    expect(el.textContent).toContain("plan.handoff.none(reason=A2 不在当前仓库)");
  });

  it("负责人在跑 · 说不清 · 要并回 ⇒ 什么都不出（哪怕负责人字段是空的，界面也不自己判）", () => {
    for (const t of ["running", "unsure", "merging"] as const) {
      const el = takeoverCell(block({ takeover: t }), () => {});
      expect(buttons(el), t).toHaveLength(0);
      expect(el.textContent, t).toBe("");
    }
  });

  it("还没建的（下一趟 update 建）⇒ 灰着的按钮，悬停说为什么", () => {
    const el = unbuiltTakeoverCell();
    const b = takeBtn(el);
    expect(b, "还没建的那一行要有灰按钮").toBeTruthy();
    expect(b!.disabled || b!.getAttribute("aria-disabled") === "true").toBe(true);
    expect(b!.title || b!.dataset.kitTitle).toBe("plan.takeover.unbuilt");
  });
});

describe("节点详情的交接说明照 takeover 出", () => {
  const cell = (): PlanCell =>
    ({
      id: "A2",
      title: "导入 csv",
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
      block: "A2",
      owner: null,
      signer: null,
      covers: [],
      coveredBy: [],
      refs: { title: [], body: [] },
      returned: null,
    }) as unknown as PlanCell;
  const slice = (b: PlanBlock): PlanSlice => ({ name: "ledger", labels: { A2: "A2 导入 csv" }, cells: [cell()], blocks: [b], pendingSets: [], kinds: [] }) as unknown as PlanSlice;
  const detail = () =>
    new CellDetail({ select: () => {}, who: () => document.createElement("span"), origin: () => "", workspace: () => "/work/ledger", switchTo: () => {}, returnCell: () => {}, takeover: () => {} });

  it("负责人那条会话停了（open，owner 还在）⇒ 出交接说明", () => {
    const el = detail().render(slice(block({ owner: who("s-stopped"), takeover: "open" })), cell());
    expect(el.querySelector(".plan-handoff")?.textContent).toContain(HANDOFF);
  });

  it("负责人在跑（running）⇒ 不出", () => {
    const el = detail().render(slice(block({ owner: who("s-live"), takeover: "running" })), cell());
    expect(el.querySelector(".plan-handoff")).toBeNull();
  });

  it("没负责人但要并回（merging）⇒ 不出", () => {
    const el = detail().render(slice(block({ takeover: "merging", listed: false })), cell());
    expect(el.querySelector(".plan-handoff")).toBeNull();
  });
});
