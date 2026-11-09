/**
 * 第二级估高（估高 Worker 用 pretext 排正文那一段）：版本钉在 1.x · 同一份语料逐条量高与冻下的那一份一致。
 */
import { describe, it, expect, vi } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import type { LineRecord } from "../../../src/frontend/ui/generated/LineRecord";

// 下面给 jsdom 装的确定 canvas 只在量高那一段里有，pretext 用的是 `vi.resetModules()` 之后另拉的一份。
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
 * 助手正文换成后端交的块树之后，在同一条件下（上面那个 canvas · 秤 2 语料里第二级那 38 件 · 生产那一句）现冻的高，四舍五入到 0.1 px。
 * 两种引擎标识下排出的是同一份（不按引擎分折行），一个列宽一组。只有数，没有正文。
 */
const HEIGHTS_009: Record<(typeof WIDTHS)[number], number[]> = {
  780: [
    2532.5, 4781, 3578.3, 2365.8, 2935.3, 2221.6, 2501.7, 2174.1, 1446, 2906.1, 4103.4, 11261.6, 6000.5, 4317.6,
    4103.4, 3772.8, 6603.2, 4317.6, 75.8, 75.8, 100.5, 75.8, 75.8, 75.8, 100.5, 75.8, 100.5, 6483, 71.7, 180.2, 115.1,
    93.4, 93.4, 201.9, 245.3, 71.7, 93.4, 115.1,
  ],
  520: [
    2755.3, 5080.8, 3921.8, 2788.1, 3504.9, 2419.6, 2749.2, 2446.4, 1520.2, 3398.3, 5142.7, 12811.9, 6751.4, 4769.1,
    5142.7, 4482.6, 7506.2, 4769.1, 75.8, 75.8, 100.5, 100.5, 100.5, 100.5, 125.3, 100.5, 100.5, 9411, 71.7, 180.2,
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

describe("量高逐条与冻下的那一份一致：同一份语料 × 确定的 canvas", () => {
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

/**
 * 估高的字号 · 行高 · 回退列宽不另抄令牌的数：模块求值时读一次 `:root` 的计算样式。
 * jsdom 不挂样式表 ⇒ 不注入时读到的是 tokens.css 那一份；在 `:root` 上注入另一份正文行高，重新求值模块，同一条料的粗估随之变。
 */
describe("估高跟着 :root 的计算样式走", () => {
  const facts = { o: 0, n: 1, t: "reply", dr: "card", ch: 460, cj: 60, pl: 3 } as const;
  async function estimateWith(lineHeightProse: string | null): Promise<number> {
    const root = document.documentElement;
    if (lineHeightProse === null) root.style.removeProperty("--line-height-prose");
    else root.style.setProperty("--line-height-prose", lineHeightProse);
    vi.resetModules();
    try {
      const m = await import("../../../src/frontend/ui/height-estimate");
      return m.estimateFromFacts(facts, 780);
    } finally {
      root.style.removeProperty("--line-height-prose");
      vi.resetModules();
    }
  }

  it("注入另一份 --line-height-prose ⇒ 粗估随之变；撤掉 ⇒ 回到 tokens.css 那一份", async () => {
    const base = await estimateWith(null);
    const doubled = await estimateWith("3.3");
    expect(base, "不注入时估出一个正数").toBeGreaterThan(0);
    expect(doubled, "正文行高翻倍，正文那一段的高跟着变").toBeGreaterThan(base);
    expect(await estimateWith(null), "撤掉注入 ⇒ 回到 tokens.css 那一份").toBe(base);
  });
});
