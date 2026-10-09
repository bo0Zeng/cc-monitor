/**
 * 切走的 tab 那条流「停放」（`MessageStream.park`）：停放期间几何读数不作数，粘不粘底照切走那一刻的。
 *
 * # 为什么要有这一格
 *
 * 切走的 tab 用 `content-visibility: hidden` 收起（`styles.css` 的 `.stream`）—— 浏览器跳过它整棵子树的样式 / 布局 / 绘制，
 * 切回来时沿用收起前的渲染状态。代价是收起期间那条流的几何**不可信**：Chromium 在收起期间读 `scrollTop` 得 0、
 * `scrollHeight` 得视口高（滚动位置本身留着，翻出来才还原）；而收起那一下滚动位置变了会来一次 `scroll`。
 * 不停放 ⇒ 这次 `scroll` 按「0 / 视口高」算成贴着底 ⇒ 用户往上翻着看的 tab 切回来被拽到底；
 * 后台来新行时按假读数去贴底 ⇒ 往一个收起的容器里写 `scrollTop`（翻出来时被浏览器还原的旧位置盖掉，白写还强制排版）。
 *
 * ⚠ 诚实边界：jsdom 没有布局，几何靠给元素定义属性来演「收起期间的读数」；真浏览器里收起会不会来 `scroll` 由性能台架看。
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { MessageStream } from "../../../src/frontend/ui/stream";

class NoopResizeObserver {
  observe(): void {}
  unobserve(): void {}
  disconnect(): void {}
}

/** 给滚动容器定几何（jsdom 恒 0）；`scrollTop` 可写、写过记下来。 */
function geometry(el: HTMLElement, g: { sh: number; ch: number; top: number }): { writes: number[] } {
  const rec = { writes: [] as number[] };
  let top = g.top;
  Object.defineProperty(el, "scrollHeight", { configurable: true, get: () => g.sh });
  Object.defineProperty(el, "clientHeight", { configurable: true, get: () => g.ch });
  Object.defineProperty(el, "scrollTop", {
    configurable: true,
    get: () => top,
    set: (v: number) => {
      rec.writes.push(v);
      top = v;
    },
  });
  return rec;
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

describe("切走的 tab 那条流停放：收起期间的几何不作数", () => {
  it("往上翻着看的 tab：收起那一下的 scroll（读到 0 / 视口高）不许把它算成贴着底", () => {
    const { root, s } = streamOf();
    geometry(root, { sh: 9000, ch: 600, top: 1000 });
    root.dispatchEvent(new Event("scroll"));
    expect(s.stuckToBottom, "前提：往上翻了 ⇒ 不贴底").toBe(false);

    s.park(true);
    // 收起期间 Chromium 的读数：scrollTop 0、scrollHeight 等于视口高
    geometry(root, { sh: 600, ch: 600, top: 0 });
    root.dispatchEvent(new Event("scroll"));
    expect(s.stuckToBottom, "按收起期间的假读数算成贴底 ⇒ 切回来被拽到底，翻到的位置丢了").toBe(false);

    s.park(false);
    geometry(root, { sh: 9000, ch: 600, top: 1000 });
    root.dispatchEvent(new Event("scroll"));
    expect(s.stuckToBottom, "翻出来之后照常按真读数算").toBe(false);
  });

  it("贴着底的 tab 收起期间来新卡：不往收起的容器里写 scrollTop（翻出来由宿主贴底）", () => {
    const { root, s } = streamOf();
    const g = geometry(root, { sh: 9000, ch: 600, top: 8400 });
    expect(s.stuckToBottom).toBe(true);
    s.park(true);
    geometry(root, { sh: 9600, ch: 600, top: 8400 }); // 落后底部 600px
    const rec = geometry(root, { sh: 9600, ch: 600, top: 8400 });
    s.insertNode(document.createElement("div"), null);
    s.batchInsert(() => {});
    expect(rec.writes, "收起期间贴底：白写（翻出来被还原的旧位置盖掉），还逼一次排版").toEqual([]);
    expect(g.writes).toEqual([]);
    expect(s.stuckToBottom, "停放不改粘底状态").toBe(true);

    s.park(false);
    s.insertNode(document.createElement("div"), null);
    expect(rec.writes.length, "翻出来之后照常贴底").toBe(1);
  });
});
