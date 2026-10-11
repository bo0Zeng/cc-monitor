/**
 * Batch13-F40b(D 审计发现 1):removeEmptyStepsCard 单测。
 * fallback 孤儿单元是一张「几步」卡里的一行(timeline entry = 那张卡)——单元被摘后卡若空,
 * 必须连卡摘掉并交回供出账;卡里还有其他单元则不动。
 */
import { describe, expect, it } from "vitest";
import { removeEmptyStepsCard } from "../../../src/frontend/ui/cards";

function mkCard(unitCount: number): HTMLElement {
  const card = document.createElement("div");
  card.className = "card card-steps";
  for (let i = 0; i < unitCount; i++) card.appendChild(document.createElement("div"));
  document.body.appendChild(card);
  return card;
}

describe("removeEmptyStepsCard", () => {
  it("卡已空 → 摘掉并交回(供 timeline.removeByElement 出账)", () => {
    const card = mkCard(1);
    card.firstElementChild!.remove(); // 模拟 reconcile 摘走唯一 fallback 单元
    expect(removeEmptyStepsCard(card)).toBe(card);
    expect(card.isConnected).toBe(false);
  });

  it("卡里还有其他单元 → 不摘,返回 null", () => {
    const card = mkCard(2);
    card.firstElementChild!.remove();
    expect(removeEmptyStepsCard(card)).toBeNull();
    expect(card.isConnected).toBe(true);
  });

  it("host 为 null(单元不在任何卡内,防御)→ null", () => {
    expect(removeEmptyStepsCard(null)).toBeNull();
  });
});
