/**
 * 一个组件一个家：对话框 · 弹出菜单 · 悬停提示 · toast 栈 · 点外面关 · 按钮 · `el()` 只在 `src/frontend/ui/kit/` 里建。
 *
 * TS 那一半由 eslint 守（`eslint.config.js` 的 `singleDoors`，家是 `kit/`；`single-home-lint.vitest.ts` 证会抓）。
 * eslint 不读 CSS ⇒ 本文件只留 CSS 那一格：设置页那套按钮类在样式表里也零命中（注释不算）。
 */
import { describe, it, expect } from "vitest";
import { productionCssFiles } from "../../../test-support/production-sources.ts";
import { stripComments } from "../../../test-support/strip-comments.ts";

/** 设置页不许再有的那套按钮类（拼出来写，免得本文件自己成了全仓 grep 的一处命中）。 */
const OLD_BTN = ["settings", "btn"].join("-");

/** CSS 里只认选择器里写的那个类（注释去掉）。 */
function cssHitsOf(file: string, text: string): string[] {
  const code = stripComments(text, "css");
  return new RegExp(`\\.${OLD_BTN}\\b`).test(code) ? [`${file}：设置页按钮类`] : [];
}

describe("按钮只经 kit/button（CSS 那一格）", () => {
  it("样式表里零命中", () => {
    expect(productionCssFiles("src").flatMap((f) => cssHitsOf(f.file, f.text))).toEqual([]);
  });

  it("🔴 正控：选择器里的认得出，注释里写的不算", () => {
    expect(cssHitsOf("x.css", `.${OLD_BTN}:hover { color: red; }`)).toEqual(["x.css：设置页按钮类"]);
    expect(cssHitsOf("x.css", `/* .${OLD_BTN} */ .a { color: red; }`)).toEqual([]);
  });
});
