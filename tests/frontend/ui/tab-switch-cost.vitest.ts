/**
 * 切 tab 的代价：切换路上不许出现「随 tab 大小 / tab 个数涨」的那几样。
 *
 * 读数与量法住性能台架（`tests/shots/perf/bench.mjs` · `webkit.py`）：一屋子 24 个 tab、四条几千条记录的长会话，
 * 真浏览器里量「按下到画出」· 同步段 · 长任务 · 帧间隔。这里钉的是读数背后的**性质**（jsdom 没有排版引擎，毫秒量不出来）：
 *
 * ① 切走的 tab 收起用 `content-visibility: hidden`，切换只翻**不继承**的属性。
 *    以前翻的是 `visibility` ＋ `pointer-events`（都是继承属性）：WebKitGTK 每切一下要把新旧两个 tab 的整棵子树样式重算一遍
 *    （长会话几万个节点，台架读数热切同步段 p50 68 ms）；而且收起的 tab 照样留在渲染树里 ——
 *    每一帧的可见性观察（`content-visibility: auto` 的卡）· 命中测试 · 合成都按**所有 tab 的节点总数**算（Chromium 轨迹里每帧 4–15 ms 的交叉计算）。
 *    `content-visibility: hidden` 让浏览器跳过收起那棵子树的样式 / 布局 / 绘制 / 命中，切回来沿用收起前的渲染状态。
 */
import { readFileSync } from "node:fs";
import path from "node:path";
import { describe, it, expect } from "vitest";

const repo = path.resolve(__dirname, "../../..");
const css = readFileSync(path.join(repo, "src/frontend/ui/styles.css"), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");

/** 选择器恰好是 `sel` 的那几条规则的声明（同一选择器写了几处就合起来，后写的盖先写的）。 */
function decls(sel: string): Map<string, string> {
  const out = new Map<string, string>();
  for (const m of css.matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
    const sels = m[1].split(",").map((s) => s.trim());
    if (!sels.includes(sel)) continue;
    for (const d of m[2].split(";")) {
      const i = d.indexOf(":");
      if (i < 0) continue;
      out.set(d.slice(0, i).trim(), d.slice(i + 1).trim());
    }
  }
  return out;
}

/** 继承属性（CSS 规范里 Inherited: yes 的那几族；翻它 ⇒ 不认独立继承快路的引擎要重算整棵子树）。 */
const INHERITED = /^(visibility|pointer-events|color|cursor|font(-.*)?|line-height|letter-spacing|word-spacing|white-space|text-(align|indent|transform|shadow|rendering)|direction|writing-mode|quotes|list-style(-.*)?|tab-size|caret-color|hyphens|overflow-wrap|word-break|-webkit-text-.*)$/;

describe("① 切 tab 只翻不继承的属性；收起的 tab 用 content-visibility: hidden", () => {
  it("收起的 tab：content-visibility: hidden；当前的：visible", () => {
    expect(decls(".stream").get("content-visibility"), "收起的 tab 留在渲染树里 ⇒ 每一帧的代价按所有 tab 的节点总数算").toBe("hidden");
    expect(decls(".stream.active").get("content-visibility")).toBe("visible");
  });

  it("切换（`.stream` → `.stream.active`）翻到的属性里一个继承属性都没有", () => {
    const base = decls(".stream");
    const on = decls(".stream.active");
    const flipped = [...on.keys()].filter((k) => base.get(k) !== on.get(k));
    expect(flipped.length, "前提：确实读到了切换那一对规则").toBeGreaterThan(0);
    const bad = flipped.filter((k) => INHERITED.test(k));
    expect(bad, "翻继承属性 ⇒ 新旧两个 tab 的整棵子树样式重算（WebKitGTK 没有独立继承属性的快路）").toEqual([]);
  });

  it("单条流的窗口（查看器 · agent 窗口，复用 `.stream`、没有 `.active`）显式翻出来，不然整页空白", () => {
    expect(decls(".session-viewer-stream").get("content-visibility")).toBe("visible");
  });
});
