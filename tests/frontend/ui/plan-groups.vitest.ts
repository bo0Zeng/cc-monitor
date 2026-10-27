/**
 * 计划页 ↔ 组：块表「组」那一格照后端写的字摆（组模块没答 ⇒ 警示 ＋ 悬停那一句，不画成空）· 按组筛块 · 组菜单进计划页先落哪一片。
 * 期望手写，夹具是合成的中性例子。
 */
import { describe, expect, it, vi } from "vitest";

vi.mock("../../../src/frontend/ui/copy-table", () => ({
  copyText: (k: string, a?: Record<string, unknown>) => (a ? `${k}(${Object.entries(a).map(([x, y]) => `${x}=${String(y)}`).join(",")})` : k),
}));

import { blocksOfGroup, groupsCell, sliceForGroup } from "../../../src/frontend/ui/views/plan-groups.ts";
import type { PlanBlock, PlanList } from "../../../src/frontend/ui/plan-reads.ts";

type G = PlanBlock["groups"];
const block = (id: string, groups: G): PlanBlock => ({ id, cells: [id], groups }) as unknown as PlanBlock;
// 那一格的字由后端写好：夹具里随便写一个，断言只看照不照摆。
const read = (...gs: [string, string][]): G => ({ groups: gs.map(([id, title]) => ({ id, title })), text: gs.length ? gs.map((g) => g[1]).join(" / ") : "-", unread: null });
const failed: G = { groups: [], text: "-", unread: "读取组失败（夹具）" };

describe("块表「组」那一格", () => {
  it("读到 ⇒ 照后端写的字，不带警示", () => {
    const el = groupsCell(block("A2", read(["g1", "开发"], ["g2", "评审"])));
    expect(el.textContent).toBe("开发 / 评审");
    expect(el.dataset.tone).toBeUndefined();
    expect(el.getAttribute("aria-label")).toBeNull();
  });
  it("组模块没答 ⇒ 字照摆、警示、那一句进悬停与读屏（不画成空）", () => {
    const el = groupsCell(block("A2", failed));
    expect(el.textContent).toBe("-");
    expect(el.dataset.tone).toBe("warn");
    expect(el.getAttribute("aria-label")).toBe("读取组失败（夹具）");
  });
});

describe("按组筛块", () => {
  it("只留读到的组里有这一组 id 的那几块；没答的 · 没有接手会话的不算", () => {
    const bs = [block("A1", read(["g1", "开发"])), block("A2", read(["g2", "评审"])), block("A3", failed), block("A4", read())];
    expect(blocksOfGroup(bs, "g1").map((b) => b.id)).toEqual(["A1"]);
    expect(blocksOfGroup(bs, "g9")).toEqual([]);
  });
});

describe("组菜单进计划页先落哪一片", () => {
  const list = (ws: string, bySession: Record<string, { slice: string }>): PlanList =>
    ({ pb: { state: "ok", said: null }, workspaces: [{ workspace: ws, bySession, slices: [] }] }) as unknown as PlanList;
  it("照各台 plan-list 的 bySession：头一个接了块的组员（同一台）所在那一片", () => {
    const lists = new Map([
      ["<local>", list("/w/a", { s9: { slice: "other" } })],
      ["devbox", list("/w/b", { s2: { slice: "site" } })],
    ]);
    const got = sliceForGroup(lists, [
      { origin: "<local>", sid: "s1" },
      { origin: "devbox", sid: "s2" },
    ]);
    expect(got).toEqual({ origin: "devbox", workspace: "/w/b", name: "site" });
  });
  it("组员在别的台接的块不算到这台；谁都没接 ⇒ null", () => {
    const lists = new Map([["<local>", list("/w/a", { s2: { slice: "x" } })]]);
    expect(sliceForGroup(lists, [{ origin: "devbox", sid: "s2" }])).toBeNull();
  });
});
