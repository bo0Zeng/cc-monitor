/**
 * 悬停提示（C20，`kit/tooltip.ts`）：不许在 body 上越攒越多（E60，经设置里的 `?` 图标测）· 500ms 才出 · 同组立刻换 · 躲窗口边。
 *
 * E60：tooltip **不许在 body 上越攒越多**。
 *
 * 原来构造时就 `appendChild(document.body)`，而全文件没有任何回收路径。
 * `rebuildCards()` 每次重建全部 `MachineCard`、每开一次设置窗跑两遍，调用点已从 16 涨到 24。
 * `settings-ia/STATUS.md` 自己立过硬前置「**必须先于任何页面化**」，而页面化已经做完了 ——
 * **门是自己立的，越过去了，且没有任何门禁会红**。这个文件就是那道门禁。
 *
 * 修法不是补 `destroy()`（那要 24 个调用点每一个都记得调 —— 一个靠自觉维持的不变量
 * 迟早会破，而且破了照样没人知道），而是让 tooltip **只在显示期间存在**。
 */
import { describe, it, expect, beforeEach, vi, afterEach } from "vitest";
import { makeInfoIcon } from "../../../../src/frontend/ui/settings/info-icon";
import { attachTooltip, liveTooltipCount, placeTip, TOOLTIP_DELAY_MS } from "../../../../src/frontend/ui/kit/tooltip";

const tipsInBody = () => document.querySelectorAll('[role="tooltip"]').length;
const hover = (el: HTMLElement) => el.dispatchEvent(new Event("mouseenter"));
const leave = (el: HTMLElement) => el.dispatchEvent(new Event("mouseleave"));

describe("E60：tooltip 不泄漏", () => {
  beforeEach(() => document.body.replaceChildren());

  it("★★ 造 24 个图标（= 今天的真实调用点数）而**一次都不悬停** → body 上零 tooltip", () => {
    const host = document.createElement("div");
    document.body.appendChild(host);
    for (let i = 0; i < 24; i += 1) host.appendChild(makeInfoIcon(`说明 ${i}`));
    expect(tipsInBody(), "构造即 append 就是原来那条泄漏").toBe(0);
  });

  it("★★ 重建 100 次（模拟 rebuildCards）→ 仍然零残留", () => {
    for (let round = 0; round < 100; round += 1) {
      const host = document.createElement("div");
      document.body.appendChild(host);
      host.appendChild(makeInfoIcon("说明"));
      host.remove(); // rebuildCards 就是这么干的
    }
    expect(tipsInBody()).toBe(0);
    expect(liveTooltipCount()).toBe(0);
  });

  it("悬停时 tooltip 才出现，离开即从 DOM 摘掉（不是只 display:none）", () => {
    const icon = makeInfoIcon("这是说明");
    document.body.appendChild(icon);
    expect(tipsInBody()).toBe(0);

    hover(icon);
    expect(tipsInBody(), "悬停了却没显示 —— 功能坏了").toBe(1);
    expect(document.querySelector('[role="tooltip"]')?.textContent).toBe("这是说明");

    leave(icon);
    expect(tipsInBody(), "只 display:none 的话这里会是 1 —— 那正是原来的泄漏").toBe(0);
  });

  it("focus / blur 与鼠标同权（键盘可达性不能因为这次改动丢掉）", () => {
    const icon = makeInfoIcon("说明");
    document.body.appendChild(icon);
    icon.dispatchEvent(new Event("focusin"));
    expect(tipsInBody()).toBe(1);
    icon.dispatchEvent(new Event("focusout"));
    expect(tipsInBody()).toBe(0);
  });

  it("反复悬停同一个图标不会攒出多条", () => {
    const icon = makeInfoIcon("说明");
    document.body.appendChild(icon);
    for (let i = 0; i < 10; i += 1) {
      hover(icon);
      leave(icon);
    }
    hover(icon);
    expect(tipsInBody()).toBe(1);
  });

  /**
   * ★ 唯一一个 `hide` 兜不住的时序：**正显示着的时候图标被销毁**
   *（`rebuildCards()` 在鼠标悬停期间跑）—— 此时 `mouseleave` 永远不会来。
   * 由下一次显示前的 `sweepOrphanTooltips()` 清掉，残留上限恒为 1 条。
   */
  it("★ 悬停中被销毁 → 下一次悬停时把孤儿扫掉，残留不累积", () => {
    for (let round = 0; round < 5; round += 1) {
      const host = document.createElement("div");
      document.body.appendChild(host);
      const icon = makeInfoIcon(`说明 ${round}`);
      host.appendChild(icon);
      hover(icon); // 显示中
      host.remove(); // 主人没了，mouseleave 永不到来
      expect(tipsInBody(), "此刻确实残留一条（这是已知且有上限的）").toBe(1);

      // 下一轮的悬停会先扫
      const next = makeInfoIcon("下一个");
      document.body.appendChild(next);
      hover(next);
      expect(tipsInBody(), "孤儿没被扫掉 —— 残留会随重建次数累积").toBe(1);
      leave(next);
      next.remove();
    }
    expect(tipsInBody()).toBe(0);
  });

  it("aria-label 仍带全文（tooltip 不在 DOM 里时，读屏靠它）", () => {
    const icon = makeInfoIcon("多行\n说明");
    expect(icon.getAttribute("aria-label")).toBe("多行\n说明");
  });
});


