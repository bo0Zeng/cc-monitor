/**
 * Batch13-F38:估高单测。
 * 环境事实(D 审计实证):jsdom 里 pretext import 成功但首次 prepare 因无 canvas
 * 抛错 → 实际执行「运行时抛错 → 算术降级」分支;fallbackTextHeight 直测钉住降级
 * 算术;涉及 textHeight 的断言按降级路径的精确值写(若未来测试环境获得 canvas,
 * pretext 真值路径结果不同,这些断言会有意识地失败提醒重校)。
 */
import { describe, it, expect, vi } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import type { LineRecord } from "../../../src/frontend/ui/generated/LineRecord";
import {
  fallbackTextHeight,
  codeBlockHeight,
  extractProseText,
  estimateStreamNodeHeight,
  applyIntrinsicSize,
} from "../../../src/frontend/ui/height-estimate";

const LH_PROSE = 15 * 1.65;
const LH_BASE = 14 * 1.55;
const LH_MONO = 13 * 1.55;

describe("fallbackTextHeight", () => {
  it("空文本高度为 0", () => {
    expect(fallbackTextHeight("", 15, LH_PROSE, 780)).toBe(0);
  });
  it("单短行 = 一个行高", () => {
    expect(fallbackTextHeight("hello", 15, LH_PROSE, 780)).toBeCloseTo(LH_PROSE);
  });
  it("硬换行逐行计数,CRLF 的 \\r 不污染宽度", () => {
    expect(fallbackTextHeight("a\r\nb\r\nc", 15, LH_PROSE, 780)).toBeCloseTo(3 * LH_PROSE);
  });
  it("CJK 按全宽折行(50 个汉字 @15px 在 375px 宽 = 2 行)", () => {
    expect(fallbackTextHeight("汉".repeat(50), 15, LH_PROSE, 375)).toBeCloseTo(2 * LH_PROSE);
  });
});

describe("codeBlockHeight", () => {
  it("行数线性(10 行比 1 行多 9 个 mono 行高)", () => {
    const one = codeBlockHeight("x");
    const ten = codeBlockHeight(Array(10).fill("x").join("\n"));
    expect(ten - one).toBeCloseTo(9 * LH_MONO);
  });
  it("空代码块按 1 行计", () => {
    expect(codeBlockHeight("")).toBe(codeBlockHeight("x"));
  });
});

describe("extractProseText(R1:块感知提取)", () => {
  it("<br> 还原为换行(user 卡 renderPlainText 形态)", () => {
    const el = document.createElement("div");
    el.innerHTML = "line1<br>line2<br>line3";
    expect(extractProseText(el).text).toBe("line1\nline2\nline3");
  });
  it("<p>/<li> 边界插换行并计块数", () => {
    const el = document.createElement("div");
    el.innerHTML = "<p>para1</p><p>para2</p><ul><li>a</li><li>b</li></ul>";
    const r = extractProseText(el);
    expect(r.text).toBe("para1\npara2\na\nb");
    expect(r.blockCount).toBe(4);
  });
  // 🔴 下面三条钉的是 2026-09-18 那次回摆(秤 2 §2):R1 保留了**所有** \n,而 marked
  //    产出的 HTML 里绝大多数换行只是**源码排版**——浏览器折叠成空格,估高却当硬断行
  //    ⇒ 正文卡系统性 ~2× 虚高。口径:只有块边界与 <br> 算断行。
  it("HTML 源码里的排版换行归一成空格(不是硬断行)", () => {
    const el = document.createElement("div");
    el.innerHTML = "<p>one\ntwo</p>\n<p>three</p>";
    expect(extractProseText(el).text).toBe("one two\nthree");
  });
  it("表格一行算一行(一行 4 个单元格不是 4 行)", () => {
    const el = document.createElement("div");
    el.innerHTML =
      "<table><tbody>\n<tr>\n<td>a</td>\n<td>b</td>\n</tr>\n<tr>\n<td>c</td>\n<td>d</td>\n</tr>\n</tbody></table>";
    const r = extractProseText(el);
    expect(r.text).toBe("a b\nc d");
    expect(r.blockCount).toBe(2);
  });
  it("真 pre-wrap 容器(.block-body 非 md)里的换行原样保留", () => {
    const el = document.createElement("div");
    el.innerHTML = '<pre class="block-body block-body-json">{\n  "a": 1\n}</pre>';
    expect(extractProseText(el).text).toBe('{\n  "a": 1\n}');
  });
  it("跳过 .code-block 与 .katex-mathml(单独估/aria 重复)", () => {
    const el = document.createElement("div");
    el.innerHTML =
      '<p>before</p><div class="code-block"><pre>const x=1</pre></div><span class="katex-mathml">dup</span><p>after</p>';
    expect(extractProseText(el).text).toBe("before\nafter");
  });
});

