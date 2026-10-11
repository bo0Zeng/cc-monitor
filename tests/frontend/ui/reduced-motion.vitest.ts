/**
 * **减少动画**（系统的「减弱动态效果」· `prefers-reduced-motion: reduce`）开着时界面上什么都不动：运行中的点不呼吸、转圈不转、
 * 浮层直接出现、悬停直接变色、命中那一行不闪。
 *
 * 做法只有一种：动效的时长一律写 `--dur-*`（`styles/tokens.css`），减少动画时那几个令牌全是 0。
 * 判据钉两件：
 * - 界面的 CSS 里 `transition` · `animation`（连同 `-duration` 两格）不写字面时长（`120ms` · `1.5s`）—— 写了就绕过了那一处开关；
 * - `tokens.css` 里定义的每个 `--dur-*`，减少动画那一段都把它置 0（两向相等：新加一个令牌不置 0 红，置 0 了一个不存在的也红）。
 *
 * 人群：`src/frontend/ui/` 下全部 `.css`（新加一份也自动进来）。
 * 买不到：读的是 CSS 文本；TS 里直接改 `style.transition`、`element.animate()`、按帧手写的动画认不出（今天一处都没有）。
 */
import { readdirSync, readFileSync, statSync } from "node:fs";
import path from "node:path";
import { describe, it, expect } from "vitest";

const ui = path.resolve(__dirname, "../../../src/frontend/ui");

function walk(dir: string): string[] {
  const out: string[] = [];
  for (const name of readdirSync(dir)) {
    const p = path.join(dir, name);
    if (statSync(p).isDirectory()) {
      if (name === "generated" || name === "node_modules") continue;
      out.push(...walk(p));
    } else if (name.endsWith(".css")) out.push(p);
  }
  return out;
}

const stripComments = (s: string): string => s.replace(/\/\*[\s\S]*?\*\//g, "");

/** 一份 CSS 里写了字面时长的动效声明（`属性: 值`）。 */
function literalDurations(css: string): string[] {
  const out: string[] = [];
  for (const m of stripComments(css).matchAll(/(?:^|[;{\s])((?:transition|animation)(?:-duration)?)\s*:\s*([^;}]*)/g)) {
    const value = m[2].replace(/var\([^)]*\)/g, "");
    const times = [...value.matchAll(/(?<![\w.-])(\d*\.?\d+)(ms|s)\b/g)].filter((t) => Number(t[1]) !== 0);
    if (times.length > 0) out.push(`${m[1]}: ${m[2].replace(/\s+/g, " ").trim()}`);
  }
  return out;
}

/** `tokens.css`：（定义的 `--dur-*`，减少动画那一段里置 0 的 `--dur-*`）。 */
function durTokens(css: string): { defined: string[]; zeroed: string[]; notZero: string[] } {
  const src = stripComments(css);
  const at = src.indexOf("@media (prefers-reduced-motion: reduce)");
  expect(at, "tokens.css 里有减少动画那一段").toBeGreaterThan(0);
  const head = src.slice(0, at);
  const tail = src.slice(at, src.indexOf("}", src.indexOf("}", at) + 1) + 1);
  const defined = [...head.matchAll(/(--dur-[a-z0-9-]+)\s*:/g)].map((m) => m[1]);
  const reduced = [...tail.matchAll(/(--dur-[a-z0-9-]+)\s*:\s*([^;]+);/g)];
  return {
    defined: [...new Set(defined)].sort(),
    zeroed: reduced.map((m) => m[1]).sort(),
    notZero: reduced.filter((m) => !/^0(ms|s)?$/.test(m[2].trim())).map((m) => `${m[1]}: ${m[2].trim()}`),
  };
}

const files = walk(ui);

describe("减少动画时界面上什么都不动", () => {
  it("人群不空，量具认得出那几形", () => {
    expect(files.length).toBeGreaterThan(20);
    expect(literalDurations(".a { transition: opacity 0.12s ease; }")).toHaveLength(1);
    expect(literalDurations(".a { animation: spin 0.8s linear infinite }")).toHaveLength(1);
    expect(literalDurations(".a { animation-duration: 2.4s; }")).toHaveLength(1);
    expect(literalDurations(".a { transition: color var(--dur-press), background var(--dur-press); }")).toHaveLength(0);
    expect(literalDurations(".a { animation: none; transition: none; }")).toHaveLength(0);
    expect(literalDurations(".a { animation: x var(--dur-float-in) ease-out 0s; }")).toHaveLength(0);
    expect(literalDurations("/* transition: opacity 1s */ .a { color: red }")).toHaveLength(0);
  });

  it("动效的时长一律写 --dur-*（不写字面时长）", () => {
    const bad: string[] = [];
    for (const f of files) for (const d of literalDurations(readFileSync(f, "utf8"))) bad.push(`${path.relative(ui, f)}: ${d}`);
    expect(bad, "改写成 var(--dur-*)（tokens.css 里没有合适的就加一个，减少动画那一段同拍置 0）").toEqual([]);
  });

  it("每个 --dur-* 在减少动画那一段里都置 0（两向相等）", () => {
    const t = durTokens(readFileSync(path.join(ui, "styles/tokens.css"), "utf8"));
    expect(t.defined.length).toBeGreaterThan(3);
    expect(t.zeroed).toEqual(t.defined);
    expect(t.notZero).toEqual([]);
  });
});