describe("C20：出现时机与摆法", () => {
  beforeEach(() => {
    document.body.replaceChildren();
    vi.useFakeTimers();
  });
  afterEach(() => vi.useRealTimers());

  it("悬停 500ms 才出；没到就离开 ⇒ 不出", () => {
    vi.advanceTimersByTime(1000); // 与上一条用例里最后收起的那条拉开（不算同一组）
    const a = document.createElement("button");
    document.body.appendChild(a);
    attachTooltip(a, "刷新");
    a.dispatchEvent(new Event("mouseenter"));
    vi.advanceTimersByTime(TOOLTIP_DELAY_MS - 1);
    expect(tipsInBody()).toBe(0);
    vi.advanceTimersByTime(1);
    expect(tipsInBody()).toBe(1);
    a.dispatchEvent(new Event("mouseleave"));
    expect(tipsInBody()).toBe(0);
    vi.advanceTimersByTime(1000); // 离开够久：不再算「同一组里移过去」
    a.dispatchEvent(new Event("mouseenter"));
    a.dispatchEvent(new Event("mouseleave"));
    vi.advanceTimersByTime(TOOLTIP_DELAY_MS * 2);
    expect(tipsInBody(), "没等到就离开了还出了").toBe(0);
  });

  it("同一组里移到下一个：立刻换，不再等；内容在显示那一刻现取", () => {
    let chord = "Ctrl+R";
    const [a, b] = [document.createElement("button"), document.createElement("button")];
    document.body.append(a, b);
    attachTooltip(a, "刷新");
    attachTooltip(b, () => `重新连接 ${chord}`);
    a.dispatchEvent(new Event("mouseenter"));
    vi.advanceTimersByTime(TOOLTIP_DELAY_MS);
    a.dispatchEvent(new Event("mouseleave"));
    chord = "F5";
    b.dispatchEvent(new Event("mouseenter"));
    expect(document.querySelector('[role="tooltip"]')?.textContent).toBe("重新连接 F5");
  });

  it("躲窗口边：上面放不下翻到下方；左右夹进视口 8px", () => {
    const host = { left: 2, top: 4, width: 20, height: 20, right: 22, bottom: 24 } as DOMRect;
    const at = placeTip(host, { width: 100, height: 30 }, { width: 300, height: 200 });
    expect(at.top).toBeGreaterThan(24);
    expect(at.left).toBe(8);
    const right = placeTip({ ...host, left: 290, right: 310 } as DOMRect, { width: 100, height: 30 }, { width: 300, height: 200 });
    expect(right.left).toBe(300 - 8 - 100);
  });
});
