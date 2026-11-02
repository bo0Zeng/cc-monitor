/**
 * 回复正文卡照后端交的树（`Block::Prose`）排：树取自 md-core 的金样（`tests/__fixtures__/md-core.golden.json`），
 * 卡里的链接 · 代码块 · 表格 · 任务框 · 公式各在其位，尖括号词照字出现。
 * 长正文照树的 `segs` 分段：建卡只排第 0 段 ＋ 一颗「显示全部」，点了余下的一段一跳地补（不卡输入）。
 * 长正文的语料是合成的（段落 ＋ 代码块），不含任何真会话正文。
 */
import { describe, it, expect, vi, afterEach } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { REPO_ROOT } from "../../../test-support/repo-root.ts";
import { LOCAL_ORIGIN } from "../../../../src/frontend/ui/ipc/origin";
import type { LineRecord } from "../../../../src/frontend/ui/generated/LineRecord";
import type { Doc } from "../../../../src/frontend/ui/generated/Doc";
import type { MdBlock } from "../../../../src/frontend/ui/generated/MdBlock";
import { renderMessage } from "../../../../src/frontend/ui/cards/index";
import { copyText } from "../../../../src/frontend/ui/copy-table";
import { sizeText } from "../../../../src/frontend/ui/format";

interface Case {
  name: string;
  doc: Doc;
}
const CASES: Case[] = JSON.parse(readFileSync(resolve(REPO_ROOT, "tests/__fixtures__/md-core.golden.json"), "utf8"));
const golden = (name: string): MdBlock[] => {
  const c = CASES.find((x) => x.name === name);
  if (!c) throw new Error(`金样里没有「${name}」`);
  return c.doc.blocks;
};

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

const ctx = (lazy = false) => ({
  parentPath: "/p/s.jsonl",
  origin: LOCAL_ORIGIN,
  toolUseNames: new Map(),
  toolUseElements: new Map(),
  pendingToolResults: new Map(),
  lazy,
});
const reply = (doc: Doc, bytes = 0, extra: object = {}): LineRecord =>
  ({
    agent: "claude",
    t: "reply",
    id: "a",
    at: "2026-01-01T00:00:00.000Z",
    blocks: [{ type: "prose", doc, bytes }],
    autoReply: false,
    draw: "card",
    human: false,
    ...extra,
  }) as never;
const cardOf = (rec: LineRecord, lazy = false): HTMLElement => {
  const r = renderMessage(rec, ctx(lazy));
  if (r.kind !== "card") throw new Error(r.kind);
  return r.element;
};

/** 金样里几例拼成一棵树：链接 · 代码块 · 表格 · 任务框 · 公式 · 尖括号词。 */
const MIXED: Doc = {
  blocks: [
    ...golden("链接 scheme"),
    ...golden("bash 代码块"),
    ...golden("表格里的转义竖线"),
    ...golden("任务列表"),
    ...golden("行内公式两种写法"),
    ...golden("尖括号词"),
  ],
  segs: [],
};

describe("正文卡读树", () => {
  it("★ 链接 · 代码块 · 表格 · 任务框 · 公式占位各在其位；尖括号词照字出现、不成元素", () => {
    const el = cardOf(reply(MIXED), true);
    const body = el.querySelector(".block-text")!;
    expect(body, "正文块").not.toBeNull();
    const a = body.querySelector<HTMLAnchorElement>("a[data-external]");
    expect(a?.target).toBe("_blank");
    const code = body.querySelector<HTMLElement>(".code-block[data-lang=bash] code");
    expect(code?.textContent).toBe("ls -la ~/a");
    expect(body.querySelector(".code-block [data-copy]")).not.toBeNull();
    expect([...body.querySelectorAll<HTMLElement>("table th")].map((t) => t.dataset.align)).toEqual(["left", "right"]);
    expect(body.querySelector("table td code")?.textContent).toBe("a | b");
    expect([...body.querySelectorAll<HTMLInputElement>("input[type=checkbox]")].map((c) => c.checked)).toEqual([false, true]);
    expect(body.querySelectorAll("[data-math-pending]").length, "惰路：公式留占位").toBeGreaterThan(0);
    expect(body.textContent).toContain("<Composer>");
    expect(body.querySelector("composer")).toBeNull();
  });

  it("急路（实时）：公式当场排好，不留占位", () => {
    const el = cardOf(reply(MIXED), false);
    expect(el.querySelectorAll("[data-math-pending]").length).toBe(0);
    expect(el.querySelector(".katex")).not.toBeNull();
  });

  it("空树的正文块不摆", () => {
    const el = cardOf(reply({ blocks: [], segs: [] }));
    expect(el.querySelector(".block-text")).toBeNull();
  });

  it("报错卡的原文照树取字（行内的字连起来、块之间换行）", () => {
    const doc: Doc = {
      blocks: [
        { t: "p", k: ["上游 ", { t: "b", k: ["过载"] }, "，", { t: "c", x: "529" }] },
        { t: "code", lang: null, x: "retry later", cont: false },
      ],
      segs: [],
    };
    const el = cardOf(reply(doc, 0, { error: { reason: "overloaded" } }));
    expect(el.querySelector(".api-error-body")?.textContent).toBe("上游 过载，529\nretry later");
  });
});

