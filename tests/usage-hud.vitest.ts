// F88b（#52）用量 HUD chip 的 jsdom 测试：setActive 算 context% / 未知模型显 ?/
// 无 usage 隐藏 / ≥80% 高亮 / 点击回调。

import { describe, it, expect } from "vitest";
import { UsageHud } from "../src/usage-hud";

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

  // 〔`设计/50` 删用量〕原先这里是「`onClick` 注册的 handler 点击时触发」——
  // chip 点下去打开的那个跨会话聚合视图（`views/usage-view.ts`）整轴退役了，
  // `onClick` 随之从 `UsageHud` 上删掉。**这一条翻面**：钉住 chip 今天是**纯只读**的。
  // ⚠ 翻面不是放宽：它挡的是「有人顺手把点击行为加回来却没有对面」。
  it("chip 是纯只读：没有挂任何点击监听，也不长成可点的样子", () => {
    const hud = new UsageHud();
    hud.setActive("claude-opus-4-8", 10_000);
    expect(hud.summaryElement.style.cursor).toBe("default");
    expect((hud as unknown as { onClick?: unknown }).onClick).toBeUndefined();
    // 点它不许抛，也不许有任何副作用可观察 —— 文本在点击前后逐字不变。
    const before = hud.summaryElement.textContent;
    hud.summaryElement.click();
    expect(hud.summaryElement.textContent).toBe(before);
  });
});
