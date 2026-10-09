/**
 * 设计令牌两条：对比度实算 · 组件 CSS 里不写令牌该管的字面量。
 *
 * - 对比度：从 `tokens.css` 现读色值，按 WCAG 相对亮度算；表与组件规范里那张对比度表逐格相等（取一位小数），
 *   外加几条底线（要读的字 ≥4.5:1、实心按钮上的白字 ≥4.5:1）。
 * - 字面量：令牌文件之外的 CSS 里，颜色（`#…` / `rgb()`）· 字号（`px`）· 圆角（`px`）· 投影里的颜色
 *   一律走令牌；例外逐条登记。
 */
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { cssDeclarations, type CssDecl } from "../../evidence/S30-css-conventions.ts";
import { REPO_ROOT } from "../../test-support/repo-root.ts";

const TOKENS = readFileSync(resolve(REPO_ROOT, "src/frontend/ui/styles/tokens.css"), "utf8");

/** 令牌的第一处声明值（`:root` 里那一条；减弱动效那段在后面，不取）。 */
function token(name: string, src = TOKENS): string {
  const m = new RegExp(`^\\s*${name}:\\s*([^;]+);`, "m").exec(src);
  if (!m) throw new Error(`tokens.css 里没有 ${name}`);
  return m[1].trim();
}

function hex(v: string): [number, number, number] {
  const m = /^#([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(v);
  if (!m) throw new Error(`不是不透明的 #色：${v}`);
  const h = m[1].length === 3 ? [...m[1]].map((c) => c + c).join("") : m[1];
  return [0, 2, 4].map((i) => parseInt(h.slice(i, i + 2), 16)) as [number, number, number];
}

function luminance(rgb: [number, number, number]): number {
  const [r, g, b] = rgb.map((c) => {
    const s = c / 255;
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

export function contrast(fg: string, bg: string): number {
  const a = luminance(hex(fg));
  const b = luminance(hex(bg));
  return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
}

const ratio = (fg: string, bg: string): number => contrast(token(fg), token(bg));

/** 组件规范里那张对比度表：前景 × 三层底（骨架 · 内容 · 凸起）。 */
const CONTRAST: Record<string, [number, number, number]> = {
  "--text": [13.4, 11.5, 9.3],
  "--text-2": [6.6, 5.7, 4.6],
  "--text-faint": [2.9, 2.5, 2.0],
  "--accent": [4.3, 3.7, 3.0],
  "--warn": [7.1, 6.1, 4.9],
  "--error": [4.9, 4.2, 3.4],
  "--error-text": [6.9, 6.0, 4.8],
  "--color-link": [7.8, 6.7, 5.4],
};
const SURFACES = ["--bg-2", "--bg", "--card"] as const;

describe("对比度实算", () => {
  it("前景 × 三层底逐格等于规范那张表（表里是一位小数，差不过 0.05）", () => {
    const off = Object.entries(CONTRAST).flatMap(([fg, want]) =>
      SURFACES.map((bg, i) => ({ fg, bg, got: ratio(fg, bg), want: want[i] })).filter((c) => Math.abs(c.got - c.want) > 0.051),
    );
    expect(off).toEqual([]);
  });

  it("要读的字在三层底上都 ≥4.5:1；淡字与赤陶橙当字不够（所以不许写要读的字）", () => {
    for (const fg of ["--text", "--text-2", "--warn", "--error-text", "--color-link"]) {
      for (const bg of SURFACES) expect(ratio(fg, bg), `${fg} on ${bg}`).toBeGreaterThanOrEqual(4.5);
    }
    expect(ratio("--text-faint", "--bg")).toBeLessThan(4.5);
    expect(ratio("--accent", "--card")).toBeLessThan(4.5);
  });

  it("实心按钮：白字在主按钮底 / 危险按钮底上 ≥4.5:1，在强调色原色上不到（所以主按钮另用一档）", () => {
    expect(contrast(token("--text-on-strong").replace(/^#fff$/i, "#ffffff"), token("--accent-strong"))).toBeGreaterThanOrEqual(4.5);
    expect(contrast("#ffffff", token("--error-strong"))).toBeGreaterThanOrEqual(4.5);
    expect(contrast("#ffffff", token("--accent"))).toBeLessThan(4.5);
  });

  it("🔴 正控：量具认得出一对不够的（把淡字当正文）与一对够的", () => {
    expect(contrast("#6b665e", "#2b2a27")).toBeLessThan(3);
    expect(contrast("#e8e6e1", "#2b2a27")).toBeGreaterThan(11);
  });
});

// ───────────────────────── 字面量 ─────────────────────────

const DECLS: CssDecl[] = cssDeclarations(REPO_ROOT).filter((d) => !d.file.endsWith("/styles/tokens.css"));

/** 令牌之外许写字面量的，逐条（文件 · 选择器 · 属性）。 */
const ALLOWED: Readonly<Record<string, string>> = {
  "src/frontend/ui/styles/shared.css|.acct-avatar|font-size": "16px 方块装不下 11px 的缩写字母",
  "src/frontend/ui/styles/shared.css|.acct-avatar|box-shadow": "头像块的内描边：随账号色叠黑白两层，不是浮层投影",
};

const COLOR = /#[0-9a-f]{3,8}\b|\brgba?\(/i;
const PX = /^-?\d+(\.\d+)?px$/;

function literalOf(d: CssDecl): string | null {
  const v = d.value.replace(/var\([^()]*\)/g, "");
  if (COLOR.test(v)) return "颜色";
  if (d.prop === "font-size" && PX.test(d.value.trim())) return "字号";
  if (d.prop === "border-radius" && d.value.split(/\s+/).some((p) => PX.test(p))) return "圆角";
  return null;
}

export function literalHits(decls: CssDecl[]): string[] {
  const out: string[] = [];
  for (const d of decls) {
    const kind = literalOf(d);
    if (kind === null) continue;
    const key = `${d.file}|${d.sels.join(",")}|${d.prop}`;
    if (key in ALLOWED) continue;
    out.push(`${key}  ${kind}：${d.value}`);
  }
  return out;
}

describe("令牌之外不写字面量（V1 · V4 · V6 · V7）", () => {
  it("分母：判过的声明条数", () => {
    expect(DECLS.length).toBeGreaterThan(2000);
  });

  it("颜色 · 字号 · 圆角字面量 == 登记的例外（今天两条）", () => {
    expect(literalHits(DECLS)).toEqual([]);
  });

  it("登记的例外今天真的还在", () => {
    const live = new Set(DECLS.map((d) => `${d.file}|${d.sels.join(",")}|${d.prop}`));
    expect(Object.keys(ALLOWED).filter((k) => !live.has(k))).toEqual([]);
  });

  it("🔴 正控：合成的四种字面量各认得出，走令牌的不报", () => {
    const at = (prop: string, value: string): CssDecl => ({
      file: "x.css", line: 1, layer: "", cond: "", sels: [".x"], rule: 0, idx: 0, prop, value, important: false,
    });
    expect(literalHits([at("color", "#fff"), at("background", "rgba(0, 0, 0, 0.4)"), at("font-size", "12px"), at("border-radius", "4px 4px 0 0")])).toHaveLength(4);
    expect(literalHits([at("color", "var(--text)"), at("font-size", "0.9em"), at("border-radius", "50%"), at("box-shadow", "var(--shadow-float)")])).toEqual([]);
  });
});
