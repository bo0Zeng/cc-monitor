// 用量 HUD chip 的 jsdom 测试：百分比按后端给的上限算、永远不超过 100 / 无 usage 隐藏 / ≥80% 高亮 / 纯只读。

import { describe, it, expect } from "vitest";
import { UsageHud } from "../../../src/frontend/ui/usage-hud";
// 高位预警的类名来自组件自己的 CSS Module（构建时哈希）—— 断言也经同一个导入取名，不写字面量。
import s from "../../../src/frontend/ui/usage-hud.module.css";

describe("UsageHud", () => {
  it("按后端给的上限算 → ctx N%，可见", () => {
    const hud = new UsageHud();
    hud.setActive("claude-opus-5-5", 350_000, 1_000_000);
    expect(hud.summaryElement.textContent).toBe("ctx 35%");
    expect(hud.summaryElement.style.display).toBe("");
    expect(hud.summaryElement.classList.contains(s.high)).toBe(false);
  });

  it("模型名不带 [1m] 的 1M 会话不再按 200k 算：用的是后端的上限，不是模型名", () => {
    const hud = new UsageHud();
    hud.setActive("claude-opus-5-5", 350_000, 1_000_000);
    expect(hud.summaryElement.textContent).not.toBe("ctx 175%");
  });

  it("上限判不出（后端说 assumed ⇒ 没有上限）：只写用了多少，不算百分比、不预警", () => {
    const hud = new UsageHud();
    hud.setActive("claude-opus-5-5", 350_000, null);
    expect(hud.summaryElement.textContent).toBe("ctx 350k");
    expect(hud.summaryElement.style.display).toBe("");
    expect(hud.summaryElement.classList.contains(s.high)).toBe(false);
    hud.setActive("m", 1_250_000, null);
    expect(hud.summaryElement.textContent).toBe("ctx 1.3M");
  });

  it("永远不显示超过 100%", () => {
    const hud = new UsageHud();
    hud.setActive("m", 250_000, 200_000);
    expect(hud.summaryElement.textContent).toBe("ctx 100%");
  });

  it("promptTokens=null（无带 usage 记录）→ 隐藏，且清 is-high", () => {
    const hud = new UsageHud();
    hud.setActive("claude-sonnet-5", 170_000, 200_000); // 先 85% → is-high
    expect(hud.summaryElement.classList.contains(s.high)).toBe(true);
    hud.setActive(null, null, null); // 再切到无 usage 会话
    expect(hud.summaryElement.style.display).toBe("none");
    expect(hud.summaryElement.classList.contains(s.high)).toBe(false); // 隐藏时清干净
  });

  it("≥80% → is-high 高亮（逼近上限预警）", () => {
    const hud = new UsageHud();
    hud.setActive("claude-haiku-4", 170_000, 200_000);
    expect(hud.summaryElement.textContent).toBe("ctx 85%");
    expect(hud.summaryElement.classList.contains(s.high)).toBe(true);
  });

  // chip 点下去打开的那个跨会话聚合视图整轴退役了：钉住 chip 今天是**纯只读**的。
  it("chip 是纯只读：没有挂任何点击监听，也不长成可点的样子", () => {
    const hud = new UsageHud();
    hud.setActive("claude-opus-4-8", 10_000, 1_000_000);
    expect(hud.summaryElement.style.cursor).toBe("default");
    expect((hud as unknown as { onClick?: unknown }).onClick).toBeUndefined();
    const before = hud.summaryElement.textContent;
    hud.summaryElement.click();
    expect(hud.summaryElement.textContent).toBe(before);
  });
});
