/**
 * 正文排版器（`src/frontend/ui/md-view.ts`）：树取自 md-core 的金样（`tests/__fixtures__/md-core.golden.json`，
 * 后端 `md_core::parse` 与高亮逐例对过的那一份），这里只判「照树排出来的 DOM 对不对」。
 */
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { REPO_ROOT } from "../../test-support/repo-root.ts";
import { buildBlocks, mountDoc, paint, segRange, type DocView } from "../../../src/frontend/ui/md-view";
import { enhanceCard } from "../../../src/frontend/ui/render";
import { copyText } from "../../../src/frontend/ui/copy-table";
import s from "../../../src/frontend/ui/md-view.module.css";
import type { Doc } from "../../../src/frontend/ui/generated/Doc";
import type { MdBlock } from "../../../src/frontend/ui/generated/MdBlock";
import type { HlCode } from "../../../src/frontend/ui/generated/HlCode";

interface Case {
  name: string;
  md: string;
  doc: Doc;
  hl?: number[][];
}

const CASES: Case[] = JSON.parse(readFileSync(resolve(REPO_ROOT, "tests/__fixtures__/md-core.golden.json"), "utf8"));

function golden(name: string): Case {
  const c = CASES.find((x) => x.name === name);
  if (!c) throw new Error(`金样里没有「${name}」`);
  return c;
}

/** 只出字（不拉高亮）。 */
function view(name: string): HTMLElement {
  return mountDoc(golden(name).doc).root;
}

/** 拉高亮那一下交上来的，一段一份。 */
type Asked = HlCode[][];

/** 建卡、拉高亮（`hlOf` 照交上来的那几块代码回 `hl`）、等它落完。 */
async function painted(doc: Doc, hlOf: (codes: readonly HlCode[]) => number[][], asked: Asked = []): Promise<DocView> {
  const v = mountDoc(doc, async (codes) => {
    asked.push([...codes]);
    return { hl: hlOf(codes) };
  });
  await settle();
  return v;
}

/** 等拉高亮的那几个 promise 落完。 */
function settle(): Promise<void> {
  return new Promise((r) => setTimeout(r, 0));
}

/** 金样里那一例连它的高亮。 */
function paintedGolden(name: string): Promise<DocView> {
  const c = golden(name);
  return painted(c.doc, () => c.hl ?? []);
}

/** 排版器许出的标签：正文里写的任何标签都只能是字。 */
const TAGS = new Set([
  "DIV", "P", "H1", "H2", "H3", "H4", "H5", "H6", "STRONG", "EM", "DEL", "CODE", "PRE", "SPAN", "A", "IMG", "BR", "SUP",
  "UL", "OL", "LI", "INPUT", "BLOCKQUOTE", "TABLE", "THEAD", "TBODY", "TR", "TH", "TD", "HR", "SECTION", "BUTTON",
]);