describe("estimateStreamNodeHeight", () => {
  it("折叠的顶层 details(压缩卡)= summary 常数,与内容量无关", () => {
    const d = document.createElement("details");
    d.className = "card card-compact";
    d.innerHTML = "<summary>s</summary><div>" + "x".repeat(5000) + "</div>";
    expect(estimateStreamNodeHeight(d)).toBe(38);
  });
  it("「几步」卡 = 每步收着一行 × 步数 + 步间距,与收着的内容量无关", () => {
    const card = document.createElement("div");
    card.className = "card card-steps";
    card.innerHTML =
      '<details class="block-collapsible"><summary>a</summary><div>' + "x".repeat(5000) + "</div></details>" +
      '<details class="block-collapsible"><summary>b</summary></details>' +
      '<details class="block-collapsible"><summary>c</summary></details>';
    expect(estimateStreamNodeHeight(card)).toBe(3 * 38 + 2 * 4);
    const one = document.createElement("div");
    one.className = "card card-steps";
    one.innerHTML = '<details class="block-collapsible"><summary>a</summary></details>';
    expect(estimateStreamNodeHeight(one)).toBe(38);
    // 派出子 agent 那一步不是 <details>，也是一行抬头；展开着的那一步按内容算（比一行高）
    const mixed = document.createElement("div");
    mixed.className = "card card-steps";
    mixed.innerHTML =
      '<div data-role="run-card"><button>agent</button></div>' +
      '<details class="block-collapsible" open><summary>a</summary><div class="block-text"><p>' + "长".repeat(400) + "</p></div></details>";
    const solo = document.createElement("div");
    solo.className = "card card-steps";
    solo.innerHTML = mixed.innerHTML.replace('<div data-role="run-card"><button>agent</button></div>', "");
    const open = estimateStreamNodeHeight(solo)!;
    expect(open, "展开着的那一步没按内容算").toBeGreaterThan(38);
    expect(estimateStreamNodeHeight(mixed)).toBe(38 + 4 + open);
    mixed.querySelector("details")!.open = false;
    expect(estimateStreamNodeHeight(mixed)).toBe(38 + 4 + 38);
  });
  it("展开的顶层 details 返回 null(走 CSS 兜底+auto 记忆)", () => {
    const d = document.createElement("details");
    d.open = true;
    d.innerHTML = "<summary>s</summary><div>body</div>";
    expect(estimateStreamNodeHeight(d)).toBeNull();
  });
  it("user 气泡:<br> 断行参与估高 = 降级路径精确值", () => {
    const el = document.createElement("div");
    el.className = "card card-user";
    el.innerHTML = '<div class="card-body">短行1<br>短行2<br>短行3</div>';
    const expected = fallbackTextHeight("短行1\n短行2\n短行3", 14, LH_BASE, 780 * 0.8 - 34);
    expect(estimateStreamNodeHeight(el)).toBeCloseTo(expected);
  });
  it("assistant 卡分块累加:prose + 嵌套代码块 + 折叠 details(R2)", () => {
    const el = document.createElement("div");
    el.className = "card card-assistant";
    const code = Array(10).fill("x").join("\n");
    el.innerHTML =
      '<div class="card-header">h</div><div class="card-body">' +
      '<div class="block-text"><p>hello</p><div class="code-block"><pre>' +
      code +
      "</pre></div></div>" +
      '<details class="block-collapsible"><summary>💭</summary></details>' +
      "</div>";
    const prose = fallbackTextHeight("hello", 15, LH_PROSE, 780) + 10; // + BLOCK_GAP
    const expected = 22 + prose + codeBlockHeight(code) + 24 + 38; // header + text块 + code+margin + details
    expect(estimateStreamNodeHeight(el)).toBeCloseTo(expected);
  });
  it("api-error/slash 细条卡走常数(非 120px 兜底)", () => {
    const err = document.createElement("div");
    err.className = "card card-api-error";
    expect(estimateStreamNodeHeight(err)).toBe(60); // 抬头 · 下一步 · 「原文」三行（§5.2.5）
    const slash = document.createElement("div");
    slash.className = "card card-slash";
    // 19 = 12px(--font-size-small) × 1.55 的 **content-box** 值(秤 2 的 B 段实测
    // `contain-intrinsic-size` 吃 content-box;原来的 34 是按 border-box 手算的,
    // 多了一份 padding 6×2 ⇒ p90 相对误差 82.9%)。
    // ⚠ 2026-09-18 之前 `applyIntrinsicSize` 的 24px 地板会把 19 顶成 24(写进 style 的是 24);
    //   地板已去掉 ⇒ 现在 19 原样出货,见下面那个 describe。
    expect(estimateStreamNodeHeight(slash)).toBe(19);
  });
  it("认不出的形态返回 null(CSS 兜底接管)", () => {
    const el = document.createElement("div");
    el.className = "card-unknown-kind";
    expect(estimateStreamNodeHeight(el)).toBeNull();
  });
});

