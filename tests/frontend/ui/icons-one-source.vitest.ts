/**
 * 图标只有一个出处：`kit/icon.ts` 的 Phosphor 登记表（规范 V9；文案新写法 N1a「图形图标由代码画，表里只留读屏名」）。
 *
 * - 生产 TS 里建 `<svg>` 的只有 `kit/icon.ts`（别处不许手写 SVG）。
 * - 生产 CSS 的伪元素不画字符：`content:` 只许空串 / `none` / 分隔点 `var(--mark-sep)`（分隔点是句中符号，在符号表里）。
 * - CSS 里要画图标（`<details>` 摘要的箭头那一类伪元素）只走 `mask: var(--icon-…)`；`--icon-…` 只由 `css-marks.ts` 从登记表取（`iconMaskUrl`），CSS 里不写 svg 地址。
 * - 产品里的 `<details>` 都不出浏览器自带三角：三扇窗都加载的 `reset.css` 里一条全局规则关掉，
 *   箭头只在一处画（`summary::before` 那颗 caretRight，展开转 90°，同 kit 折叠块）；别处不许另画、也不许各自再关一遍。
 * - 能展开的行（折叠块 · 过程行 · 消息记录 · 维护项 · 历史页项目头）的折叠号都是 `kit/fold.ts` 的 `foldCaret()`：同一颗 caretRight，
 *   所在元素 `aria-expanded="true"` 时转 90°；不按开合在 caretDown / caretRight 两张图之间换。
 */
import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { productionCssFiles, productionTsFiles } from "../../test-support/production-sources.ts";
import { REPO_ROOT } from "../../test-support/repo-root.ts";

const css = productionCssFiles("src/frontend");
const ts = productionTsFiles("src/frontend");

describe("图标只有一个出处", () => {
  it("量具自检：扫到了样式与源码", () => {
    expect(css.length).toBeGreaterThan(20);
    expect(ts.length).toBeGreaterThan(100);
  });

  it("只有 kit/icon.ts 建 svg", () => {
    const bad = ts
      .filter((f) => !f.file.endsWith("kit/icon.ts"))
      .filter((f) => /createElementNS\(|<svg[\s>]|www\.w3\.org\/2000\/svg/.test(f.text))
      .map((f) => f.file);
    expect(bad).toEqual([]);
  });

  it("CSS 伪元素的 content 不画字符", () => {
    const allowed = new Set(['""', "none", "var(--mark-sep)"]);
    const bad: string[] = [];
    for (const f of css) for (const m of f.text.matchAll(/(?:^|[;{\s])content:\s*([^;}]+)/g)) if (!allowed.has(m[1].trim())) bad.push(`${f.file}: ${m[1].trim()}`);
    expect(bad).toEqual([]);
  });

  it("CSS 画图标只走 mask: var(--icon-…)，不写 svg 地址", () => {
    const bad: string[] = [];
    for (const f of css) {
      if (/data:image\/svg/.test(f.text)) bad.push(`${f.file}: data:image/svg`);
      for (const m of f.text.matchAll(/(?:^|[;{\s])(?:-webkit-)?mask(?:-image)?:\s*([^;}]+)/g))
        if (!/^var\(--icon-[a-z-]+\)/.test(m[1].trim())) bad.push(`${f.file}: ${m[1].trim()}`);
    }
    expect(bad).toEqual([]);
  });

  it("<details> 不出自带三角：reset.css 全局关掉；摘要箭头全产品只画一处", () => {
    const reset = css.find((f) => f.file.endsWith("styles/reset.css"))?.text ?? "";
    expect(reset, "reset.css 里没有关掉 summary 的自带三角").toMatch(/(?:^|[\s,}])summary\s*\{[^}]*list-style:\s*none/);
    expect(reset).toMatch(/summary::-webkit-details-marker\s*\{[^}]*display:\s*none/);
    // 三扇窗都加载 reset.css（各窗口 html 的样式表清单）。
    for (const html of ["index.html", "settings.html", "viewer.html"]) expect(readFileSync(resolve(REPO_ROOT, html), "utf8"), html).toContain("styles/reset.css");
    const elsewhere = css.filter((f) => !f.file.endsWith("styles/reset.css") && /details-marker|summary[^{]*\{[^}]*list-style/.test(f.text)).map((f) => f.file);
    expect(elsewhere, "别处又各自关了一遍自带三角（只许 reset.css 一处）").toEqual([]);
    const carets = css.flatMap((f) => [...f.text.matchAll(/mask:\s*var\(--icon-caret-right\)/g)].map(() => f.file));
    expect(carets, "摘要箭头画了不止一处（或一处都没有）").toHaveLength(1);
    const caretRule = css.flatMap((f) => [...f.text.matchAll(/([^{}]+)\{[^}]*mask:\s*var\(--icon-caret-right\)/g)].map((m) => m[1].trim()));
    expect(caretRule[0], "箭头那一条得是全局的 summary::before（只许排除几类标题行）").toMatch(/^summary(?::not\([^)]*\))?::before$/);
  });

  it("折叠号只有 foldCaret 一颗：不按开合换图", () => {
    const fold = ts.find((f) => f.file.endsWith("kit/fold.ts"))?.text ?? "";
    expect(fold, "kit/fold.ts 里没有 foldCaret").toMatch(/export function foldCaret\(/);
    const swap = /\?\s*["']caret(?:Down|Right)["']\s*:\s*["']caret(?:Down|Right)["']/;
    expect(swap.test('open ? "caretDown" : "caretRight"'), "量具：换图写法认得出").toBe(true);
    const bad = ts.filter((f) => swap.test(f.text)).map((f) => f.file);
    expect(bad, "这几份按开合换折叠号的图（改用 foldCaret）").toEqual([]);
    const users = ts.filter((f) => /\bfoldCaret\(\)/.test(f.text)).map((f) => f.file.replace(/^.*src\/frontend\/ui\//, "")).sort();
    expect(users).toEqual(["kit/fold.ts", "settings/chore-row.ts", "settings/rule-editor.ts", "settings/rules-section.ts", "status-messages.ts", "turn-fold.ts", "views/history-rows.ts"]);
  });

  it("--icon-… 只由 css-marks.ts 经登记表设", () => {
    const setters = ts.filter((f) => /setProperty\(\s*["'`]--icon-/.test(f.text)).map((f) => f.file);
    expect(setters.every((f) => f.endsWith("css-marks.ts")), setters.join(", ")).toBe(true);
    const marks = ts.find((f) => f.file.endsWith("css-marks.ts"));
    expect(marks?.text).toMatch(/iconMaskUrl\(/);
    for (const f of css) for (const m of f.text.matchAll(/--icon-([a-z-]+)\s*:/g)) expect(["size", "size-compact", "size-empty"], `${f.file} 自己写了 --icon-${m[1]}`).toContain(m[1]);
  });
});
