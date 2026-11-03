// 正文排好之后补的占位（图）与几处「界面手上有原文」的 Markdown：一律后端解析成树（`md-doc`），照 `md-view` 排。
// 界面这一侧没有解析器、没有公式排版器：marked · highlight.js · dompurify · katex 不在依赖里；公式是后端排好的画单。
import { describe, it, expect, vi, beforeEach } from "vitest";
import { UNTIL_CEILING } from "../../test-support/wall-ceiling";

vi.mock("../../../src/frontend/ui/ipc/chan-caller", async (orig) => ({
  ...(await orig<typeof import("../../../src/frontend/ui/ipc/chan-caller")>()),
  callTyped: vi.fn(),
}));

import { readFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { productionTsFiles } from "../../test-support/production-sources";
import { callTyped } from "../../../src/frontend/ui/ipc/chan-caller";
import { renderMessage } from "../../../src/frontend/ui/cards";
import { mountWhole } from "../../../src/frontend/ui/render";
import type { Doc } from "../../../src/frontend/ui/generated/Doc";
import { LOCAL } from "../../../src/frontend/ui/ipc/origin";

const callMock = callTyped as unknown as ReturnType<typeof vi.fn>;
const ROOT = resolve(__dirname, "../../..");

// 树取自 md-core 金样：`\[..\]` · `\(..\)` · 段落紧挨着的 `$$` 三种写法，后端都认成公式。
const golden = new Map(
  (JSON.parse(readFileSync(resolve(ROOT, "tests/__fixtures__/md-core.golden.json"), "utf8")) as { name: string; doc: Doc }[]).map((c) => [c.name, c.doc]),
);
const doc = (name: string): Doc => {
  const d = golden.get(name);
  if (!d) throw new Error(`金样里没有「${name}」`);
  return d;
};
const ctx = (lazy = false) =>
  ({ parentPath: "/p/s.jsonl", origin: LOCAL, toolUseNames: new Map(), toolUseElements: new Map(), pendingToolResults: new Map(), lazy }) as never;

beforeEach(() => {
  callMock.mockReset();
  callMock.mockImplementation(async (_o: string, cmd: string) => (cmd === "prose-hl" ? { hl: [] } : undefined));
});

describe("mountWhole：后端交的整棵树（来话 · 计划卡 · 思考 · 计划格）照正文卡同一套排", () => {
  it("三种公式写法：当场画成 SVG，与正文卡排出来的同一份；惰路也不留占位", () => {
    for (const name of ["行内公式两种写法", "\\[ 块公式", "段落紧挨着的 $$ 块"]) {
      const el = mountWhole(doc(name));
      const prose = renderMessage(
        { agent: "claude", t: "reply", id: "r", at: "2026-01-01T00:00:00.000Z", blocks: [{ type: "prose", doc: doc(name), more: "" }], autoReply: false, draw: "card", human: false } as never,
        ctx(),
      );
      if (prose.kind !== "card") throw new Error(prose.kind);
      expect(el.querySelectorAll("[data-math] svg").length, name).toBeGreaterThan(0);
      expect(el.outerHTML, name).toBe(prose.element.querySelector(".block-text")?.firstElementChild?.outerHTML);
      expect(mountWhole(doc(name), true).outerHTML, name).toBe(el.outerHTML);
    }
  });

  it("多段的长树一次排完", () => {
    const blocks = Array.from({ length: 3 }, (_, i) => ({ t: "p" as const, k: [`第${i}段`] }));
    const el = mountWhole({ blocks, segs: [1, 2] });
    expect(el.textContent).toBe("第0段第1段第2段");
  });
});

describe("界面手上有原文的那几处：问后端要树（md-doc），照树排", () => {
  it("★ 思考展开那一下问 md-doc（prose），回来的树照排、公式当场排", async () => {
    const text = "想想 \\(a+b\\)";
    callMock.mockImplementation(async (_o: string, cmd: string) => (cmd === "md-doc" ? doc("行内公式两种写法") : { hl: [] }));
    const res = renderMessage({ agent: "claude", t: "reply", id: "u1", at: "2026-01-01T00:00:00Z", blocks: [{ type: "thinking", text, preview: text }], autoReply: false } as never, ctx(true));
    expect(res.kind).toBe("steps");
    const d = (res as { units: HTMLElement[] }).units[0] as HTMLDetailsElement;
    d.open = true;
    d.dispatchEvent(new Event("toggle"));
    await vi.waitFor(() => expect(d.querySelector("[data-math] svg")).not.toBeNull(), UNTIL_CEILING);
    expect(callMock).toHaveBeenCalledWith(LOCAL, "md-doc", { text, as: "prose", refs: [] }, expect.anything());
  });

  it("收着时那一行照抄后端写好的 `preview`，界面不自己截", () => {
    const res = renderMessage({ agent: "claude", t: "reply", id: "u3", at: "2026-01-01T00:00:00Z", blocks: [{ type: "thinking", text: "第一行\n第二行", preview: "后端写的那一行…" }], autoReply: false } as never, ctx());
    const d = (res as { units: HTMLElement[] }).units[0] as HTMLDetailsElement;
    expect(d.textContent).toContain("后端写的那一行…");
    expect(d.textContent).not.toContain("第一行");
  });

  it("问没成 ⇒ 照原文的字", async () => {
    callMock.mockRejectedValue(new Error("断了"));
    const res = renderMessage({ agent: "claude", t: "reply", id: "u2", at: "2026-01-01T00:00:00Z", blocks: [{ type: "thinking", text: "原文 **字**", preview: "原文 **字**" }], autoReply: false } as never, ctx());
    const d = (res as { units: HTMLElement[] }).units[0] as HTMLDetailsElement;
    d.open = true;
    d.dispatchEvent(new Event("toggle"));
    await vi.waitFor(() => expect(d.textContent).toContain("原文 **字**"), UNTIL_CEILING);
  });
});

describe("依赖棘轮：界面没有 Markdown 解析器、没有公式排版器", () => {
  it("package.json 里没有 marked · marked-katex-extension · highlight.js · dompurify · katex，src/frontend/ui 里没有一处引它们", () => {
    const pkg = JSON.parse(readFileSync(join(ROOT, "package.json"), "utf8")) as { dependencies?: object; devDependencies?: object };
    const deps = Object.keys({ ...pkg.dependencies, ...pkg.devDependencies });
    const GONE = ["marked", "marked-katex-extension", "highlight.js", "dompurify", "katex"];
    expect(deps.filter((d) => GONE.includes(d))).toEqual([]);
    const USE = /(from|import)\s*\(?\s*["'](marked|marked-katex-extension|highlight\.js|dompurify|katex)[/"']/;
    const files = productionTsFiles("src/frontend");
    expect(files.length, "一份都没读到 —— 本条在空转").toBeGreaterThan(50);
    expect(files.filter((f) => USE.test(f.text)).map((f) => f.file)).toEqual([]);
  });
});

/**
 * IO 按滚动容器分（root 必填）。单元这一半：同 root 复用、不同 root 各一个、
 * 回调里补算并 unobserve、`releaseEnhanceRoot` 断开且之后再 observe 会新建。真渲染管线上的那一半在
 * `tests/frontend/ui/views/session-viewer-scroll.vitest.ts`「D2」。
 */
describe("D2 · 每个滚动容器一个 IO", () => {
  class FakeIO {
    static all: FakeIO[] = [];
    readonly observed = new Set<Element>();
    disconnected = false;
    constructor(
      readonly cb: IntersectionObserverCallback,
      readonly opts: IntersectionObserverInit = {},
    ) {
      FakeIO.all.push(this);
    }
    observe(el: Element): void {
      this.observed.add(el);
    }
    unobserve(el: Element): void {
      this.observed.delete(el);
    }
    disconnect(): void {
      this.disconnected = true;
    }
    fire(el: Element): void {
      this.cb([{ isIntersecting: true, target: el } as unknown as IntersectionObserverEntry], this as never);
    }
  }
  /** 一张卡：一块正文树 ＋（`img` 真）一张惰路建的、等着进视口才取的图。 */
  const mk = (img = true): HTMLElement => {
    const el = document.createElement("div");
    el.append(mountWhole(doc("标题"), true));
    if (img) {
      const ph = document.createElement("img");
      ph.dataset.imagePending = "";
      el.append(ph);
    }
    return el;
  };

  it("同 root 复用、不同 root 各一个；进视口即补算并 unobserve；release 之后新建", async () => {
    FakeIO.all = [];
    vi.stubGlobal("IntersectionObserver", FakeIO);
    try {
      const { observeForEnhance, releaseEnhanceRoot } = await import("../../../src/frontend/ui/render");
      const A = document.createElement("div");
      const B = document.createElement("div");
      const [a1, a2, b1] = [mk(), mk(), mk()];
      observeForEnhance(a1, A);
      observeForEnhance(a2, A);
      observeForEnhance(b1, B);
      expect(FakeIO.all.map((io) => io.opts.root)).toEqual([A, B]);
      const [ioA, ioB] = FakeIO.all;
      expect([...ioA.observed]).toEqual([a1, a2]);
      expect([...ioB.observed]).toEqual([b1]);
      ioA.fire(a1);
      expect(a1.querySelector("[data-image-pending]")).toBeNull();
      expect(a2.querySelector("[data-image-pending]")).not.toBeNull();
      expect([...ioA.observed]).toEqual([a2]);
      releaseEnhanceRoot(A);
      expect(ioA.disconnected).toBe(true);
      observeForEnhance(mk(), A);
      expect(FakeIO.all.length).toBe(3);
      expect(FakeIO.all[2].opts.root).toBe(A);
    } finally {
      vi.unstubAllGlobals();
    }
  });

  it("里面没有等着取的图的卡不交给 IO（个个挂着 ＝ 滚动时每一帧都替它们算一遍交叉：300 轮的子运行 2495 个目标 → 299 个）", async () => {
    FakeIO.all = [];
    vi.stubGlobal("IntersectionObserver", FakeIO);
    try {
      const { observeForEnhance } = await import("../../../src/frontend/ui/render");
      const root = document.createElement("div");
      const plain = mk(false);
      const pic = mk();
      const math = document.createElement("div");
      math.append(mountWhole(doc("\\[ 块公式"), true));
      observeForEnhance(plain, root);
      observeForEnhance(pic, root);
      observeForEnhance(math, root);
      const io = FakeIO.all.find((x) => x.opts.root === root)!;
      expect([...io.observed], "公式当场画好，不等进视口").toEqual([pic]);
    } finally {
      vi.unstubAllGlobals();
    }
  });
});