describe("md-view：金样逐例", () => {
  it("金样有货（防读空了全绿）", () => {
    expect(CASES.length).toBeGreaterThanOrEqual(20);
  });

  for (const c of CASES) {
    it(`排得出、标签都在许出的那几种里：${c.name}`, async () => {
      const root = (await paintedGolden(c.name)).root;
      for (const e of root.querySelectorAll("*")) expect(TAGS, `${c.name} 出了 <${e.tagName}>`).toContain(e.tagName);
    });
  }

  it("尖括号词与 details 照字出现，不成元素", () => {
    const r = view("尖括号词");
    expect(r.textContent).toBe("把 <Composer> 和 Vec<String> 换掉");
    expect(r.querySelector("composer")).toBeNull();
    const d = view("details 块当字");
    expect(d.querySelector("details, summary")).toBeNull();
    expect([...d.querySelectorAll("p")].map((p) => p.textContent)).toEqual(["<details><summary>点开</summary>", "里面的字", "</details>"]);
    const k = view("br 与 kbd");
    expect(k.querySelector("kbd")).toBeNull();
    expect(k.querySelectorAll("br")).toHaveLength(1);
  });

  it("链接：放行的 scheme 成链接、交系统浏览器开；没放行的只剩字", () => {
    const as = [...view("裸网址与 www").querySelectorAll("a")];
    expect(as.map((a) => a.getAttribute("href"))).toEqual(["https://example.com/x", "http://www.example.org"]);
    for (const a of as) {
      expect(a.hasAttribute("data-external")).toBe(true);
      expect(a.target).toBe("_blank");
      expect(a.rel).toBe("noopener noreferrer");
    }
    const m = view("链接 scheme");
    expect([...m.querySelectorAll("a")].map((a) => a.getAttribute("href"))).toEqual(["mailto:a@example.com"]);
    expect(m.textContent).toBe("信 与 文");
  });

  it("代码块：照三元组打类，字一个不丢；外壳是带复制钮的 code-block", async () => {
    const r = (await paintedGolden("bash 代码块")).root;
    const code = r.querySelector(`pre > code.${s.mdCode}`);
    expect(code?.textContent).toBe("ls -la ~/a");
    // hl [0,2,4, 0,4,7, 1,1,7]：「ls」函数 ·「 -la」变量 · 空一格 ·「~」变量
    expect([...code!.querySelectorAll("span")].map((x) => [x.textContent, x.className])).toEqual([
      ["ls", s.hlFunction],
      [" -la", s.hlVariable],
      ["~", s.hlVariable],
    ]);
    const blk = r.querySelector(".code-block");
    expect(blk?.querySelector(".code-bar .code-lang")?.textContent).toBe("bash");
    const btn = blk?.querySelector("button.code-copy");
    expect(btn?.hasAttribute("data-copy")).toBe(true);
    expect(btn?.textContent).toBe(copyText("render.codeBlock.copy"));
  });

  it("代码块：没拉到高亮 ⇒ 整块纯字；拉的那一下失败 ⇒ 照旧纯字；三元组越界 ⇒ 截在串尾", async () => {
    const c = golden("~~~ 围栏");
    const plain = mountDoc(c.doc).root.querySelector("code")!;
    expect(plain.children).toHaveLength(0);
    expect(plain.textContent).toBe("fn main() {}");
    const failed = mountDoc(c.doc, () => Promise.reject(new Error("断了")));
    await settle();
    expect(failed.root.querySelector("code")!.children).toHaveLength(0);
    expect(failed.root.querySelector("code")!.textContent).toBe("fn main() {}");
    const over = (await painted(c.doc, () => [[0, 2, 0, 5, 99, 1]])).root.querySelector("code")!;
    expect(over.textContent).toBe("fn main() {}");
    expect([...over.querySelectorAll("span")].map((x) => x.textContent)).toEqual(["fn", "() {}"]);
  });

  it("十三个高亮类各对一个样式类，互不相同", async () => {
    const x = "abcdefghijklm";
    const hl = [...x].flatMap((_, i) => [0, 1, i]);
    const doc: Doc = { blocks: [{ t: "code", lang: "x", x, cont: false }], segs: [] };
    const spans = [...(await painted(doc, () => [hl])).root.querySelectorAll("code span")];
    const names = spans.map((e) => e.className);
    expect(names).toEqual([
      s.hlKeyword, s.hlString, s.hlComment, s.hlNumber, s.hlFunction, s.hlType, s.hlConstant,
      s.hlVariable, s.hlAttribute, s.hlTag, s.hlInserted, s.hlDeleted, s.hlMeta,
    ]);
    expect(new Set(names).size).toBe(13);
  });

  it("表格：表头 · 行 · 对齐各在其位，格里的转义竖线照字", () => {
    const t = view("表格里的转义竖线").querySelector("table")!;
    expect([...t.querySelectorAll("thead th")].map((h) => [h.textContent, h.getAttribute("data-align")])).toEqual([
      ["名", "left"],
      ["值", "right"],
    ]);
    const tds = [...t.querySelectorAll("tbody td")];
    expect(tds[0].querySelector("code")?.textContent).toBe("a | b");
    expect(tds[1].getAttribute("data-align")).toBe("right");
  });

  it("任务列表：两个只读框，第二个勾上", () => {
    const boxes = [...view("任务列表").querySelectorAll<HTMLInputElement>(`li.${s.mdTask} > input[type=checkbox]`)];
    expect(boxes.map((b) => [b.checked, b.disabled])).toEqual([
      [false, true],
      [true, true],
    ]);
  });

  it("公式：行内与块级各出占位，交 enhanceCard 排成 KaTeX", () => {
    const r = view("行内公式两种写法");
    expect([...r.querySelectorAll("[data-math-pending]")].map((e) => [e.getAttribute("data-math-pending"), e.textContent])).toEqual([
      ["inline", "E=mc^2"],
      ["inline", "a+b"],
    ]);
    const b = view("\\[ 块公式");
    expect(b.querySelector("[data-math-pending=display]")?.textContent).toBe("\\sum_i x_i");
    enhanceCard(b);
    expect(b.querySelector("[data-math-pending]")).toBeNull();
    expect(b.querySelector(".katex-display")).not.toBeNull();
  });

  it("钱数不是公式：没有占位", () => {
    expect(view("钱数不是公式").querySelector("[data-math-pending]")).toBeNull();
  });

  it("脚注 · 提示块 · 引用 · 分隔线 · 标题 · 起号", () => {
    const f = view("脚注");
    expect(f.querySelector(`sup.${s.mdFnRef}`)?.textContent).toBe("1");
    const li = f.querySelector<HTMLLIElement>(`section.${s.mdNotes} ol > li`);
    expect([li?.value, li?.textContent]).toEqual([1, "注一"]);
    const al = view("提示块").querySelector(`blockquote.${s.mdAlert}`);
    expect([al?.getAttribute("data-kind"), al?.textContent]).toEqual(["warning", "小心"]);
    const q = view("引用与分隔线");
    expect(q.querySelector("blockquote")?.className).toBe("");
    expect(q.querySelector("blockquote + hr")).not.toBeNull();
    const h = view("标题").querySelector("h2");
    expect(h?.querySelector("strong")?.textContent).toBe("标题");
    expect(view("有序列表起号").querySelector("ol")?.getAttribute("start")).toBe("3");
    const sub = view("缩进不够的子项");
    expect(sub.querySelector("ol")?.hasAttribute("start")).toBe(false);
    expect(sub.querySelector("ul")).not.toBeNull();
  });

  it("删除线只认树里的 del、单波浪号照字", () => {
    const r = view("单波浪号与删除线");
    expect(r.querySelector("del")?.textContent).toBe("删");
    expect(r.textContent).toBe("~/a~/b 与 删");
  });
});