describe("长正文分段", () => {
  /** 合成：`n` 段，每 `per` 块一段（段落与代码块交替）。 */
  function long(n: number, per: number): Doc {
    const blocks: MdBlock[] = [];
    for (let i = 0; i < n * per; i++) {
      blocks.push(i % 3 === 2 ? { t: "code", lang: "ts", x: `const x${i} = 1;`, cont: false } : { t: "p", k: [`段落 ${i} ` + "字".repeat(30)] });
    }
    return { blocks, segs: Array.from({ length: n - 1 }, (_, i) => (i + 1) * per) };
  }
  const count = (el: HTMLElement): number => el.querySelectorAll(".block-text p, .block-text .code-block").length;

  it("★ 建卡只排第 0 段；点「显示全部」之后一段一跳地补（点的那一下同步零段），补完按钮收掉、块一个不少", async () => {
    vi.stubGlobal("MessageChannel", undefined); // 让出走 `setTimeout` 那一支，好用假定时器一跳一跳地走
    vi.useFakeTimers();
    const doc = long(4, 9);
    const el = cardOf(reply(doc, 123_456));
    expect(count(el)).toBe(9);
    const btn = el.querySelector<HTMLButtonElement>(".block-body-show-full")!;
    expect(btn.textContent, "按钮上的大小是后端交的整块字节数").toBe(copyText("cards.text.showAll", { size: sizeText(123_456) }));
    btn.click();
    expect(count(el), "点的那一下就同步排了").toBe(9);
    for (let i = 2; i <= 4; i++) {
      await vi.advanceTimersToNextTimerAsync();
      expect(count(el), `第 ${i - 1} 跳`).toBe(9 * i);
    }
    expect(el.querySelector(".block-body-show-full")).toBeNull();
    expect(count(el)).toBe(doc.blocks.length);
  });

  it("只有一段：整棵一次排完，没有按钮", () => {
    const el = cardOf(reply(long(1, 12)));
    expect(count(el)).toBe(12);
    expect(el.querySelector(".block-body-show-full")).toBeNull();
  });

  it("★ 卡头那一家的名字是宿主交进来的（会话是哪一家由后端说），不按会话文件名猜；还不知道 ⇒ 先空着、按那一格补", async () => {
    const { SPEAKER_SELECTOR } = await import("../../../../src/frontend/ui/cards/speaker");
    const head = (speaker: string | null, parentPath: string): string => {
      const rec = reply({ blocks: [{ t: "p", k: ["好"] }], segs: [] });
      const el = (renderMessage(rec, { ...ctx(), parentPath, speaker }) as { element: HTMLElement }).element;
      const wrap = document.createElement("div");
      wrap.appendChild(el);
      return wrap.querySelector<HTMLElement>(SPEAKER_SELECTOR)?.textContent ?? "<没找到那一格>";
    };
    expect(head("Codex", "/p/s.jsonl")).toBe("Codex");
    expect(head("Claude", "/x/rollout-2026-01-01.jsonl"), "文件名像 Codex 也照宿主说的").toBe("Claude");
    expect(head(null, "/p/s.jsonl"), "还不知道是哪一家 ⇒ 不顶替成哪一家").toBe("");
  });
});
