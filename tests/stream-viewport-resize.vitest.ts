/**
 * ★ 步 3（`设计/10 §6`）下半的**观察端**：`MessageStream` 到底观察了哪几根轴。
 *
 * # 为什么要单独一个文件
 *
 * `tabs.vitest.ts` 把 `../src/stream` **整个 mock 掉了**（那里量的是 TabManager 的调度，
 * 不是滚动容器本身）⇒ 「RO 有没有观察 `scrollEl`」那一形在那边**盖不住**。
 * 消费端（补不补批）住 `tabs.vitest.ts` 的两格，观察端住这里，**两格是一对，各买各的**。
 *
 * # 这一格钉的是什么
 *
 * 原来只观察 `.stream-content`，那根轴只答「内容长高了吗」。
 * 把窗口拉高 / 收起侧栏 / 拖宽 tab 栏时变的是 **`.stream` 自己**，内容一个字没动
 * ⇒ 旧的 RO 一次都不响，多出来的那块空白没有任何入口去补
 * （`fillAbove` 挂在 scroll 事件上，而**不可滚的元素不产生 scroll 事件**）。
 *
 * 🔴 **两根轴必须分得开**：内容那根不许触发 `onViewportResize` ——
 * 否则每来一条消息都当成"视口变了"补一轮，那是把一个补批入口改成一台永动机。
 *
 * ⚠ 诚实边界：jsdom 没有布局引擎，真 `ResizeObserver` 永远不会自己响。
 * 这里的 RO 是桩，**手动**喂 entries ⇒ 量的是「观察了谁 / 分不分得开」，
 * **不是**「真机上窗口拉高会不会响」。
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { MessageStream } from "../src/stream";

interface Fired {
  target: Element;
}
class FakeResizeObserver {
  static live: FakeResizeObserver[] = [];
  targets: Element[] = [];
  disconnected = false;
  constructor(private cb: (entries: Fired[]) => void) {
    FakeResizeObserver.live.push(this);
  }
  observe(t: Element): void {
    this.targets.push(t);
  }
  unobserve(t: Element): void {
    this.targets = this.targets.filter((x) => x !== t);
  }
  disconnect(): void {
    this.disconnected = true;
    this.targets = [];
  }
  /** 手动喂一次回调（jsdom 无布局 ⇒ 真 RO 不会自己响）。 */
  fire(...targets: Element[]): void {
    this.cb(targets.map((target) => ({ target })));
  }
}

const rootOf = (): HTMLElement => {
  const el = document.createElement("div");
  el.className = "stream";
  document.body.appendChild(el);
  return el;
};

beforeEach(() => {
  document.body.replaceChildren();
  FakeResizeObserver.live = [];
  vi.stubGlobal("ResizeObserver", FakeResizeObserver);
});
afterEach(() => vi.unstubAllGlobals());

describe("步 3：MessageStream 的 ResizeObserver 观察两根轴", () => {
  it("内容与容器都被观察上（只观察内容 ⇒ 窗口拉高那一路一次都不响）", () => {
    const root = rootOf();
    const s = new MessageStream(root);
    const ro = FakeResizeObserver.live[0];
    expect(ro, "构造时应建一个 RO").toBeTruthy();
    expect(ro.targets, "内容那根轴（贴底）不许丢").toContain(s.contentElement);
    expect(ro.targets, "容器那根轴没观察 ⇒ 窗口拉高之后没有任何东西去补那块空白").toContain(root);
  });

  it("🔴 两根轴分得开：只有容器那根触发 onViewportResize", () => {
    const root = rootOf();
    const s = new MessageStream(root);
    const ro = FakeResizeObserver.live[0];
    const seen = vi.fn();
    s.onViewportResize = seen;

    ro.fire(s.contentElement);
    expect(
      seen,
      "内容长高也当成「视口变了」⇒ 每来一条消息补一轮，补批入口变成永动机",
    ).not.toHaveBeenCalled();

    ro.fire(root);
    expect(seen, "容器变了却没人通知 ⇒ 这一步等于没做").toHaveBeenCalledTimes(1);

    // 同一批里两根轴都变（窗口拉高时内容也会重排）⇒ 仍然只报一次。
    ro.fire(s.contentElement, root);
    expect(seen).toHaveBeenCalledTimes(2);
  });

  it("没挂 onViewportResize 时不许抛（宿主可以不接这根轴）", () => {
    const root = rootOf();
    const s = new MessageStream(root);
    const ro = FakeResizeObserver.live[0];
    expect(() => ro.fire(root, s.contentElement)).not.toThrow();
  });

  it("dispose 之后 RO 断开（关 tab 不留观察者）", () => {
    const root = rootOf();
    const s = new MessageStream(root);
    const ro = FakeResizeObserver.live[0];
    s.dispose();
    expect(ro.disconnected).toBe(true);
  });
});
