/**
 * 浮层定位（躲窗口边）：菜单 · 浮层面板 · 子菜单 · 悬停提示全走 `kit/place.ts` 那一处。
 *
 * 10-08 用户在 4.1.3 上撞见：点会话头右上角「⋯」，菜单落在窗口中间。病根：触发物贴着窗口右边（右端离窗边 < 8px），
 * 右端对齐算出的左边 + 宽度越过「窗宽 − 8」，旧的 `placeAt` 把它当成「从这一点往右放不下」再往左整整翻一个菜单宽
 * ⇒ 菜单右端离「⋯」差一个菜单宽。下面每条都在旧实现上红过。
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { placeFloat } from "../../../../src/frontend/ui/kit/place";
import { closeMenu, openMenu, updateMenuItem } from "../../../../src/frontend/ui/kit/menu";
import { closePopover, openPopover } from "../../../../src/frontend/ui/kit/popover";

const VIEW = { width: 1024, height: 768 };
type Box = { left: number; top: number; right: number; bottom: number };
const rect = (b: Box): DOMRect => ({ ...b, x: b.left, y: b.top, width: b.right - b.left, height: b.bottom - b.top, toJSON: () => b }) as DOMRect;

/** 每个元素的外接框由表给；浮层（菜单 / 面板）的大小按角色给；不在 DOM 里的 ⇒ 全 0（浏览器就是这样）。 */
const boxes = new Map<Element, Box>();
let floatSize = { width: 240, height: 360 };
beforeEach(() => {
  Object.defineProperty(window, "innerWidth", { value: VIEW.width, configurable: true });
  Object.defineProperty(window, "innerHeight", { value: VIEW.height, configurable: true });
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    if (!this.isConnected) return rect({ left: 0, top: 0, right: 0, bottom: 0 });
    const b = boxes.get(this);
    if (b) return rect(b);
    const role = this.getAttribute("role");
    if (role === "menu" || role === "dialog") return rect({ left: 0, top: 0, right: floatSize.width, bottom: floatSize.height });
    return rect({ left: 0, top: 0, right: 0, bottom: 0 });
  });
});
afterEach(() => {
  closeMenu();
  closePopover();
  vi.restoreAllMocks();
  boxes.clear();
  floatSize = { width: 240, height: 360 };
  document.body.replaceChildren();
});

function anchorAt(b: Box): HTMLButtonElement {
  const el = document.createElement("button");
  document.body.appendChild(el);
  boxes.set(el, b);
  return el;
}
const menuEl = (): HTMLElement => document.querySelector<HTMLElement>('body > [role="menu"]')!;
const at = (el: HTMLElement): { left: number; top: number } => ({ left: parseFloat(el.style.left), top: parseFloat(el.style.top) });

