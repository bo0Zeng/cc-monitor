/**
 * 锚在触发物上的菜单跟着触发物走：菜单开着时，它所在的那个框（对话框）长高 / 变矮把触发物挪了位置 ⇒ 菜单重排、仍贴着触发物下沿。
 *
 * 起因：截图台架 10-12 偶发「浮层不贴着触发物」（起新会话 · 挑账号）：点开账号下拉之后，对话框里晚到的那几格（轮换 · 运行于）才画出来，
 * 对话框居中长高、账号那一格往上挪了 18px，菜单还停在开的那一刻算的位置。真用的时候同样：对话框里的东西晚到一拍，菜单就悬在半空。
 *
 * 人群：`kit/menu.ts::openMenu` 锚在触发物（`{ el }`）上、触发物在一个框（`[role="dialog"]` / `[aria-modal="true"]`）里的菜单。
 * 买不到：jsdom 没有布局，`ResizeObserver` 不会自己响 —— 这里替它报一次「框变了」，触发物的外接框也是桩；
 * 真浏览器里那一下由截图台架那张图（`panel-new-session-account`）的排版量具判。
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { closeMenu, openMenu } from "../../../../src/frontend/ui/kit/menu";
import { nonEmpty } from "../../../test-support/non-empty";

class FakeResizeObserver {
  static live: FakeResizeObserver[] = [];
  readonly seen: Element[] = [];
  constructor(readonly cb: ResizeObserverCallback) {
    FakeResizeObserver.live.push(this);
  }
  observe(el: Element): void {
    this.seen.push(el);
  }
  unobserve(): void {}
  disconnect(): void {
    FakeResizeObserver.live = FakeResizeObserver.live.filter((o) => o !== this);
  }
}

function rect(top: number): DOMRect {
  return {
    left: 100,
    top,
    right: 490,
    bottom: top + 28,
    width: 390,
    height: 28,
    x: 100,
    y: top,
    toJSON: () => ({}),
  } as DOMRect;
}

describe("锚在触发物上的菜单跟着触发物走", () => {
  beforeEach(() => {
    FakeResizeObserver.live = [];
    vi.stubGlobal("ResizeObserver", FakeResizeObserver);
    Object.defineProperty(window, "innerWidth", {
      value: 1280,
      configurable: true,
    });
    Object.defineProperty(window, "innerHeight", {
      value: 800,
      configurable: true,
    });
  });
  afterEach(() => {
    closeMenu();
    document.body.replaceChildren();
    vi.unstubAllGlobals();
  });

  it("对话框变了把触发物挪了 ⇒ 菜单重排到触发物新的下沿", () => {
    const dialog = document.createElement("div");
    dialog.setAttribute("role", "dialog");
    dialog.setAttribute("aria-modal", "true");
    const trigger = document.createElement("button");
    dialog.appendChild(trigger);
    document.body.appendChild(dialog);
    let top = 345;
    trigger.getBoundingClientRect = () => rect(top);
    openMenu({ el: trigger }, [{ label: "work", onClick: () => undefined }]);
    const menu = document.querySelector<HTMLElement>('body > [role="menu"]');
    expect(menu?.style.top).toBe(`${345 + 28 + 4}px`);

    // 对话框里晚到的几格画出来了：对话框居中长高，触发物往上挪 18px；浏览器报「那个框变了」。
    top = 327;
    const watching = FakeResizeObserver.live.filter((o) =>
      o.seen.includes(dialog),
    );
    nonEmpty(watching);
    for (const o of watching) o.cb([], o as unknown as ResizeObserver);
    expect(menu?.style.top).toBe(`${327 + 28 + 4}px`);
  });

  it("菜单关了就不再看那个框", () => {
    const dialog = document.createElement("div");
    dialog.setAttribute("role", "dialog");
    const trigger = document.createElement("button");
    dialog.appendChild(trigger);
    document.body.appendChild(dialog);
    trigger.getBoundingClientRect = () => rect(300);
    openMenu({ el: trigger }, [{ label: "work", onClick: () => undefined }]);
    closeMenu();
    expect(
      FakeResizeObserver.live.filter((o) => o.seen.includes(dialog)),
    ).toEqual([]);
  });
});
