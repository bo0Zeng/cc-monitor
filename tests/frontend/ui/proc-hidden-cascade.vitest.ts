// 过程折着时那几张卡真的不见：`turn-fold.ts` 给卡挂 `proc-hidden`（折着的过程）/ `queued-landed`（落成了人话的排队插话），
// 藏不藏只靠这两个类的 `display: none`。卡自己的样式（如 `.card-steps { display: flex }`）写在它后面、同一权重时会把它盖掉 ⇒
// 过程行显示折着、底下的卡照样露着。这里把真样式表交给 jsdom，对 cards/ 里每一种卡各造一张、挂上类再量。
import { describe, it, expect, beforeAll } from "vitest";
import { readFileSync, readdirSync } from "node:fs";
import { resolve } from "node:path";
import { REPO_ROOT } from "../../test-support/repo-root";

const cardsDir = resolve(REPO_ROOT, "src/frontend/ui/cards");
/** cards/ 里每一种卡的根类（`className = "card card-…"` 那一串）。 */
const cardClasses = [
  ...new Set(
    readdirSync(cardsDir)
      .filter((f) => f.endsWith(".ts"))
      .flatMap((f) => [...readFileSync(resolve(cardsDir, f), "utf8").matchAll(/className = "(card card-[^"]+)"/g)].map((m) => m[1])),
  ),
].sort();

/** jsdom 算样式时不进 `@layer` 块：样式表整份住在一层里，摊平成顶层规则（同一层内先后与权重不变）再交给它。 */
function flatten(rules: CSSRuleList): string[] {
  return [...rules].flatMap((r) => (r.constructor.name === "CSSLayerBlockRule" ? flatten((r as CSSGroupingRule).cssRules) : [r.cssText]));
}

beforeAll(() => {
  const raw = document.createElement("style");
  raw.textContent = readFileSync(resolve(REPO_ROOT, "src/frontend/ui/styles.css"), "utf8");
  document.head.appendChild(raw);
  const flat = flatten((raw.sheet as CSSStyleSheet).cssRules);
  raw.remove();
  const style = document.createElement("style");
  style.textContent = flat.join("\n");
  document.head.appendChild(style);
  expect(flat.some((t) => t.includes(".proc-hidden"))).toBe(true);
});

function displayOf(classes: string): string {
  const content = document.createElement("div");
  content.className = "stream-content";
  const card = document.createElement("div");
  card.className = classes;
  content.appendChild(card);
  document.body.appendChild(content);
  const d = getComputedStyle(card).display;
  content.remove();
  return d;
}

describe("折着的卡真的藏起来", () => {
  it("cards/ 里认得出那几种卡（含过程里的几步）", () => {
    expect(cardClasses).toContain("card card-steps");
    expect(cardClasses.length).toBeGreaterThan(10);
  });

  it.each(cardClasses)("%s ＋ proc-hidden ⇒ 不显示", (c) => {
    expect(displayOf(`${c} proc-hidden`)).toBe("none");
  });

  it.each(cardClasses)("%s ＋ queued-landed ⇒ 不显示", (c) => {
    expect(displayOf(`${c} queued-landed`)).toBe("none");
  });
});
