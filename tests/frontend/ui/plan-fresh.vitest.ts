/**
 * 新登记的仓库还没有图（pb repo 建了 project，等某个会话认领它画图）：那一屏怎么说、按钮去哪、交接说明怎么给。
 * 期望手写，夹具是合成的中性例子。
 */
import { describe, expect, it, vi } from "vitest";

vi.mock("../../../src/frontend/ui/copy-table", () => ({
  copyText: (k: string, a?: Record<string, unknown>) => (a ? `${k}(${Object.entries(a).map(([x, y]) => `${x}=${String(y)}`).join(",")})` : k),
}));

import { isFresh } from "../../../src/frontend/ui/views/plan-model.ts";
import { freshPane } from "../../../src/frontend/ui/views/plan-bits.ts";
import { handoffBox } from "../../../src/frontend/ui/views/plan-cell.ts";
import type { PlanSlice } from "../../../src/frontend/ui/plan-reads.ts";

const slice = (extra: Partial<PlanSlice> = {}): PlanSlice => ({ name: "invoice", domain: "软件", bare: false, error: null, stale: null, cells: [], top: [], blocks: [], ...extra }) as unknown as PlanSlice;

const HANDOFF = "你负责 invoice 的 project：先 pb claim project <认领码>，再 pb where。认领码只在人登记这个仓库与重发时印出来；丢了请人敲 `pb reissue project`";

describe("无图：哪一个仓库算", () => {
  it("读好了、一个节点都没有 ⇒ 算", () => {
    expect(isFresh(slice())).toBe(true);
  });
  it("有节点 · 读不成又没读好过 · 这一刻读不成 ⇒ 都不算（各有各的那一屏）", () => {
    expect(isFresh(slice({ cells: [{ id: "A1" }] as unknown as PlanSlice["cells"] }))).toBe(false);
    expect(isFresh(slice({ bare: true }))).toBe(false);
    expect(isFresh(slice({ error: "图.md 读不成" }))).toBe(false);
  });
});

describe("无图那一屏", () => {
  it("一句「名字 · 领域 · 无图」＋ 一行怎么让它有 ＋［起新会话…］，点了按工作区根开框", () => {
    const opened: string[] = [];
    const el = freshPane(slice(), "/work/ledger", (cwd) => opened.push(cwd));
    expect(el.textContent).toContain("plan.fresh.title(name=invoice,domain=软件)");
    expect(el.textContent).toContain("plan.fresh.hint");
    const btn = [...el.querySelectorAll("button")].find((b) => b.textContent?.includes("plan.fresh.newSession"));
    expect(btn, "没有［起新会话…］").toBeTruthy();
    btn!.click();
    expect(opened).toEqual(["/work/ledger"]);
    expect(el.textContent).not.toContain("/work/ledger");
  });
  it("pb 没给领域 ⇒ 标题不带那一段", () => {
    const el = freshPane(slice({ domain: null }), "/w", () => {});
    expect(el.textContent).toContain("plan.fresh.titleBare(name=invoice)");
  });
  it("带上 project 的交接说明 ⇒ 那一句原样（不带认领码，句末 pb 写了码找谁要）＋［复制］，界面不另拼一句", () => {
    const el = freshPane(slice(), "/w", () => {}, handoffBox({ handoff: HANDOFF, handoffError: null }));
    expect(el.querySelector(".plan-handoff")?.textContent).toContain(HANDOFF);
    expect([...el.querySelectorAll("button")].some((b) => b.getAttribute("aria-label") === "plan.handoff.copy" || b.textContent?.includes("plan.handoff.copy"))).toBe(true);
    expect(el.textContent).not.toContain("reissue project <");
  });
});

describe("交接说明给不了", () => {
  it("handoff 为 null ⇒ 照出 pb 说的为什么，没有［复制］", () => {
    const el = handoffBox({ handoff: null, handoffError: "project 不在当前仓库：先请人敲 `pb repo invoice`" });
    expect(el.textContent).toContain("plan.handoff.none(reason=project 不在当前仓库：先请人敲 `pb repo invoice`)");
    expect(el.querySelector("button")).toBeNull();
  });
});
