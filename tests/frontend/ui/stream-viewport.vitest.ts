/**
 * 消息流的滚动视口止于输入栏（与答题块）上沿（5.1.3 用户报：滚到底时滚动条下端与最后一截被输入栏挡住）。
 *
 * 输入栏 · 答题块浮在 `#message-stream` 底上（absolute），各自量了高报给宿主：输入栏写 `--composer-room`，
 * 答题块写 `--answer-room`（块自己的高 ＋ 它与输入栏之间那道缝）。流的盒子下沿要让出这两样：
 * 视口（连滚动条）整个在它们上面，不是盒子一直到窗底、只拿内容的 padding 垫高。
 *
 * - 真样式表交给 jsdom，量 `.stream` 的下沿离 `#message-stream` 底多远（`var()` 照根上写的那几格代进去）；
 * - 底部 padding 不跟着两格变（变了 ＝ 又回到靠垫高让位）；
 * - 答题块报的那一格 ＝ 块高 ＋ 它自己的下外边距（那道缝）；收起 ⇒ 0。
 *
 * 买不到：jsdom 不排版 —— 盒子真画在哪、滚动条真露没露，照截图看（截图工具的 layout-check）。
 */
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, it, expect, vi, beforeAll, beforeEach } from "vitest";
import { REPO_ROOT } from "../../test-support/repo-root";
import { AnswerBlock, type AnswerHost } from "../../../src/frontend/ui/answer-block";
import type { Tab } from "../../../src/frontend/ui/tab-model";
import type { Ask, Needs } from "../../../src/frontend/ui/session-reads";
import { liveFace } from "../../test-support/session-faces";
import { LOCAL } from "../../../src/frontend/ui/ipc/origin";

/** jsdom 算样式时不进 `@layer` 块：摊平成顶层规则（同一层内先后与权重不变）。 */
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
});