describe("浮层贴着触发物", () => {
  it("★ 会话头「⋯」贴着窗口右边：菜单右端对齐「⋯」右端、在它下方 4px（不往左整翻一个菜单宽）", () => {
    const more = anchorAt({ left: 990, top: 6, right: 1018, bottom: 34 });
    openMenu({ el: more, align: "end" }, [{ label: "项一" }]);
    // 「⋯」右端离窗边只有 6px ⇒ 菜单右端停在窗边内 8px（差 2px），不是差一个菜单宽。
    expect(at(menuEl())).toEqual({ left: 1024 - 8 - 240, top: 38 });
  });

  it("左端对齐的下拉靠右放不下 ⇒ 改右端对齐（贴着触发物往里收），不是从左端往左翻一个宽", () => {
    const sel = anchorAt({ left: 900, top: 100, right: 1000, bottom: 128 });
    openMenu({ el: sel }, [{ label: "work" }]);
    expect(at(menuEl())).toEqual({ left: 1000 - 240, top: 132 });
  });

  it("右端对齐的菜单锚在左边的触发物上（往左放不下）⇒ 改左端对齐", () => {
    floatSize = { width: 200, height: 300 };
    const chip = anchorAt({ left: 10, top: 300, right: 90, bottom: 324 });
    openMenu({ el: chip, align: "end" }, [{ label: "项二" }]);
    expect(at(menuEl())).toEqual({ left: 10, top: 328 });
  });

  it("★ 状态栏上的浮层面板（下面放不下）⇒ 翻到触发物上方 4px，不盖住触发物", () => {
    floatSize = { width: 300, height: 200 };
    const chip = anchorAt({ left: 900, top: 744, right: 1000, bottom: 768 });
    openPopover(chip, document.createElement("div"), { label: "额度" });
    const pop = document.querySelector<HTMLElement>('body > [role="dialog"]')!;
    expect(at(pop)).toEqual({ left: 1000 - 300, top: 744 - 4 - 200 });
  });

  it("★ 开着时触发物被重画掉（不在 DOM 里了、外接框全 0）、菜单项回来再排一次 ⇒ 位置不动，不飞到左上角", () => {
    const more = anchorAt({ left: 990, top: 6, right: 1018, bottom: 34 });
    openMenu({ el: more, align: "end" }, [{ id: "kill", label: "项三", pending: true }]);
    const first = at(menuEl());
    more.remove();
    updateMenuItem("kill", { id: "kill", label: "项三" });
    expect(at(menuEl())).toEqual(first);
  });

  it("★ 子菜单靠右下放不下 ⇒ 翻到左侧、底端对齐那一项（位置由同一处算，不靠 CSS 只翻左右）", () => {
    floatSize = { width: 240, height: 300 };
    openMenu({ x: 700, y: 600 }, [{ label: "项四", submenu: [{ label: "work" }, { label: "home" }] }]);
    const wrap = menuEl().querySelector<HTMLElement>(':scope > [role="none"]')!;
    const fly = wrap.querySelector<HTMLElement>('[role="menu"][data-sub]')!;
    boxes.set(wrap, { left: 784, top: 700, right: 1016, bottom: 728 });
    wrap.querySelector<HTMLButtonElement>(":scope > button")!.click();
    // 视口里：左侧（784 − 240 = 544）· 顶对齐放不下 ⇒ 底端对齐那一项（728 − 300 = 428）；写成相对那一项的偏移。
    expect(at(fly)).toEqual({ left: 544 - 784, top: 428 - 700 });
  });

  it("右键点（一点）：右下放不下 ⇒ 往左上翻（与原先一样）", () => {
    floatSize = { width: 200, height: 100 };
    openMenu({ x: 1000, y: 700 }, [{ label: "项五" }]);
    expect(at(menuEl())).toEqual({ left: 800, top: 600 });
  });
});

describe("placeFloat（纯函数）", () => {
  const r = { left: 100, top: 100, right: 160, bottom: 130 };
  it("下方 · 右端对齐 · 间距；放不下翻上方；两边都放不下 ⇒ 挑空多的那边、夹进窗口 8px", () => {
    expect(placeFloat({ rect: r, side: "below", align: "end", gap: 4 }, { width: 50, height: 40 }, VIEW)).toEqual({ left: 110, top: 134 });
    expect(placeFloat({ rect: { ...r, top: 700, bottom: 730 }, side: "below", align: "start", gap: 4 }, { width: 50, height: 40 }, VIEW)).toEqual({ left: 100, top: 656 });
    expect(placeFloat({ rect: { ...r, top: 300, bottom: 330 }, side: "below", align: "start", gap: 4 }, { width: 50, height: 700 }, VIEW).top).toBe(768 - 8 - 700);
  });
  it("右侧 / 左侧：放不下翻到另一侧；居中对齐的上下夹进窗口；比窗口还宽 ⇒ 贴左边 8px", () => {
    expect(placeFloat({ rect: { ...r, left: 900, right: 1000 }, side: "right", align: "start", gap: 6 }, { width: 100, height: 40 }, VIEW)).toEqual({ left: 794, top: 100 });
    expect(placeFloat({ rect: { ...r, left: 20, right: 28 }, side: "left", align: "center", gap: 6 }, { width: 100, height: 40 }, VIEW)).toEqual({ left: 34, top: 95 });
    expect(placeFloat({ rect: r, side: "below", align: "start", gap: 4 }, { width: 2000, height: 40 }, VIEW).left).toBe(8);
  });
});
