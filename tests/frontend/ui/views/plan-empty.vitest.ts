/**
 * 计划页整页那一句（没有片可画时）照后端成品摆：哪台答了 pb 不在 / 太旧 ⇒ 那台那一格的 `said` 上、`hint` 在下（两格都是后端分好的），
 * 找不到 pb 才给［填 pb 路径］；pb 好着、一片都没有 ⇒ 页自己的那一句与［添加工作区］。夹具里后端写的字一律「后端·…」，
 * 照文案键现拼的那一句（mock 成键名）只许出现在页自己那几句上。
 */
import { describe, expect, it, vi } from "vitest";

vi.mock("../../../../src/frontend/ui/copy-table", () => ({
  copyText: (k: string, a?: Record<string, unknown>) => (a ? `${k}(${Object.entries(a).map(([x, y]) => `${x}=${String(y)}`).join(",")})` : k),
}));

import { PlanView } from "../../../../src/frontend/ui/views/plan.ts";
import type { PlanList } from "../../../../src/frontend/ui/plan-reads.ts";
import s from "../../../../src/frontend/ui/views/plan.module.css";

type Pb = PlanList["pb"];
type Inner = { machines: string[]; per: Map<string, unknown>; wholeState(): HTMLElement | null };

/** 一台答了 `plan-list`：pb 那一格照给的，一个工作区都没有。 */
const answered = (pb: Pb): HTMLElement => {
  const v = new PlanView() as unknown as Inner;
  v.per.set(v.machines[0], { state: "ok", list: { pb, workspaces: [] } as PlanList, at: 0 });
  const el = v.wholeState();
  expect(el, "有片可画").not.toBeNull();
  return el!;
};

const buttons = (el: HTMLElement): string[] => [...el.querySelectorAll("button")].map((b) => b.textContent ?? "");
/** 整页那一句里的几行字（图标不算）。 */
const lines = (el: HTMLElement): string[] => [...el.children].filter((c) => c.tagName === "DIV" && c.textContent).map((c) => c.textContent ?? "");

describe("计划页首次加载", () => {
  it("★ 清单还没回来 ⇒ 骨架画在大纲那一栏的位子上（整页居中那一格把它挤成零宽 ⇒ 一片空白）", () => {
    const v = new PlanView() as unknown as { bodyEl: HTMLElement; renderBody(): void };
    v.renderBody();
    const skel = v.bodyEl.querySelector('[aria-hidden="true"]');
    expect(skel, "有骨架").not.toBeNull();
    expect(v.bodyEl.className).not.toBe(s.pvWhole);
    expect(skel!.closest(`.${s.pvTree}`), "骨架在大纲那一栏里").not.toBeNull();
  });
});

describe("计划页整页那一句", () => {
  it("找不到 pb ⇒ 那台写的那一句 · 下一步各一行，给［填 pb 路径］", () => {
    const el = answered({ state: "missing", said: "后端·未找到", hint: "后端·设置里那个路径下没有" });
    expect(lines(el)).toEqual(["后端·未找到", "后端·设置里那个路径下没有"]);
    expect(buttons(el).some((b) => b.includes("plan.empty.setPb"))).toBe(true);
  });

  it("没装、也没有下一步 ⇒ 只那一句，不补一行", () => {
    const el = answered({ state: "missing", said: "后端·未装" });
    expect(lines(el)).toEqual(["后端·未装"]);
  });

  it("pb 太旧 ⇒ 「要更新」与 pb 那一半各一行；没有［填 pb 路径］（换个路径不治旧）", () => {
    const el = answered({ state: "unsupported", said: "后端·要更新", hint: "后端·缺 dump" });
    expect(lines(el)).toEqual(["后端·要更新", "后端·缺 dump"]);
    expect(buttons(el).some((b) => b.includes("plan.empty.setPb"))).toBe(false);
  });

  it("pb 好着、一片都没有 ⇒ 页自己的那一句 · 提示 ·［添加工作区］", () => {
    const el = answered({ state: "ok", said: null });
    expect(lines(el)).toEqual(["plan.empty.text", "plan.empty.hint"]);
    expect(buttons(el).some((b) => b.includes("plan.empty.add"))).toBe(true);
    expect(el.textContent).not.toContain("后端·");
  });
});
