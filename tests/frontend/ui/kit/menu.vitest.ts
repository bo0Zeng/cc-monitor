/**
 * 弹出菜单（C12）：躲窗口边 · 关法（Esc 走弹层栈 · 点外面 · 再点触发物 · 选了一项）· 同一时刻一个 · 键盘 · 不可选的灰着说为什么 · 开着时换项 / 追加 / 摘掉。
 */
import { describe, it, expect, vi, beforeAll, beforeEach, afterEach } from "vitest";
import {
  appendMenuItem,
  closeMenu,
  menuGeneration,
  menuOpen,
  openMenu,
  placeAt,
  removeMenuItem,
  updateMenuItem,
} from "../../../../src/frontend/ui/kit/menu";
import { dispatcher } from "../../../../src/frontend/ui/keybindings/registry";

const menus = (): HTMLElement[] => [...document.querySelectorAll<HTMLElement>('body > [role="menu"]')];
const items = (): HTMLButtonElement[] => [...(menus()[0]?.querySelectorAll<HTMLButtonElement>(':scope > [role^="menuitem"]') ?? [])];
const escape = (): void => void window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", code: "Escape", bubbles: true }));
const key = (k: string): void => void menus()[0].dispatchEvent(new KeyboardEvent("keydown", { key: k, bubbles: true }));

beforeAll(() => {
  dispatcher.applyOverrides({});
  dispatcher.start();
});
beforeEach(() => vi.useFakeTimers());
afterEach(() => {
  closeMenu();
  vi.useRealTimers();
  document.body.replaceChildren();
});

