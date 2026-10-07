// 状态栏「上下文」chip 的 jsdom 测试：百分比按后端给的上限算、永远不超过 100 / 无 usage 不渲染 / ≥80% 琥珀 / 点开浮层与再点关。

import { describe, it, expect, beforeEach } from "vitest";
import { UsageHud } from "../../../src/frontend/ui/usage-hud";
import { closePopover } from "../../../src/frontend/ui/kit/popover";
import { copyText } from "../../../src/frontend/ui/copy-table";

const text = (hud: UsageHud): string => hud.summaryElement.textContent ?? "";
const intent = (hud: UsageHud): string | undefined => hud.summaryElement.dataset.intent;
const panel = (): HTMLElement | null => document.querySelector<HTMLElement>(`[role=dialog][aria-label='${copyText("usageHud.panel.label")}']`);

beforeEach(() => {
  closePopover();
  document.body.innerHTML = "";
});

describe("状态栏「上下文」chip", () => {
  it("按后端给的上限算 → 上下文 N%，可见", () => {
    const hud = new UsageHud();
    hud.setActive("claude-opus-5-5", 350_000, 1_000_000, "model");
    expect(text(hud)).toBe(copyText("usageHud.chip.pct", { n: "35" }));
    expect(hud.summaryElement.style.display).toBe("");
    expect(intent(hud)).toBe("neutral");
  });

  it("上限判不出：只写用了多少，不算百分比、不上色", () => {
    const hud = new UsageHud();
    hud.setActive("claude-opus-5-5", 350_000, null, "assumed");
    expect(text(hud)).toBe(copyText("usageHud.chip.tokens", { tokens: "350k" }));
    expect(intent(hud)).toBe("neutral");
    hud.setActive("m", 1_250_000, null, "assumed");
    expect(text(hud)).toBe(copyText("usageHud.chip.tokens", { tokens: "1.3M" }));
  });

  it("永远不显示超过 100%", () => {
    const hud = new UsageHud();
    hud.setActive("m", 250_000, 200_000, "observed");
    expect(text(hud)).toBe(copyText("usageHud.chip.pct", { n: "100" }));
  });

  it("≥80% ⇒ 琥珀；切到没有一轮回复的会话 ⇒ 不渲染、颜色清干净", () => {
    const hud = new UsageHud();
    hud.setActive("claude-haiku-4", 170_000, 200_000, "model");
    expect([text(hud), intent(hud)]).toEqual([copyText("usageHud.chip.pct", { n: "85" }), "warn"]);
    hud.setActive("claude-haiku-4", 158_000, 200_000, "model");
    expect(intent(hud)).toBe("neutral");
    hud.setActive("claude-sonnet-5", 170_000, 200_000, "model");
    hud.setActive(null, null, null);
    expect(hud.summaryElement.style.display).toBe("none");
    expect(intent(hud)).toBe("neutral");
  });

  it("事实要不到 ⇒ 上下文 —（压过旧的数）；浮层里错误条＋［重试］交宿主", () => {
    let retried = 0;
    const hud = new UsageHud();
    hud.host = { openSettings: () => {}, retry: () => retried++ };
    document.body.appendChild(hud.summaryElement);
    hud.setActive("m", 170_000, 200_000, "model");
    hud.setUnavailable(copyText("events.unseen.unreachableTitle", { machine: "devbox" }));
    expect([text(hud), intent(hud)]).toEqual([copyText("usageHud.chip.none"), "neutral"]);
    hud.summaryElement.click();
    expect(panel()?.textContent).toContain(copyText("usageHud.panel.unavailable"));
    [...(panel()?.querySelectorAll("button") ?? [])].find((b) => b.textContent === copyText("usageHud.panel.retry"))?.click();
    expect([retried, panel()]).toEqual([1, null]);
  });

  it("★ 点开浮层：百分比 · 最新一轮 / 上限 · 模型 · 上限来源；再点 chip 关；［改上限…］去设置", () => {
    let settings = 0;
    const hud = new UsageHud();
    hud.host = { openSettings: () => settings++, retry: () => {} };
    document.body.appendChild(hud.summaryElement);
    hud.setActive("claude-opus-5-5[1m]", 350_000, 1_000_000, "setting");
    hud.summaryElement.click();
    const p = panel();
    expect(p?.textContent).toContain(copyText("usageHud.chip.pct", { n: "35" }));
    expect(p?.textContent).toContain(copyText("usageHud.panel.usage", { tok: "350k", limit: "1M", model: "claude-opus-5-5" }));
    expect(p?.textContent).toContain(copyText("usageHud.panel.from", { from: copyText("usageHud.from.setting") }));
    expect(hud.summaryElement.getAttribute("aria-expanded")).toBe("true");
    hud.summaryElement.click();
    expect(panel()).toBeNull();
    expect(hud.summaryElement.getAttribute("aria-expanded")).toBe("false");
    hud.summaryElement.click();
    [...(panel()?.querySelectorAll("button") ?? [])].find((b) => b.textContent === copyText("usageHud.panel.fix"))?.click();
    expect([settings, panel()]).toEqual([1, null]);
  });

  it("上限判不出时浮层第一行写「上限未知」，给［设上限］，不画进度条", () => {
    const hud = new UsageHud();
    document.body.appendChild(hud.summaryElement);
    hud.setActive("m", 350_000, null, "assumed");
    hud.summaryElement.click();
    expect(panel()?.textContent).toContain(copyText("usageHud.panel.unknownLimit"));
    expect(panel()?.textContent).toContain(copyText("usageHud.panel.setLimit"));
    expect(panel()?.textContent).not.toContain("%");
  });

  it("浮层开着时会话变成没有用量 ⇒ chip 不渲染、浮层一起关", () => {
    const hud = new UsageHud();
    document.body.appendChild(hud.summaryElement);
    hud.setActive("m", 10_000, 200_000, "model");
    hud.summaryElement.click();
    expect(panel()).not.toBeNull();
    hud.setActive(null, null, null);
    expect(panel()).toBeNull();
  });
});