describe("md-view：分段", () => {
  const p = (x: string): MdBlock => ({ t: "p", k: [x] });
  const code = (x: string): MdBlock => ({ t: "code", lang: "x", x, cont: false });
  const doc: Doc = { blocks: [p("一"), code("ab"), p("三"), { t: "quote", blocks: [code("cd")] }], segs: [2, 3] };

  it("segRange 照 segs 切：[0,2) [2,3) [3,4)", () => {
    expect([0, 1, 2].map((i) => segRange(doc, i))).toEqual([
      [0, 2],
      [2, 3],
      [3, 4],
    ]);
    expect(segRange({ blocks: [p("x")], segs: [] }, 0)).toEqual([0, 1]);
  });

  it("只建第 0 段；buildNext 一段段接上，建完回 false；每段只拉本段的代码块、高亮从那一段的第 0 块数起", async () => {
    const asked: Asked = [];
    // 「ab」那一块第 0 字关键字；「cd」那一块第 1 字字符串。
    const v = await painted(doc, (codes) => codes.map((c) => (c.x === "ab" ? [0, 1, 0] : [1, 1, 1])), asked);
    expect([v.segCount, v.built()]).toEqual([3, 1]);
    expect(v.root.children).toHaveLength(2);
    expect(v.root.querySelector(`.${s.hlKeyword}`)?.textContent).toBe("a");
    expect(v.buildNext()).toBe(true);
    expect(v.root.children).toHaveLength(3);
    expect(v.buildNext()).toBe(true);
    await settle();
    expect(v.root.querySelector(`blockquote .${s.hlString}`)?.textContent).toBe("d");
    expect(v.built()).toBe(3);
    expect(v.buildNext()).toBe(false);
    expect(v.root.children).toHaveLength(4);
    // 中间那一段没有代码块 ⇒ 不拉。
    expect(asked).toEqual([[{ lang: "x", x: "ab" }], [{ lang: "x", x: "cd" }]]);
  });

  it("先出字、后上色：拉到之前代码照纯字排好、字一个不少", () => {
    const v = mountDoc(doc, () => new Promise(() => {}));
    const code0 = v.root.querySelector("code")!;
    expect(code0.textContent).toBe("ab");
    expect(code0.children).toHaveLength(0);
  });

  it("一棵带两段代码的树建卡 ⇒ 拉高亮之后两段的代码元素都带高亮类", async () => {
    const two: Doc = { blocks: [code("let a"), code("let b")], segs: [1] };
    const v = await painted(two, (codes) => codes.map(() => [0, 3, 0]));
    v.buildNext();
    await settle();
    const codes = [...v.root.querySelectorAll(`code.${s.mdCode}`)];
    expect(codes).toHaveLength(2);
    for (const c of codes) expect(c.querySelector(`span.${s.hlKeyword}`)?.textContent).toBe("let");
  });

  it("buildBlocks ＋ paint：嵌在列表与引用里的代码块也按先序取高亮", () => {
    const blocks: MdBlock[] = [
      { t: "list", start: null, tight: true, items: [{ task: null, blocks: [code("xy")] }] },
      code("zw"),
    ];
    const host = document.createElement("div");
    const built = buildBlocks(blocks);
    expect(built.codes.map((c) => c.x)).toEqual(["xy", "zw"]);
    host.append(built.frag);
    paint(built.codes, { hl: [[1, 1, 2], [0, 1, 3]] });
    expect([...host.querySelectorAll("code span")].map((e) => [e.textContent, e.className])).toEqual([
      ["y", s.hlComment],
      ["z", s.hlNumber],
    ]);
  });
});

describe("md-view：源码", () => {
  it("排版器里没有 innerHTML / insertAdjacentHTML（字只经文本节点落）", () => {
    const src = readFileSync(resolve(REPO_ROOT, "src/frontend/ui/md-view.ts"), "utf8");
    expect(src).not.toMatch(/innerHTML|outerHTML|insertAdjacentHTML/);
  });
});
