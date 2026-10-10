// 状态栏「上下文」chip 的 jsdom 测试：字照抄核心写好的（百分比 / 用了多少）· 颜色按核心的语气 · 无 usage 不渲染 · 点开浮层与再点关。

import { describe, it, expect, beforeEach } from "vitest";
import { UsageHud } from "../../../src/frontend/ui/usage-hud";
import { closePopover } from "../../../src/frontend/ui/kit/popover";
import { copyText } from "../../../src/frontend/ui/copy-table";
import type { UsageFact } from "../../../src/frontend/ui/session-reads";

const text = (hud: UsageHud): string => hud.summaryElement.textContent ?? "";
const intent = (hud: UsageHud): string | undefined => hud.summaryElement.dataset.intent;
const panel = (): HTMLElement | null => document.querySelector<HTMLElement>(`[role=dialog][aria-label='${copyText("usageHud.panel.label")}']`);

/** 一份后端成品（字是核心写的；这里只是夹具，界面不许自己算它们）。 */
const usage = (over: Partial<UsageFact> = {}): UsageFact => ({
  promptTokens: 350_000,
  model: "claude-opus-5-5",
  peakPromptTokens: 350_000,
  limit: 1_000_000,
  limitFrom: "model",
  percent: 35,
  contextText: "核心·35",
  contextTone: "plain",
  promptTokensText: "核心·350k",
  limitText: "核心·1M",
  limitFromText: "核心·模型名",
  ...over,
});
const blind = (): UsageFact => usage({ limitFrom: "assumed", percent: null, contextText: "核心·350k", limitFromText: null });

beforeEach(() => {
  closePopover();
  document.body.innerHTML = "";
});

describe("状态栏「上下文」chip", () => {
  it("★ 字照抄核心写好的那一格（判得出写百分比、判不出写用了多少都是核心的事），可见", () => {
    const hud = new UsageHud();
    hud.setActive(usage());
    expect(text(hud)).toBe(copyText("usageHud.chip.ctx", { ctx: "核心·35" }));
    expect(hud.summaryElement.style.display).toBe("");
    expect(intent(hud)).toBe("neutral");
    hud.setActive(blind());
    expect(text(hud)).toBe(copyText("usageHud.chip.ctx", { ctx: "核心·350k" }));
  });

  it("★ 颜色按核心的语气：warn ⇒ 琥珀；切到没有一轮回复的会话 ⇒ 不渲染、颜色清干净", () => {
    const hud = new UsageHud();
    hud.setActive(usage({ percent: 85, contextTone: "warn" }));
    expect(intent(hud)).toBe("warn");
    // 百分比再高，核心没说 warn 就不上色（界面不按数自己判）。
    hud.setActive(usage({ percent: 99, contextTone: "plain" }));
    expect(intent(hud)).toBe("neutral");
    hud.setActive(usage({ contextTone: "warn" }));
    hud.setActive(null);
    expect(hud.summaryElement.style.display).toBe("none");
    expect(intent(hud)).toBe("neutral");
  });

  it("事实要不到 ⇒ 上下文 —（压过旧的数）；浮层里错误条＋［重试］交宿主", () => {
    let retried = 0;
    const hud = new UsageHud();
    hud.host = { openSettings: () => {}, retry: () => retried++ };
    document.body.appendChild(hud.summaryElement);
    hud.setActive(usage({ contextTone: "warn" }));
    hud.setUnavailable(copyText("events.unseen.unreachableTitle", { machine: "devbox" }));
    expect([text(hud), intent(hud)]).toEqual([copyText("usageHud.chip.none"), "neutral"]);
    hud.summaryElement.click();
    expect(panel()?.textContent).toContain(copyText("usageHud.panel.unavailable"));
    [...(panel()?.querySelectorAll("button") ?? [])].find((b) => b.textContent === copyText("usageHud.panel.retry"))?.click();
    expect([retried, panel()]).toEqual([1, null]);
  });

  it("★ 点开浮层：那一格 · 最新一轮 / 上限 · 模型 · 上限来源（都照抄核心）；再点 chip 关；［改上限…］去设置", () => {
    let settings = 0;
    const hud = new UsageHud();
    hud.host = { openSettings: () => settings++, retry: () => {} };
    document.body.appendChild(hud.summaryElement);
    hud.setActive(usage({ model: "claude-opus-5-5[1m]", limitFrom: "setting", limitFromText: "核心·设置" }));
    hud.summaryElement.click();
    const p = panel();
    expect(p?.textContent).toContain(copyText("usageHud.chip.ctx", { ctx: "核心·35" }));
    expect(p?.textContent).toContain(copyText("usageHud.panel.usage", { tok: "核心·350k", limit: "核心·1M", model: "claude-opus-5-5" }));
    expect(p?.textContent).toContain(copyText("usageHud.panel.from", { from: "核心·设置" }));
    expect(hud.summaryElement.getAttribute("aria-expanded")).toBe("true");
    hud.summaryElement.click();
    expect(panel()).toBeNull();
    expect(hud.summaryElement.getAttribute("aria-expanded")).toBe("false");
    hud.summaryElement.click();
    [...(panel()?.querySelectorAll("button") ?? [])].find((b) => b.textContent === copyText("usageHud.panel.fix"))?.click();
    expect([settings, panel()]).toEqual([1, null]);
  });

  it("上限判不出（核心没给百分比）时浮层第一行写「上限未知」，给［设上限］，不画进度条", () => {
    const hud = new UsageHud();
    document.body.appendChild(hud.summaryElement);
    hud.setActive(blind());
    hud.summaryElement.click();
    expect(panel()?.textContent).toContain(copyText("usageHud.panel.unknownLimit"));
    expect(panel()?.textContent).toContain(copyText("usageHud.panel.setLimit"));
    expect(panel()?.textContent).toContain(copyText("usageHud.panel.tokensOnly", { tok: "核心·350k", model: "claude-opus-5-5" }));
  });

  it("浮层开着时会话变成没有用量 ⇒ chip 不渲染、浮层一起关", () => {
    const hud = new UsageHud();
    document.body.appendChild(hud.summaryElement);
    hud.setActive(usage());
    hud.summaryElement.click();
    expect(panel()).not.toBeNull();
    hud.setActive(null);
    expect(panel()).toBeNull();
  });
});