describe("C12 弹出菜单", () => {
  it("躲窗口边：靠右靠下放不下 ⇒ 往左往上翻；比窗口还大 ⇒ 贴边内缩 8px", () => {
    expect(placeAt(10, 10, 100, 50, 800, 600)).toEqual({ left: 10, top: 10 });
    expect(placeAt(780, 590, 100, 50, 800, 600)).toEqual({ left: 680, top: 540 });
    expect(placeAt(5, 5, 900, 700, 800, 600)).toEqual({ left: 8, top: 8 });
  });

  it("选了一项：做它、菜单关；危险项 data-variant；不可选的 disabled ＋ 说为什么、点了不做", () => {
    const run = vi.fn();
    const skip = vi.fn();
    openMenu({ x: 5, y: 5 }, [
      { label: "在新窗口打开", onClick: run },
      { label: "预览", enabled: false, title: "devbox 要更新", onClick: skip },
      { label: "", divider: true },
      { label: "结束会话", danger: true },
    ]);
    const [a, b, , d] = [...menus()[0].children] as HTMLButtonElement[];
    expect([b.disabled, b.title, d.dataset.variant]).toEqual([true, "devbox 要更新", "danger"]);
    b.click();
    expect(skip).not.toHaveBeenCalled();
    a.click();
    expect([run.mock.calls.length, menuOpen(), menus().length]).toEqual([1, false, 0]);
  });

  it("组名一行只读、不进键盘走位；单选组：点一项 ⇒ 勾挪到它（同组别的去勾）、菜单不关、调它的 onClick；别的组不动；跟着单选走的灰字随之重取", () => {
    const picked: string[] = [];
    let words = "work · 不用 tmux";
    openMenu({ x: 5, y: 5 }, [
      { label: "恢复", detailOf: () => words },
      { label: "账号", heading: true },
      { label: "work", radio: "account", checked: true, onClick: () => picked.push("work") },
      { label: "home", radio: "account", onClick: () => ((words = "home · 不用 tmux"), picked.push("home")) },
      { label: "运行于", heading: true },
      { label: "在 tmux 里", radio: "run", onClick: () => picked.push("tmux") },
      { label: "不用 tmux", radio: "run", checked: true },
    ]);
    const heads = [...menus()[0].querySelectorAll('[role="presentation"]')].map((e) => e.textContent);
    expect(heads).toEqual(["账号", "运行于"]);
    const [resume, work, home, tmux, direct] = items();
    expect(items().map((b) => b.getAttribute("role"))).toEqual(["menuitem", "menuitemradio", "menuitemradio", "menuitemradio", "menuitemradio"]);
    expect([work, home, tmux, direct].map((b) => b.getAttribute("aria-checked"))).toEqual(["true", "false", "false", "true"]);
    home.click();
    expect(menuOpen(), "点单选菜单不关").toBe(true);
    expect([work, home, tmux, direct].map((b) => b.getAttribute("aria-checked"))).toEqual(["false", "true", "false", "true"]);
    expect([work.querySelector("svg"), home.querySelector("svg")].map((x) => x !== null), "勾跟着挪").toEqual([false, true]);
    expect(resume.querySelector('[data-part="detail"]')?.textContent).toBe("home · 不用 tmux");
    tmux.click();
    expect([work, home, tmux, direct].map((b) => b.getAttribute("aria-checked"))).toEqual(["false", "true", "true", "false"]);
    expect(picked).toEqual(["home", "tmux"]);
    key("ArrowDown");
    expect(document.activeElement, "组名那一行不进走位").toBe(resume);
  });

  it("同一时刻一个；Esc 只关菜单（弹层栈）、不连带下面那层", () => {
    let under = 0;
    const below = { handleEsc: () => ((under += 1), true) };
    dispatcher.pushOverlay(below);
    openMenu({ x: 1, y: 1 }, [{ label: "a" }]);
    openMenu({ x: 2, y: 2 }, [{ label: "b" }]);
    expect(menus().map((m) => m.textContent)).toEqual(["b"]);
    escape();
    expect([menus().length, under]).toEqual([0, 0]);
    escape();
    expect(under).toBe(1);
    dispatcher.popOverlay(below);
  });

  it("锚在触发物上：再点触发物 ＝ 关（不是先关又开）；点外面关；aria-expanded 跟着", () => {
    const btn = document.createElement("button");
    document.body.appendChild(btn);
    expect(openMenu({ el: btn }, [{ label: "devbox" }])).toBe(true);
    expect(btn.getAttribute("aria-expanded")).toBe("true");
    expect(openMenu({ el: btn }, [{ label: "devbox" }]), "再点触发物又开了一个").toBe(false);
    expect([menus().length, btn.getAttribute("aria-expanded")]).toEqual([0, "false"]);
    openMenu({ el: btn }, [{ label: "devbox" }]);
    document.body.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true }));
    expect(menus().length).toBe(0);
  });

  it("右键开的：开菜单那一下自己的 pointerdown 不算点外面（下一拍才挂）", () => {
    openMenu({ x: 1, y: 1 }, [{ label: "a" }]);
    document.body.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true }));
    expect(menus().length).toBe(1);
    vi.advanceTimersByTime(1);
    document.body.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true }));
    expect(menus().length).toBe(0);
  });

  it("键盘：↓↑ 走、跳过不可选、Home / End 到两头", () => {
    openMenu({ x: 1, y: 1 }, [{ label: "a" }, { label: "b", enabled: false }, { label: "c" }]);
    const [a, , c] = items();
    key("ArrowDown");
    expect(document.activeElement).toBe(a);
    key("ArrowDown");
    expect(document.activeElement, "落到了不可选的那一项").toBe(c);
    key("ArrowDown");
    expect(document.activeElement).toBe(a);
    key("End");
    expect(document.activeElement).toBe(c);
    key("Home");
    expect(document.activeElement).toBe(a);
  });

  it("子菜单：悬停 150ms 展开、离开 250ms 收起；→ 开并进第一项，← 收回到父项；当前项打勾", () => {
    openMenu({ x: 1, y: 1 }, [{ label: "恢复", submenu: [{ label: "work", checked: true }, { label: "home", checked: false }] }]);
    const wrap = menus()[0].querySelector<HTMLElement>('[role="none"]')!;
    const parent = wrap.querySelector<HTMLButtonElement>("button")!;
    wrap.dispatchEvent(new MouseEvent("mouseenter"));
    vi.advanceTimersByTime(149);
    expect(wrap.dataset.subOpen).toBeUndefined();
    vi.advanceTimersByTime(1);
    expect(wrap.dataset.subOpen).toBe("true");
    wrap.dispatchEvent(new MouseEvent("mouseleave"));
    vi.advanceTimersByTime(250);
    expect(wrap.dataset.subOpen).toBeUndefined();
    parent.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true }));
    const sub = wrap.querySelectorAll<HTMLButtonElement>('[role="menuitemradio"]');
    expect([document.activeElement, sub[0].getAttribute("aria-checked"), sub[0].querySelectorAll("svg").length]).toEqual([sub[0], "true", 1]);
    sub[0].dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowLeft", bubbles: true }));
    expect([document.activeElement, wrap.dataset.subOpen]).toEqual([parent, undefined]);
  });

  it("开着时按 id 换项（展开态带过去）· 追加 · 摘掉；代次：开 / 关都换一代，关了之后改不到", () => {
    const g0 = menuGeneration();
    openMenu({ x: 1, y: 1 }, [{ id: "attach", label: "检测中", enabled: false }, { id: "x", label: "x" }]);
    const g1 = menuGeneration();
    expect(g1).toBeGreaterThan(g0);
    updateMenuItem("attach", { id: "attach", label: "接回 orders" });
    appendMenuItem({ label: "用 work 恢复" });
    removeMenuItem("x");
    expect(items().map((b) => [b.textContent, b.disabled])).toEqual([["接回 orders", false], ["用 work 恢复", false]]);
    closeMenu();
    expect(menuGeneration()).toBeGreaterThan(g1);
    updateMenuItem("attach", { label: "迟到的" });
    expect(menus().length).toBe(0);
  });
});
