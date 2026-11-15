/**
 * 「开始用」卡（主窗口空态底下那一块）在底部抽屉开着时不被盖住。
 *
 * `#app` 网格里抽屉占消息区下面那一行（`styles/layout.css`）：抽屉一开，`#message-stream` 那一格就矮那么一截，
 * 它自己 `overflow: hidden` ⇒ 里面照窗口高（`vh`）往下排的东西，超出那一格的下半截就被抽屉上沿切掉（缺陷截图里卡片只剩一行）。
 * 修好的样子与消息流让位输入栏同一套：空态盒子钉在 `#message-stream` 里（上沿 0，下沿让出输入栏 ＋ 答题块），自己滚；
 * 竖着怎么摆只照这一格的高，不照窗口高。
 *
 * - 真样式表交给 jsdom，算空态盒子的上下沿（`var()` 照根上写的那几格代进去）；
 * - 抽屉开着 ⇒ 消息区那一格的下沿就是抽屉上沿 ⇒ 盒子（连卡）下沿在抽屉上沿之上；盒子装不下 ⇒ 盒子自己滚，卡的下半截滚得到。
 *
 * 买不到：jsdom 不排版 —— 卡片真画在哪，照截图看（截图工具 `main-empty` 那一张，抽屉开着）。
 */
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, it, expect, beforeAll, vi } from "vitest";
import { REPO_ROOT } from "../../test-support/repo-root";
import { MainEmpty } from "../../../src/frontend/ui/main-empty";
import type { FleetLines } from "../../../src/frontend/ui/generated/FleetLines";
import type { Readiness } from "../../../src/frontend/ui/settings/readiness-reads";

vi.mock("../../../src/frontend/ui/copy-table", () => ({ copyText: (k: string) => k }));

/** jsdom 算样式时不进 `@layer` 块：摊平成顶层规则（同一层内先后与权重不变）。 */
function flatten(rules: CSSRuleList): string[] {
  return [...rules].flatMap((r) => (r.constructor.name === "CSSLayerBlockRule" ? flatten((r as CSSGroupingRule).cssRules) : [r.cssText]));
}

const SHEET = readFileSync(resolve(REPO_ROOT, "src/frontend/ui/styles.css"), "utf8");

beforeAll(() => {
  const raw = document.createElement("style");
  raw.textContent = SHEET;
  document.head.appendChild(raw);
  const flat = flatten((raw.sheet as CSSStyleSheet).cssRules);
  raw.remove();
  const style = document.createElement("style");
  style.textContent = flat.join("\n");
  document.head.appendChild(style);
});

