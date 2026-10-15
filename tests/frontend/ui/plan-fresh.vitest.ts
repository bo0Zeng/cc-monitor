/**
 * 新开的一片还没有图（pb 只记了名字与领域，顶块要等某个会话 claim 才建）：那一屏怎么说、按钮去哪。期望手写，夹具是合成的中性例子。
 */
import { describe, expect, it, vi } from "vitest";

vi.mock("../../../src/frontend/ui/copy-table", () => ({
  copyText: (k: string, a?: Record<string, unknown>) => (a ? `${k}(${Object.entries(a).map(([x, y]) => `${x}=${String(y)}`).join(",")})` : k),
}));

import { isFresh } from "../../../src/frontend/ui/views/plan-model.ts";
import { freshPane } from "../../../src/frontend/ui/views/plan-bits.ts";
import type { PlanSlice } from "../../../src/frontend/ui/plan-reads.ts";

const slice = (extra: Partial<PlanSlice> = {}): PlanSlice => ({ name: "invoice", domain: "软件", bare: false, error: null, stale: null, cells: [], top: [], blocks: [], ...extra }) as unknown as PlanSlice;

describe("无图：哪一片算", () => {
  it("读好了、一格都没有 ⇒ 算", () => {
    expect(isFresh(slice())).toBe(true);
  });
  it("有格 · 读不成又没读好过 · 这一刻读不成 ⇒ 都不算（各有各的那一屏）", () => {
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
});
