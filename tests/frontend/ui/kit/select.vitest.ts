/**
 * 下拉：框上画当前项（身份块 · 字 · 灰字）· 点开是弹出菜单（宽同框、当前项打勾、不可选的灰着说为什么）·
 * 焦点在框上 ↑↓ 直接换值不展开（跳过不可选）· 换值回调、`setValue` 不回调 · 在模态框里弹出的压过那个框。
 */
import { describe, it, expect, vi, beforeAll, afterEach } from "vitest";
import { select, type SelectOption } from "../../../../src/frontend/ui/kit/select";
import { closeMenu } from "../../../../src/frontend/ui/kit/menu";
import { dispatcher } from "../../../../src/frontend/ui/keybindings/registry";

const menu = (): HTMLElement | null => document.querySelector<HTMLElement>('body > [role="menu"]');
const rows = (): HTMLButtonElement[] => [...(menu()?.querySelectorAll<HTMLButtonElement>('[role^="menuitem"]') ?? [])];
const badge = (t: string) => (): HTMLElement => {
  const b = document.createElement("i");
  b.className = "lead";
  b.textContent = t;
  return b;
};
const OPTS: SelectOption[] = [
  { value: "work", label: "work", note: "默认", lead: badge("W") },
  { value: "api", label: "api", enabled: false, why: "需登录", lead: badge("A") },
  { value: "personal", label: "personal", lead: badge("P") },
];

beforeAll(() => {
  dispatcher.applyOverrides({});
  dispatcher.start();
});
afterEach(() => {
  closeMenu();
  document.body.replaceChildren();
});

describe("下拉", () => {
  it("★ 框上画当前项：身份块 · 字 · 灰字；点开 ⇒ 菜单宽同框、当前项打勾、不可选的灰着说为什么；点一项 ⇒ 换值、回调、菜单关", () => {
    const onChange = vi.fn();
    const h = select({ label: "账号", options: OPTS, value: "work", onChange });
    document.body.appendChild(h.el);
    expect([h.el.getAttribute("aria-label"), h.el.dataset.value, h.el.querySelector(".lead")?.textContent, h.el.textContent]).toEqual(["账号", "work", "W", "Wwork默认"]);
    h.el.click();
    expect(menu()).not.toBeNull();
    expect(rows().map((r) => r.getAttribute("aria-checked"))).toEqual(["true", "false", "false"]);
    expect(rows()[1].disabled, "不可选的灰着").toBe(true);
    expect(rows()[1].textContent).toContain("需登录");
    rows()[2].click();
    expect([h.value(), h.el.dataset.value, h.el.querySelector(".lead")?.textContent, menu()]).toEqual(["personal", "personal", "P", null]);
    expect(onChange).toHaveBeenCalledWith("personal");
  });

  it("★ 焦点在框上 ↑↓ 直接换值不展开、跳过不可选、到头停住；Alt+↓ 展开", () => {
    const onChange = vi.fn();
    const h = select({ label: "账号", options: OPTS, value: "work", onChange });
    document.body.appendChild(h.el);
    const press = (key: string, altKey = false) => h.el.dispatchEvent(new KeyboardEvent("keydown", { key, altKey, bubbles: true }));
    press("ArrowDown");
    expect([h.value(), menu()]).toEqual(["personal", null]);
    press("ArrowDown");
    expect(h.value(), "到头停住").toBe("personal");
    press("ArrowUp");
    expect(h.value(), "跳过不可选").toBe("work");
    expect(onChange.mock.calls.map((c) => c[0])).toEqual(["personal", "work"]);
    press("ArrowDown", true);
    expect(menu()).not.toBeNull();
  });

  it("setValue / setOptions 不回调；换了选项、原值不在了 ⇒ 落到第一个能选的", () => {
    const onChange = vi.fn();
    const h = select({ label: "机器", options: OPTS, onChange });
    expect(h.value(), "没给值 ⇒ 第一个能选的").toBe("work");
    h.setValue("personal");
    h.setOptions([{ value: "a", label: "a", enabled: false }, { value: "b", label: "b" }]);
    expect(h.value()).toBe("b");
    expect(onChange).not.toHaveBeenCalled();
    h.setDisabled(true);
    expect(h.el.disabled).toBe(true);
  });

  it("在模态框里弹出 ⇒ 菜单压过那个框（不被背板盖住）；框外弹出的照旧", () => {
    const modal = document.createElement("div");
    modal.setAttribute("aria-modal", "true");
    const inside = select({ label: "账号", options: OPTS });
    modal.appendChild(inside.el);
    document.body.appendChild(modal);
    inside.el.click();
    expect(menu()?.dataset.overModal).toBe("true");
    closeMenu();
    const outside = select({ label: "账号", options: OPTS });
    document.body.appendChild(outside.el);
    outside.el.click();
    expect(menu()?.dataset.overModal).toBeUndefined();
  });
});