/** 把 `var(--x, 后备)` 照 `vars` 代进去，再算 `calc()` 里的 px 加减。 */
function px(value: string, vars: Record<string, string>): number {
  let v = value;
  for (let i = 0; i < 8 && v.includes("var("); i++) {
    v = v.replace(/var\(\s*(--[\w-]+)\s*(?:,\s*([^()]*))?\)/g, (_m, name: string, fb?: string) => vars[name] ?? fb ?? "0px");
  }
  v = v.replace(/calc\(/g, "(");
  expect(v, `只认 px 加减：${value}`).toMatch(/^[\d.\spx()+-]+$/);
  return Function(`"use strict"; return (${v.replace(/px/g, "")});`)() as number;
}

/** 样式表里选择器列表含 `.empty-state` 本身（不是子孙）的那几条规则体有没有写 `inset`（jsdom 不展开这个简写，只好读原文）。 */
function emptyDeclaresInset(): boolean {
  const bare = SHEET.replace(/\/\*[\s\S]*?\*\//g, "");
  return [...bare.matchAll(/([^{}]+)\{([^{}]*)\}/g)].some(
    ([, sel, body]) => sel.split(",").some((x) => x.trim() === ".empty-state") && /(^|[;\s])inset\s*:/.test(body),
  );
}

interface Box {
  /** 盒子上沿离窗顶。 */
  top: number;
  /** 盒子下沿离窗顶。 */
  bottom: number;
  /** 盒子自己滚（装不下的那一截滚得到）。 */
  scrolls: boolean;
}

/**
 * 窗高 `win`、消息区那一格从 `areaTop` 到 `areaBottom`（抽屉开着 ⇒ `areaBottom` 是抽屉上沿）：空态盒子的上下沿。
 * 盒子不是钉在那一格里的（不是 absolute · 上下沿不是 px）⇒ 当它照文档流往下排：上沿 ＝ 那一格上沿 ＋ margin-top，
 * 下沿 ＝ 上沿 ＋ 内容高 `content`（jsdom 不排版，内容高由调用方给）。
 */
function emptyBox(win: number, areaTop: number, areaBottom: number, content: number, composer = 0, answer = 0): Box {
  const root = document.createElement("div");
  root.id = "message-stream";
  const vars: Record<string, string> = { "--composer-room": `${composer}px`, "--answer-room": `${answer}px` };
  for (const [k, v] of Object.entries(vars)) root.style.setProperty(k, v);
  const el = document.createElement("div");
  el.className = "empty-state";
  root.appendChild(el);
  document.body.appendChild(root);
  const cs = getComputedStyle(el);
  const scrolls = cs.overflowY === "auto" || cs.overflowY === "scroll";
  let out: Box;
  if (cs.position === "absolute") {
    // jsdom 不展开 `inset` 简写：读成 auto 的那一边，规则里写了 `inset` 才当钉住（0）；没写 ⇒ 那一边没钉，盒子跟内容一样高。
    const inset = emptyDeclaresInset();
    const pinned = (s: string): number | null => (s === "auto" || s === "" ? (inset ? 0 : null) : px(s, vars));
    const t = pinned(cs.top);
    const b = pinned(cs.bottom);
    const top = areaTop + (t ?? 0);
    out = { top, bottom: b === null ? top + content : areaBottom - b, scrolls };
  } else {
    const mt = cs.marginTop.endsWith("vh") ? (parseFloat(cs.marginTop) / 100) * win : px(cs.marginTop || "0px", vars);
    const top = areaTop + mt;
    out = { top, bottom: top + content, scrolls: false };
  }
  root.remove();
  return out;
}

const READY: Readiness = {
  skipped: false,
  steps: [
    { id: "a", name: "一", hint: "一句", done: false, act: { label: "去", go: "aliases" } },
    { id: "b", name: "二", hint: "一句", done: true, act: { label: "去", go: "aliases" } },
  ],
} as unknown as Readiness;

describe("「开始用」卡让出抽屉那一截", () => {
  it("卡挂在空态盒子 `.empty-state` 里（下面几条量的就是这个盒子）", () => {
    const m = new MainEmpty({ open: () => {}, skip: () => {} });
    m.face({ line: "本机无会话", hint: "一句" } as FleetLines);
    m.readiness(READY);
    expect(m.el.classList.contains("empty-state")).toBe(true);
    expect(m.el.querySelector(':scope > [data-role="start-card"]')).not.toBeNull();
  });

  it("★ 抽屉开着（窗 700 · 消息区 40–420 · 抽屉上沿 420）⇒ 空态盒子（连卡）下沿在抽屉上沿之上", () => {
    // 首屏那一句 ＋ 卡 ≈ 360px（缺陷截图里那一份的量级）。
    const b = emptyBox(700, 40, 420, 360);
    expect(b.bottom).toBeLessThanOrEqual(420);
    expect(b.top).toBeGreaterThanOrEqual(40);
  });

  it("★ 盒子装不下（抽屉拖到一半、窗又矮）⇒ 盒子自己滚，卡的下半截滚得到，不被切掉", () => {
    const b = emptyBox(500, 40, 230, 360);
    expect(b.bottom).toBeLessThanOrEqual(230);
    expect(b.scrolls).toBe(true);
  });

  it("★ 与消息流同一套：下沿再让出输入栏 ＋ 答题块", () => {
    const b = emptyBox(700, 40, 420, 200, 120, 30);
    expect(b.bottom).toBe(420 - 150);
  });
});
