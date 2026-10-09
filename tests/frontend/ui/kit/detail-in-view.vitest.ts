/**
 * 复制详情就地展开（写不进剪贴板）：滚的是出错那一块所在的滚动区，对准那一块（条 / 行）本身 —— 条的顶部留在看得见的地方，
 * 原文框尽量露全；不让焦点把条滚出视野。
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { copyDetailButton } from "../../../../src/frontend/ui/kit/detail";

type Box = { top: number; bottom: number };
const at = (el: Element, r: () => Box): void => {
  el.getBoundingClientRect = () => ({ ...r(), left: 0, right: 100, width: 100, height: r().bottom - r().top, x: 0, y: r().top, toJSON: () => ({}) }) as DOMRect;
};

/** 一个 200 高的滚动区里，出错那一块的顶在内容 `hostTop` 处、原文框高 `boxH`；返回点了之后滚到哪。 */
async function expandIn(hostTop: number, scrolled: number, boxH = 160): Promise<{ scrollTop: number; focusScrolled: boolean }> {
  const sc = document.createElement("div");
  sc.style.overflowY = "auto";
  let top = 0; // jsdom 不排版：滚动位置自己记
  Object.defineProperty(sc, "scrollTop", { configurable: true, get: () => top, set: (v: number) => void (top = Math.max(0, v)) });
  const host = document.createElement("div");
  host.dataset.detailHost = "";
  sc.appendChild(host);
  document.body.appendChild(sc);
  const btn = copyDetailButton("读取画面失败", "码：no_server")!;
  host.appendChild(btn);
  sc.scrollTop = scrolled;
  at(sc, () => ({ top: 0, bottom: 200 }));
  at(host, () => ({ top: hostTop - sc.scrollTop, bottom: hostTop - sc.scrollTop + 40 + boxH }));
  let focusScrolled = false;
  const focus = HTMLTextAreaElement.prototype.focus;
  HTMLTextAreaElement.prototype.focus = function (this: HTMLTextAreaElement, o?: FocusOptions) {
    if (!o?.preventScroll) focusScrolled = true;
    at(this.parentElement!, () => ({ top: hostTop - sc.scrollTop + 40, bottom: hostTop - sc.scrollTop + 40 + boxH }));
    focus.call(this, o);
  };
  try {
    btn.querySelector("button")!.click();
    await vi.advanceTimersByTimeAsync(0);
  } finally {
    HTMLTextAreaElement.prototype.focus = focus;
  }
  return { scrollTop: sc.scrollTop, focusScrolled };
}

describe("复制详情就地展开：出错那一块留在视野里", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    document.body.innerHTML = "";
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText: () => Promise.reject(new Error("denied")) },
    });
  });
  afterEach(() => vi.useRealTimers());

  it("★ 框落到下沿外 ⇒ 往下滚到露全，但条顶不出上沿；焦点不自己滚", async () => {
    const r = await expandIn(150, 0);
    expect(r.focusScrolled, "原文框拿焦点时让浏览器自己滚（会把框底对齐、条滚出去）").toBe(false);
    // 框底在 150+40+160=350，下沿 200 ⇒ 要滚 150；条顶在 150 ⇒ 最多滚 150。
    expect(r.scrollTop).toBe(150);
  });

  it("★ 框比视野还高 ⇒ 只滚到条顶贴上沿，不再往下", async () => {
    const r = await expandIn(120, 0, 400);
    expect(r.scrollTop).toBe(120);
  });

  it("★ 条顶已在上沿之上 ⇒ 滚回到条顶；都看得见 ⇒ 不动", async () => {
    expect((await expandIn(50, 100)).scrollTop).toBe(50);
    document.body.innerHTML = "";
    expect((await expandIn(0, 0, 100)).scrollTop).toBe(0);
  });
});
