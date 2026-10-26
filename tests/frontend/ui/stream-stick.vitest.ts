/**
 * 粘底只由人的滚动改：流自己写 `scrollTop` 引来的那一次 `scroll` 不重判粘底。
 *
 * # 为什么要有这一格
 *
 * `scroll` 事件是异步的：`snap()` 写完 scrollTop，要到下一帧才派发；这期间卡片还在长高（字体 · Markdown · 估高换实高），
 * 一帧里浏览器又是先跑 scroll、后跑 ResizeObserver。按派发那一刻的几何重判 ⇒ 读到「离底 24px」⇒ 粘底被解开，
 * 那次长高的 RO 不再贴底，流停在离底二十几像素处；长高晚到一点 ⇒ RO 先贴底、停在底。同一份会话两种样子
 * （截图台架 main-default 连拍：scrollTop 299 / 325）。
 *
 * ⚠ 诚实边界：jsdom 没有布局、也不自己派 scroll；几何靠定义属性演，事件手动派 —— 演的是真浏览器里那一帧的先后。
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { MessageStream } from "../../../src/frontend/ui/stream";

class NoopResizeObserver {
  constructor(readonly cb: (entries: { target: Element }[]) => void) {}
  observe(): void {}
  unobserve(): void {}
  disconnect(): void {}
}

/** 可变几何：scrollTop 写进去照浏览器夹到 [0, sh - ch]。 */
function geometry(el: HTMLElement, g: { sh: number; ch: number; top: number }): typeof g {
  Object.defineProperty(el, "scrollHeight", { configurable: true, get: () => g.sh });
  Object.defineProperty(el, "clientHeight", { configurable: true, get: () => g.ch });
  Object.defineProperty(el, "scrollTop", {
    configurable: true,
    get: () => g.top,
    set: (v: number) => {
      g.top = Math.max(0, Math.min(v, g.sh - g.ch));
    },
  });
  return g;
}

beforeEach(() => {
  document.body.replaceChildren();
  vi.stubGlobal("ResizeObserver", NoopResizeObserver);
});
afterEach(() => vi.unstubAllGlobals());

function streamOf(): { root: HTMLElement; s: MessageStream } {
  const root = document.createElement("div");
  root.className = "stream active";
  document.body.appendChild(root);
  return { root, s: new MessageStream(root) };
}

describe("粘底只由人的滚动改", () => {
  it("自己贴底之后、scroll 派发之前内容又长高 24px：那次 scroll 不解开粘底", () => {
    const { root, s } = streamOf();
    const g = geometry(root, { sh: 734, ch: 734, top: 0 });
    g.sh = 2010;
    s.insertNode(document.createElement("div"), null);
    expect(g.top, "前提：插卡时贴了底").toBe(1276);
    g.sh = 2034; // 卡片在 scroll 派发之前长高
    root.dispatchEvent(new Event("scroll"));
    expect(s.stuckToBottom, "按派发那一刻的几何重判 ⇒ 离底 24 被当成人往上翻了，流停在离底处").toBe(true);
  });

  it("人真往上翻（scrollTop 不是流自己写的那个值）：照旧解开", () => {
    const { root, s } = streamOf();
    const g = geometry(root, { sh: 734, ch: 734, top: 0 });
    g.sh = 2010;
    s.insertNode(document.createElement("div"), null);
    root.dispatchEvent(new Event("scroll"));
    expect(s.stuckToBottom).toBe(true);
    g.top = 900;
    root.dispatchEvent(new Event("scroll"));
    expect(s.stuckToBottom, "人往上翻了 ⇒ 不再贴底").toBe(false);
    g.top = 1270;
    root.dispatchEvent(new Event("scroll"));
    expect(s.stuckToBottom, "翻回底部 24px 内 ⇒ 重新贴底").toBe(true);
  });

  it("自己写过的值只顶一次：之后人停在同一个 scrollTop 上滚动、内容又长了，照常按几何判", () => {
    const { root, s } = streamOf();
    const g = geometry(root, { sh: 734, ch: 734, top: 0 });
    g.sh = 2010;
    s.insertNode(document.createElement("div"), null);
    root.dispatchEvent(new Event("scroll")); // 自己那一次
    g.sh = 2100;
    root.dispatchEvent(new Event("scroll")); // 人的
    expect(s.stuckToBottom).toBe(false);
  });
});
