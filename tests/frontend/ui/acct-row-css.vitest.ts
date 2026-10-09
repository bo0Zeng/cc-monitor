/**
 * 账号面板轮换列表一行的样式（10-08 主会话审截图：兜底开 / 关一眼看不出；personal 行「在用」被截成「在…」、重置时刻折成两行）。
 *
 * 浏览器里量外接框的那一半住截图工具（`tests/shots/scenes/layout-check.ts`：标签全字可见 · 时刻一行 · 兜底开着看得见、关着不悬停看不见）；
 * 门禁里没有排版引擎，这里钉住让它成立的那几条样式：
 *
 * ① 兜底开关：关 ⇒ 透明、0 宽（仍可聚焦）；所在行悬停或行内有焦点 ⇒ 出来；开 ⇒ 实心底（与只读标同一个样子）。
 * ② 一行挤了先截号名：号名不折行 · 省略 · 能缩到 0；状态标签与行尾用量不缩、不折行。
 */
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import postcss, { type Rule } from "postcss";

const UI = resolve(__dirname, "../../../src/frontend/ui");

/** 一份 CSS 里，选择器（逗号分开的任一段）恰好是 `sel` 的规则里各声明的最后取值。 */
function declsOf(file: string, sel: string): Map<string, string> {
  const out = new Map<string, string>();
  postcss.parse(readFileSync(resolve(UI, file), "utf8")).walkRules((r: Rule) => {
    if (!r.selectors.map((x) => x.trim()).includes(sel)) return;
    r.walkDecls((d) => void out.set(d.prop, d.value));
  });
  return out;
}

const noShrink = (d: Map<string, string>): boolean => d.get("flex") === "none" || d.get("flex-shrink") === "0";

describe("账号面板轮换列表一行", () => {
  it("① 兜底关着：透明、收成 0 宽不占地方（不是 display:none / visibility:hidden —— 那样键盘到不了）", () => {
    const d = declsOf("rot-editor.module.css", 'button.rotFallback[aria-pressed="false"]');
    expect([d.get("opacity"), d.get("max-width")]).toEqual(["0", "0"]);
    expect(d.has("display") || d.has("visibility")).toBe(false);
  });

  it("① 兜底关着：所在行悬停或行内有焦点 ⇒ 出来", () => {
    for (const sel of [':hover > button.rotFallback[aria-pressed="false"]', ':focus-within > button.rotFallback[aria-pressed="false"]']) {
      const d = declsOf("rot-editor.module.css", sel);
      expect([d.get("opacity"), d.get("max-width")], sel).toEqual(["1", "none"]);
    }
  });

  it("① 兜底开着 ＝ 实心底，只读标同一条规则", () => {
    for (const sel of ['button.rotFallback[aria-pressed="true"]', ".rotFallbackMark"]) {
      const bg = declsOf("rot-editor.module.css", sel).get("background");
      expect(bg && bg !== "transparent" && bg !== "none", `${sel} 的底色 = ${bg}`).toBe(true);
    }
  });

  it("② 号名：不折行 · 省略号 · 能缩到 0（挤了先截它）", () => {
    const d = declsOf("acct.module.css", ".acctRowName");
    expect([d.get("white-space"), d.get("overflow"), d.get("text-overflow"), d.get("min-width")]).toEqual(["nowrap", "hidden", "ellipsis", "0"]);
  });

  it("② 行里的状态标签（在用 · 起始 · 按量 …）不缩", () => {
    expect(noShrink(declsOf("acct.module.css", ".acctRow > .acctTag"))).toBe(true);
  });

  it("② 行尾用量不缩、时刻不折行", () => {
    const d = declsOf("acct.module.css", ".acctRowUsage");
    expect(noShrink(d), `flex = ${d.get("flex")}`).toBe(true);
    expect(d.get("white-space")).toBe("nowrap");
  });
});
