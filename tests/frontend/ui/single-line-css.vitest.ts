/**
 * 单行文字不压按钮（10-08 用户：会话名字太长时跟右边那几个按钮叠在一起）。
 *
 * 浏览器里量外接框的那一半住截图工具（`tests/shots/scenes/layout-check.ts`，每张图都量）；门禁里没有排版引擎，
 * 这里钉住让它成立的那几条样式：
 *
 * ① 登记的单行文字：不折行 · 超出省略 · 能缩到 0（否则把右边的按钮挤出去）。
 * ② 登记的右侧按钮组：不缩（宽度固定，永远不被压）。
 * ③ 标签页行尾按钮盖在标题尾巴上：底色每一层都垫着不透的 `--bg-2`（只写半透明的选中色 ⇒ 标题字透出来，就是那次撞见的样子）。
 */
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import postcss, { type Rule } from "postcss";

const UI = resolve(__dirname, "../../../src/frontend/ui");

/** 一份 CSS 里，选择器恰好是 `sel`（逗号分开的任一段）的规则里各声明的最后取值。 */
function declsOf(file: string, sel: string): Map<string, string> {
  const out = new Map<string, string>();
  postcss.parse(readFileSync(resolve(UI, file), "utf8")).walkRules((r: Rule) => {
    if (!r.selectors.map((x) => x.trim()).includes(sel)) return;
    r.walkDecls((d) => void out.set(d.prop, d.value));
  });
  return out;
}

/** ① 单行文字：文件 · 选择器 · 是什么。 */
const SINGLE_LINE: [string, string, string][] = [
  ["session-head.module.css", ".shTitle", "会话头标题"],
  ["session-head.module.css", ".shWhere", "会话头机器名"],
  ["session-head.module.css", ".shDir", "会话头目录"],
  ["session-head.module.css", ".shState", "会话头状态一句"],
  ["styles.css", ".tab-title", "标签页名字"],
  ["kit/fold.module.css", ".foldSummary", "折叠块标题行右侧那一句摘要"],
];

/** ② 右侧按钮组：文件 · 选择器 · 是什么。 */
const FIXED_BUTTONS: [string, string, string][] = [
  ["session-head.module.css", ".shExtra", "会话头［恢复 ▾］那一格"],
  ["session-head.module.css", ".shActs", "会话头右侧按钮"],
  ["styles.css", ".tab-trail", "标签页行尾徽标"],
  ["kit/fold.module.css", ".foldTitle", "折叠块标题（窄了让摘要先省略，标题不挤成竖排）"],
];

describe("单行文字不压按钮", () => {
  it.each(SINGLE_LINE)("① %s %s（%s）：不折行 · 省略号 · 能缩到 0", (file, sel) => {
    const d = declsOf(file, sel);
    expect([d.get("white-space"), d.get("overflow"), d.get("text-overflow"), d.get("min-width")]).toEqual(["nowrap", "hidden", "ellipsis", "0"]);
  });

  it.each(FIXED_BUTTONS)("② %s %s（%s）：不缩", (file, sel) => {
    const d = declsOf(file, sel);
    const flex = d.get("flex");
    expect(flex === "none" || d.get("flex-shrink") === "0" || /^\S+ 0( |$)/.test(flex ?? ""), `${sel} 的 flex = ${flex}`).toBe(true);
  });

  it("③ 标签页行尾按钮：每条给它上底色的规则，最底下那一层都是不透的 --bg-2", () => {
    const root = postcss.parse(readFileSync(resolve(UI, "styles.css"), "utf8"));
    const backgrounds: string[] = [];
    root.walkRules((r: Rule) => {
      if (!r.selectors.some((x) => /\.tab-acts$/.test(x.trim()))) return;
      r.walkDecls(/^background(-color)?$/, (d) => void backgrounds.push(`${r.selector} → ${d.value}`));
    });
    expect(backgrounds.length, "一条都没扫到 ⇒ 量具坏了").toBeGreaterThan(0);
    for (const b of backgrounds) expect(b, "最底层不是 --bg-2 ⇒ 半透明，标题字会透出来").toMatch(/(^|,)\s*var\(--bg-2\)\s*$/);
  });
});