describe("applyIntrinsicSize(F39 复用的契约面)", () => {
  it("写入格式 auto <N>px、四舍五入、**不夹取**(地板已去掉)", () => {
    const el = document.createElement("div");
    el.className = "card card-user";
    el.innerHTML = '<div class="card-body">hi</div>';
    applyIntrinsicSize(el);
    const v = el.style.getPropertyValue("contain-intrinsic-size");
    expect(v).toMatch(/^auto \d+px$/);
    // 🔴 这一条 2026-09-18 之前断的是 `>= 24`(`applyIntrinsicSize` 里那个 `Math.max(24, …)` 地板)。
    //   地板已整个去掉:
    //   它防的两件事都是空集(0 不可达、负值写不出来且浏览器自己会拒),而它让 13/83 张卡虚高,
    //   300 张细条卡的风暴里总高虚高 +20.5%/+23.8%。保险职责搬到判据层 ——
    //   秤 2 的 **F1**「每 class 估值最小值 ≥ 登记常数」会红,`Math.max` 只会静默抹平。
    //   ⇒ 这里从"断一个下限"改成"断**没有**下限":写进去的必须逐位 = round(估值)。
    expect(v).toBe(`auto ${Math.round(estimateStreamNodeHeight(el) as number)}px`);
    // 这张卡的估值是 21.7(LH_BASE = 14 × 1.55)⇒ 落地 22;旧地板下会被顶成 24
    expect(v).toBe("auto 22px");
  });

  it("**低于旧地板 24 的估值原样出货**(地板去掉之后这一条才成立)", () => {
    for (const [cls, expected] of [
      ["card card-api-retry", "auto 17px"], // 常数 17,旧地板下写的是 24
      ["card card-slash", "auto 19px"], //    常数 19,旧地板下写的是 24
      ["card card-bash-input", "auto 19px"], // 常数 19,旧地板下写的是 24
    ] as const) {
      const el = document.createElement("div");
      el.className = cls;
      applyIntrinsicSize(el);
      expect(el.style.getPropertyValue("contain-intrinsic-size"), cls).toBe(expected);
    }
  });
  it("估不出(null)时不写 style", () => {
    const el = document.createElement("div");
    el.className = "card-unknown-kind";
    applyIntrinsicSize(el);
    expect(el.style.getPropertyValue("contain-intrinsic-size")).toBe("");
  });
});

// ═══════════════════════════════════════════════════════════════════════
// 升 Pretext 1.0（A17-45-3-14-1）：版本钉在 1.x · 量高逐条与 0.0.9 一致。
// 放在文件最后：下面给 jsdom 装的确定 canvas 只在这一段里有，pretext 用的是 `vi.resetModules()` 之后另拉的一份。
// ═══════════════════════════════════════════════════════════════════════

const REPO = resolve(__dirname, "../../..");

