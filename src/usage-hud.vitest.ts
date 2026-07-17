// F88b（#52）用量 HUD chip 的 jsdom 测试：setActive 算 context% / 未知模型显 ?/
// 无 usage 隐藏 / ≥80% 高亮 / 点击回调。

import { describe, it, expect, vi } from "vitest";
import { UsageHud } from "./usage-hud";

describe("UsageHud (F88b #52)", () => {
  it("已知模型 → ctx N%，可见", () => {
    const hud = new UsageHud();
    // 100k / 200k = 50%
    hud.setActive("claude-opus-4-8", 100_000);
    expect(hud.summaryElement.textContent).toBe("ctx 50%");
    expect(hud.summaryElement.style.display).toBe("");
    expect(hud.summaryElement.classList.contains("is-high")).toBe(false);
  });

  it("[1m] 变体上限 1M → 正确 %", () => {
    const hud = new UsageHud();
    // 500k / 1M = 50%
    hud.setActive("claude-opus-4-8[1m]", 500_000);
    expect(hud.summaryElement.textContent).toBe("ctx 50%");
  });

  it("未知模型 → ctx ?（不显错 %），仍可见", () => {
    const hud = new UsageHud();
    hud.setActive("gpt-4", 100_000);
    expect(hud.summaryElement.textContent).toBe("ctx ?");
    expect(hud.summaryElement.style.display).toBe("");
    expect(hud.summaryElement.classList.contains("is-high")).toBe(false);
  });

  it("promptTokens=null（无带 usage 记录）→ 隐藏，且清 is-high", () => {
    const hud = new UsageHud();
    hud.setActive("claude-sonnet-5", 170_000); // 先 85% → is-high
    expect(hud.summaryElement.classList.contains("is-high")).toBe(true);
    hud.setActive(null, null); // 再切到无 usage 会话
    expect(hud.summaryElement.style.display).toBe("none");
    expect(hud.summaryElement.classList.contains("is-high")).toBe(false); // 隐藏时清干净
  });

  it("≥80% → is-high 高亮（逼近上限预警）", () => {
    const hud = new UsageHud();
    // 170k / 200k = 85%
    hud.setActive("claude-sonnet-5", 170_000);
    expect(hud.summaryElement.textContent).toBe("ctx 85%");
    expect(hud.summaryElement.classList.contains("is-high")).toBe(true);
  });

  it("model=null 但有 token → ctx ?（上限未知）", () => {
    const hud = new UsageHud();
    hud.setActive(null, 50_000);
    expect(hud.summaryElement.textContent).toBe("ctx ?");
  });

  it("onClick 注册的 handler 点击时触发", () => {
    const hud = new UsageHud();
    const spy = vi.fn();
    hud.onClick(spy);
    hud.summaryElement.click();
    expect(spy).toHaveBeenCalledOnce();
  });
});
