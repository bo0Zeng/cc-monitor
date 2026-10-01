/**
 * 超长回复不许同步排整段 markdown（617 KB 一条 ≈ 8 s 卡主线程，文本布局补审）。
 * 要求住址：裁定原话「assistant text 块超过阈值只渲染前一截 ＋ 一颗『显示全部』按钮（点了再渲染余下，分片让出主线程，不卡输入）」。
 * 语料是合成的（结构：段落 ＋ 围栏代码块），不含任何真会话正文。
 */
import { describe, it, expect, vi, afterEach } from "vitest";
import { LOCAL_ORIGIN } from "../../../../src/frontend/ui/ipc/origin";
import type { JsonlRecord } from "../../../../src/frontend/ui/generated/JsonlRecord";

const rendered: number[] = [];
vi.mock("../../../../src/frontend/ui/render", async (orig) => {
  const real = await orig<typeof import("../../../../src/frontend/ui/render")>();
  return {
    ...real,
    renderMarkdown: (md: string, opts?: { lazy?: boolean }) => {
      rendered.push(md.length);
      return real.renderMarkdown(md, opts);
    },
  };
});

import { LONG_REPLY_HEAD_CHARS, markdownPieces, renderMessage } from "../../../../src/frontend/ui/cards/index";

const fenceCount = (s: string): number => s.split("\n").filter((l) => /^\s*(```|~~~)/.test(l)).length;

/** 合成语料：段落与代码块交替，总长约 `n` 字。 */
function corpus(n: number): string {
  const parts: string[] = [];
  let len = 0;
  for (let i = 0; len < n; i++) {
    // 代码块里夹空行：切片只许在围栏之外的空行上切
    const p = i % 3 === 2 ? "```ts\n" + "const x = 1;\n\n".repeat(20) + "```" : `段落 ${i} ` + "字".repeat(300);
    parts.push(p);
    len += p.length + 2;
  }
  return parts.join("\n\n");
}

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
  rendered.length = 0;
});

describe("长回复切片", () => {
  it("没有超长单块时：各片接回去 == 原文；每片 ≤ 阈值；代码块不被劈开（每片围栏成对）", () => {
    const md = corpus(90_000);
    const pieces = markdownPieces(md, LONG_REPLY_HEAD_CHARS);
    expect(pieces.join("\n")).toBe(md);
    for (const p of pieces) {
      expect(p.length).toBeLessThanOrEqual(LONG_REPLY_HEAD_CHARS);
      expect(fenceCount(p) % 2, p.slice(0, 40)).toBe(0);
    }
    // 跨阈值的那个代码块整块归下一片（代码块里的空行不是切点）
    const prose = "字".repeat(LONG_REPLY_HEAD_CHARS - 1000);
    const code = "```\n" + "x\n\n".repeat(1000) + "```";
    expect(markdownPieces(`${prose}\n\n${code}`, LONG_REPLY_HEAD_CHARS)).toEqual([`${prose}\n`, code]);
  });

  it("单个代码块比阈值还长：切成几截、每截各自成对围栏，代码行一行不少", () => {
    const code = "```py\n" + Array.from({ length: 5000 }, (_, i) => `line_${i} = ${i}`).join("\n") + "\n```";
    const pieces = markdownPieces(code, LONG_REPLY_HEAD_CHARS);
    expect(pieces.length).toBeGreaterThan(1);
    const lines: string[] = [];
    for (const p of pieces) {
      expect(p.length).toBeLessThanOrEqual(LONG_REPLY_HEAD_CHARS);
      const ls = p.split("\n");
      expect([ls[0], ls[ls.length - 1]]).toEqual(["```py", "```"]);
      lines.push(...ls.slice(1, -1));
    }
    expect(lines).toEqual(Array.from({ length: 5000 }, (_, i) => `line_${i} = ${i}`));
  });
});

describe("长回复建卡", () => {
  const ctx = () => ({
    parentPath: "/p/s.jsonl",
    origin: LOCAL_ORIGIN,
    toolUseNames: new Map(),
    toolUseElements: new Map(),
    pendingToolResults: new Map(),
    lazy: false,
  });
  const asst = (text: string): JsonlRecord =>
    ({
      type: "assistant",
      uuid: "a",
      timestamp: "2026-01-01T00:00:00.000Z",
      message: { role: "assistant", content: [{ type: "text", text }] },
    }) as never;

  it("建卡时只同步排第一片；点「显示全部」之后一片一跳地补（点的那一下同步零片），补完按钮收掉、正文一字不少", async () => {
    vi.stubGlobal("MessageChannel", undefined); // 让出走 `setTimeout` 那一支，好用假定时器一跳一跳地走
    vi.useFakeTimers();
    const md = corpus(90_000);
    const pieces = markdownPieces(md, LONG_REPLY_HEAD_CHARS);
    const res = renderMessage(asst(md), ctx());
    expect(res.kind).toBe("card");
    const el = (res as { element: HTMLElement }).element;
    expect(rendered).toEqual([pieces[0].length]);
    const btn = el.querySelector<HTMLButtonElement>(".block-body-show-full")!;
    expect(btn).not.toBeNull();
    rendered.length = 0;
    btn.click();
    expect(rendered, "点的那一下就同步排了").toEqual([]);
    for (let i = 1; i < pieces.length; i++) {
      await vi.advanceTimersToNextTimerAsync();
      expect(rendered.length, `第 ${i} 跳`).toBe(i);
    }
    expect(rendered).toEqual(pieces.slice(1).map((p) => p.length));
    expect(el.querySelector(".block-body-show-full")).toBeNull();
  });

  it("阈值以内：整段一次排完，没有按钮", () => {
    const md = corpus(LONG_REPLY_HEAD_CHARS - 1000);
    const el = (renderMessage(asst(md), ctx()) as { element: HTMLElement }).element;
    expect(rendered).toEqual([md.length]);
    expect(el.querySelector(".block-body-show-full")).toBeNull();
  });
});
