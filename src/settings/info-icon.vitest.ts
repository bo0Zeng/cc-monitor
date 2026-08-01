/**
 * E60：`makeInfoIcon` 的 tooltip **不许在 body 上越攒越多**。
 *
 * 原来构造时就 `appendChild(document.body)`，而全文件没有任何回收路径。
 * `rebuildCards()` 每次重建全部 `MachineCard`、每开一次设置窗跑两遍，调用点已从 16 涨到 24。
 * `settings-ia/STATUS.md` 自己立过硬前置「**必须先于任何页面化**」，而页面化已经做完了 ——
 * **门是自己立的，越过去了，且没有任何门禁会红**。这个文件就是那道门禁。
 *
 * 修法不是补 `destroy()`（那要 24 个调用点每一个都记得调 —— 一个靠自觉维持的不变量
 * 迟早会破，而且破了照样没人知道），而是让 tooltip **只在显示期间存在**。
 */
import { describe, it, expect, beforeEach } from "vitest";
import { makeInfoIcon, __liveTooltipCountForTests, swapFileName } from "./info-icon";

const tipsInBody = () => document.querySelectorAll(".settings-info-tooltip").length;
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
    expect(__liveTooltipCountForTests()).toBe(0);
  });

  it("悬停时 tooltip 才出现，离开即从 DOM 摘掉（不是只 display:none）", () => {
    const icon = makeInfoIcon("这是说明");
    document.body.appendChild(icon);
    expect(tipsInBody()).toBe(0);

    hover(icon);
    expect(tipsInBody(), "悬停了却没显示 —— 功能坏了").toBe(1);
    expect(document.querySelector(".settings-info-tooltip")?.textContent).toBe("这是说明");

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

describe("swapFileName（同文件的既有工具，顺带钉住）", () => {
  it("两种分隔符都认，无分隔符时整体替换", () => {
    expect(swapFileName("C:\\a\\b\\Microsoft.PowerShell_profile.ps1", "profile.ps1")).toBe(
      "C:\\a\\b\\profile.ps1",
    );
    expect(swapFileName("/home/u/x.ps1", "profile.ps1")).toBe("/home/u/profile.ps1");
    expect(swapFileName("x.ps1", "profile.ps1")).toBe("profile.ps1");
  });
});