/** 把 `var(--x, 后备)` 照 `vars` 代进去，再算 `calc()` 里的 px 加减（样式表里这一格只用得到这么多）。 */
function px(value: string, vars: Record<string, string>): number {
  let v = value;
  for (let i = 0; i < 8 && v.includes("var("); i++) {
    v = v.replace(/var\(\s*(--[\w-]+)\s*(?:,\s*([^()]*))?\)/g, (_m, name: string, fb?: string) => vars[name] ?? fb ?? "0px");
  }
  v = v.replace(/calc\(/g, "(");
  expect(v, `只认 px 加减：${value}`).toMatch(/^[\d.\spx()+-]+$/);
  return Function(`"use strict"; return (${v.replace(/px/g, "")});`)() as number;
}

/** 当前那条流的下沿离 `#message-stream` 底多远（px）。 */
function measure(composer: number, answer: number): { bottom: number } {
  const root = document.createElement("div");
  root.id = "message-stream";
  const vars = { "--composer-room": `${composer}px`, "--answer-room": `${answer}px` };
  for (const [k, v] of Object.entries(vars)) root.style.setProperty(k, v);
  const sc = document.createElement("div");
  sc.className = "stream active";
  root.appendChild(sc);
  document.body.appendChild(root);
  const cs = getComputedStyle(sc);
  // jsdom 不展开 `inset` 简写：只写了 `inset: 0` 的盒子这里读成 auto ⇒ 当 0（一直到底）。下沿写成别的简写也读成 0 ⇒ 红、不会绿着说假话。
  const b = cs.bottom === "auto" || cs.bottom === "" ? "0px" : cs.bottom;
  const out = { bottom: px(b, vars) };
  root.remove();
  return out;
}

describe("流的视口止于输入栏与答题块上沿", () => {
  it("★ 输入栏高 120 ⇒ 流的下沿离底 120（视口连滚动条在输入栏上面）；变高到 200（多行 · 附件条 · 待发那一排）⇒ 视口跟着缩", () => {
    expect(measure(120, 0).bottom).toBe(120);
    expect(measure(200, 0).bottom).toBe(200);
  });

  it("★ 答题块在 ⇒ 视口再让出块那一格（块高 ＋ 缝）", () => {
    expect(measure(120, 96).bottom).toBe(216);
  });

  it("★ 底部 padding 不跟着两格变：让位靠盒子，不靠垫高内容", () => {
    // jsdom 读不出带 calc 的 padding 简写（读成空）⇒ 直接读样式表里 `.stream {…}` 那一条的 padding 声明。
    const css = readFileSync(resolve(REPO_ROOT, "src/frontend/ui/styles.css"), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
    const rule = /(?:^|\n)\.stream\s*\{([^}]*)\}/.exec(css)?.[1];
    expect(rule, "styles.css 里找得到 `.stream {…}`").toBeDefined();
    const pads = [...rule!.matchAll(/(?:^|;)\s*(padding[\w-]*)\s*:\s*([^;]*)/g)].map((m) => `${m[1]}: ${m[2].trim()}`);
    expect(pads.length, "`.stream` 写着 padding").toBeGreaterThan(0);
    for (const p of pads) expect(p).not.toMatch(/--composer-room|--answer-room/);
  });
});

const ASKS = JSON.parse(readFileSync(resolve(__dirname, "../../__fixtures__/needs-ask.golden.json"), "utf8")) as Record<"approve", Ask>;
const NOW = Date.parse("2026-10-11T12:00:00Z");

function waitingTab(): Tab {
  const n = { kind: "approve", tool: "Bash", call: "toolu_1", what: "w", text: "核心·等批准", head: "核心·等批准 · Bash", headCode: null, tone: "need", rank: 1, waitedMs: 12_000, ask: ASKS.approve, manualText: null, receivedAt: NOW } as Needs;
  return { sessionId: "s1", origin: LOCAL, face: liveFace("needs_you"), needs: n } as unknown as Tab;
}

describe("答题块报的那一格含它与输入栏之间的缝", () => {
  beforeEach(() => {
    document.body.innerHTML = "";
  });

  it("★ 块高 300、下外边距 16 ⇒ 报 316；块收起 ⇒ 报 0", () => {
    let cur: Tab | null = waitingTab();
    const host = {
      active: () => cur,
      answer: vi.fn(async () => ({ kind: "answered" as const })),
      reread: vi.fn<(sid: string) => void>(),
      toast: vi.fn<(title: string, detail?: string) => void>(),
      room: vi.fn<(px: number) => void>(),
    } satisfies AnswerHost;
    const b = new AnswerBlock(host);
    // jsdom 不排版：块高打桩；缝照块自己的下外边距（真样式里是令牌，这里直接给值）。
    Object.defineProperty(b.el, "offsetHeight", { configurable: true, get: () => 300 });
    b.el.style.marginBottom = "16px";
    document.body.append(b.el);
    b.render(NOW);
    expect(b.el.hidden).toBe(false);
    expect(host.room).toHaveBeenLastCalledWith(316);
    cur = null;
    b.render(NOW);
    expect(host.room).toHaveBeenLastCalledWith(0);
  });

  it("★ 块与输入栏之间那道缝写在块自己的下外边距上（不在 bottom 里）—— 上一格量的就是它", () => {
    const css = readFileSync(resolve(REPO_ROOT, "src/frontend/ui/answer-block.module.css"), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
    const rule = /\.abBlock\s*\{([^}]*)\}/.exec(css)?.[1];
    expect(rule, "answer-block.module.css 里找得到 `.abBlock {…}`").toBeDefined();
    const decl = (p: string) => new RegExp(`(?:^|;)\\s*${p}\\s*:\\s*([^;]*)`).exec(rule!)?.[1].trim();
    expect(decl("margin-bottom"), "缝是块的下外边距").toMatch(/^var\(--space-\d+\)$/);
    expect(decl("bottom"), "bottom 只到输入栏上沿，不另加缝").toMatch(/^var\(--composer-room(?:,\s*0(?:px)?)?\)$/);
  });
});
