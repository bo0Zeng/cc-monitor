/**
 * 图标只有一个出处：`kit/icon.ts` 的 Phosphor 登记表（规范 V9；文案新写法 N1a「图形图标由代码画，表里只留读屏名」）。
 *
 * - 生产 TS 里建 `<svg>` 的只有 `kit/icon.ts`（别处不许手写 SVG）。
 * - 生产 CSS 的伪元素不画字符：`content:` 只许空串 / `none` / 分隔点 `var(--mark-sep)`（分隔点是句中符号，在符号表里）。
 * - CSS 里要画图标（`<details>` 摘要的箭头那一类伪元素）只走 `mask: var(--icon-…)`；`--icon-…` 只由 `css-marks.ts` 从登记表取（`iconMaskUrl`），CSS 里不写 svg 地址。
 */
import { describe, expect, it } from "vitest";
import { productionCssFiles, productionTsFiles } from "../../test-support/production-sources.ts";

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

  it("--icon-… 只由 css-marks.ts 经登记表设", () => {
    const setters = ts.filter((f) => /setProperty\(\s*["'`]--icon-/.test(f.text)).map((f) => f.file);
    expect(setters.every((f) => f.endsWith("css-marks.ts")), setters.join(", ")).toBe(true);
    const marks = ts.find((f) => f.file.endsWith("css-marks.ts"));
    expect(marks?.text).toMatch(/iconMaskUrl\(/);
    for (const f of css) for (const m of f.text.matchAll(/--icon-([a-z-]+)\s*:/g)) expect(["size", "size-compact", "size-empty"], `${f.file} 自己写了 --icon-${m[1]}`).toContain(m[1]);
  });
});