describe("Pretext 版本：package.json 钉的与锁里装的都是 1.x、同一个数", () => {
  it("钉版 ＝ 锁里 ＝ 1.x", () => {
    const pkg = JSON.parse(readFileSync(resolve(REPO, "package.json"), "utf8")) as {
      dependencies: Record<string, string>;
    };
    const lock = JSON.parse(readFileSync(resolve(REPO, "package-lock.json"), "utf8")) as {
      packages: Record<string, { version?: string; dependencies?: Record<string, string> }>;
    };
    const pinned = pkg.dependencies["@chenglou/pretext"];
    const locked = lock.packages["node_modules/@chenglou/pretext"]?.version;
    const rootPinned = lock.packages[""]?.dependencies?.["@chenglou/pretext"];
    expect(pinned, "package.json 钉精确版本（不带 ^ ~）").toMatch(/^\d+\.\d+\.\d+$/);
    expect(pinned.startsWith("1."), `package.json 钉的是 ${pinned}`).toBe(true);
    expect(locked, "锁里装的").toBe(pinned);
    expect(rootPinned, "锁的根那一份").toBe(pinned);
  });
});

/**
 * 确定的 canvas：字宽只跟字符类有关（CJK 一个字号宽 · 空白 0.28em · 数字 0.55em · 大写 0.66em · 小写 0.5em · 其余 0.34em），
 * 字号从 font 串开头的 `<n>px` 读；font 串不是「字号 + 字族」⇒ 照 Canvas 的样子不收（保留上一个）。
 */
function charEm(cp: number): number {
  if (cp >= 0x2e80) return 1;
  if (cp === 0x20 || cp === 0x09 || cp === 0xa0) return 0.28;
  if (cp >= 0x30 && cp <= 0x39) return 0.55;
  if (cp >= 0x41 && cp <= 0x5a) return 0.66;
  if (cp >= 0x61 && cp <= 0x7a) return 0.5;
  return 0.34;
}
class FakeContext {
  private f = "10px sans-serif";
  letterSpacing = "0px";
  lang = "";
  get font(): string {
    return this.f;
  }
  set font(v: string) {
    if (/^\s*\d+(\.\d+)?px\s+\S/.test(v)) this.f = v.trim();
  }
  measureText(text: string): TextMetrics {
    const size = Number(/^(\d+(?:\.\d+)?)px/.exec(this.f)?.[1] ?? 10);
    let w = 0;
    for (const ch of text) w += charEm(ch.codePointAt(0) ?? 0) * size;
    return {
      width: w,
      actualBoundingBoxLeft: 0,
      actualBoundingBoxRight: w,
      actualBoundingBoxAscent: size * 0.8,
      actualBoundingBoxDescent: size * 0.2,
      fontBoundingBoxAscent: size * 0.9,
      fontBoundingBoxDescent: size * 0.25,
      emHeightAscent: size * 0.8,
      emHeightDescent: size * 0.2,
      hangingBaseline: size * 0.7,
      alphabeticBaseline: 0,
      ideographicBaseline: -size * 0.2,
    } as TextMetrics;
  }
}
class FakeOffscreenCanvas {
  constructor(
    public width: number,
    public height: number,
  ) {}
  getContext(kind: string): FakeContext | null {
    return kind === "2d" ? new FakeContext() : null;
  }
}

const UA = {
  chromium:
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/153.0.0.0 Safari/537.36",
  webkit: "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Safari/605.1.15",
} as const;
const WIDTHS = [780, 520] as const;

/**
 * 0.0.9 在同一条件下（上面那个 canvas · 秤 2 语料里第二级那 38 件 · 生产那一句）排出的高，四舍五入到 0.1 px。
 * 两种引擎标识下 0.0.9 排出的是同一份（它不按引擎分折行），一个列宽一组。只有数，没有正文。
 */
const HEIGHTS_009: Record<(typeof WIDTHS)[number], number[]> = {
  780: [
    2483, 4818.6, 3732.7, 2435.7, 3004, 2332.6, 2771.7, 2333.9, 1495.5, 3127.7, 4224.6, 11303.6, 5983.7, 4466.2,
    4224.6, 3938.1, 6688.3, 4466.2, 75.8, 75.8, 100.5, 75.8, 75.8, 75.8, 100.5, 75.8, 100.5, 6483, 71.7, 180.2, 115.1,
    93.4, 93.4, 201.9, 245.3, 71.7, 93.4, 115.1,
  ],
  520: [
    2705.8, 5155.4, 4192.8, 2934.3, 3621.3, 2604.9, 2994.4, 2630.9, 1569.7, 3602.6, 5245.6, 13305.5, 6812.4, 5018.5,
    5245.6, 4600.3, 7480.9, 5018.5, 75.8, 75.8, 100.5, 100.5, 100.5, 100.5, 125.3, 100.5, 100.5, 9411, 71.7, 180.2,
    115.1, 93.4, 93.4, 223.6, 245.3, 71.7, 115.1, 115.1,
  ],
};

interface Measured {
  h: number;
  /** 这一件差一行是多少 px（行高 × 外推倍数）。 */
  oneLine: number;
}

/** 语料里每条第二级那一件，在一个引擎标识 × 一个列宽下经估高 Worker 的 `measure` 排出来的高。 */
async function refinedCorpusHeights(engine: keyof typeof UA, colW: number): Promise<Measured[]> {
  vi.resetModules();
  vi.stubGlobal("OffscreenCanvas", FakeOffscreenCanvas);
  vi.stubGlobal("navigator", {
    ...navigator,
    userAgent: UA[engine],
    vendor: engine === "webkit" ? "Apple Computer, Inc." : "Google Inc.",
  });
  try {
    const est = await import("../../../src/frontend/ui/height-estimate");
    const { measure } = await import("../../../src/frontend/ui/height-worker");
    const fixture = readFileSync(resolve(REPO, "tests/__fixtures__/scale2-height-line-records.jsonl"), "utf8");
    const out: Measured[] = [];
    for (const line of fixture.split("\n")) {
      if (!line.trim()) continue;
      const it = est.refineItemOf(JSON.parse(line) as LineRecord, colW);
      if (!it) continue;
      out.push({ h: Math.round(est.refinedHeight(it, measure) * 10) / 10, oneLine: it.lineHeightPx * it.scale });
    }
    return out;
  } finally {
    vi.unstubAllGlobals();
    vi.resetModules();
  }
}

describe("量高逐条与改前（0.0.9）一致：同一份语料 × 确定的 canvas", () => {
  for (const engine of ["chromium", "webkit"] as const) {
    for (const colW of WIDTHS) {
      it(`${engine} · 列宽 ${colW}：差一行以内的逐条放过，差一行以上的不超过一成`, async () => {
        const now = await refinedCorpusHeights(engine, colW);
        const before = HEIGHTS_009[colW];
        expect(now.length, "第二级的件数（语料或 refineItemOf 变了 ⇒ 先重冻上面那组数）").toBe(before.length);
        const over = now
          .map((m, i) => ({ i, was: before[i], now: m.h, oneLine: m.oneLine }))
          .filter((d) => Math.abs(d.now - d.was) > d.oneLine + 0.05);
        expect(over.length, `差一行以上的件：${JSON.stringify(over)}`).toBeLessThanOrEqual(Math.floor(before.length / 10));
        const drift = now.reduce((a, m, i) => a + Math.abs(m.h - before[i]), 0) / before.reduce((a, h) => a + h, 0);
        expect(drift, "合计高度的相对漂移").toBeLessThan(0.01);
      });
    }
  }
  it("字体串 canvas 不收 ⇒ Worker 那一件回 null（留第一级），不把整批抛掉", async () => {
    const [{ h }] = await refinedCorpusHeights("chromium", 780);
    expect(h).toBeGreaterThan(0);
    vi.resetModules();
    vi.stubGlobal("OffscreenCanvas", FakeOffscreenCanvas);
    try {
      const { measure } = await import("../../../src/frontend/ui/height-worker");
      const bad = { text: "abc", font: "", lineHeightPx: 20, widthPx: 300, fixed: 0, scale: 1 };
      const posted: unknown[] = [];
      vi.stubGlobal("postMessage", (m: unknown) => posted.push(m));
      expect(() => measure(bad)).toThrow(RangeError);
      (self.onmessage as (ev: MessageEvent) => void)({ data: { id: 7, items: [bad] } } as MessageEvent);
      expect(posted).toEqual([{ id: 7, heights: [null] }]);
    } finally {
      vi.unstubAllGlobals();
      vi.resetModules();
    }
  });
});
