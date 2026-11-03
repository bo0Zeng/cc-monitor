/**
 * 会话消息里的编号带标题：界面只排后端给的，不认编号、不拼标签。
 * - 回复正文：送后端交过的树，回来的是标好引用节点的树（`ref`：编号原样 ＋ 标题）⇒ 照它重排那一块，编号后面跟一枚淡色标题；
 * - 人说的话：送原文，回来的是几处 `{start, end, ids, label}` ⇒ 照原文排，编号后面跟标题。
 * 回空 ⇒ 正文逐字原样；点标题 ⇒ 开计划页那一格；同一帧几块 ⇒ 只问一次。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { UNTIL_CEILING } from "../../test-support/wall-ceiling";

vi.mock("../../../src/frontend/ui/plan-reads", () => ({ fetchMsgRefs: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ emit: vi.fn(() => Promise.resolve()), listen: vi.fn(() => Promise.resolve(() => {})) }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn().mockResolvedValue(undefined) }));

import { emit } from "@tauri-apps/api/event";
import { fetchMsgRefs } from "../../../src/frontend/ui/plan-reads";
import { askRefs, __refsSettledForTests } from "../../../src/frontend/ui/plan-refs";
import { PLAN_OPEN_EVENT } from "../../../src/frontend/ui/window-events";
import type { Doc } from "../../../src/frontend/ui/generated/Doc";
import type { Inl } from "../../../src/frontend/ui/generated/Inl";
import type { PlanMsgRefs } from "../../../src/frontend/ui/generated/PlanMsgRefs";
import { renderMessage, type LineRecord, type RenderContext } from "../../../src/frontend/ui/cards";

const fetchMock = fetchMsgRefs as unknown as ReturnType<typeof vi.fn>;
const emitMock = emit as unknown as ReturnType<typeof vi.fn>;

const ctx = (): RenderContext => ({
  parentPath: "/p/s.jsonl",
  origin: "devbox",
  toolUseNames: new Map(),
  toolUseElements: new Map(),
  pendingToolResults: new Map(),
});

/** 一段的树。 */
const para = (...k: Inl[]): Doc => ({ blocks: [{ t: "p", k }], segs: [] });
/** 引用节点（后端插好的那一种）。 */
const ref = (x: string, label: string, ids: string[] = [x]): Inl => ({ t: "ref", x, ids, label });

/** 一张回复卡：几块回复正文（像后端那样交树）。 */
function card(...docs: Doc[]): HTMLElement {
  const rec = {
    agent: "claude",
    t: "reply",
    id: "a",
    at: "2026-01-01T00:00:00.000Z",
    blocks: docs.map((doc) => ({ type: "prose", doc, more: "" })),
    autoReply: false,
    draw: "card",
    human: false,
  };
  const r = renderMessage(rec as never as LineRecord, ctx());
  if (r.kind !== "card") throw new Error(r.kind);
  return r.element;
}

/** 看得见的字（段落之间的换行不算）。 */
function words(el: Element): string {
  return (el.textContent ?? "").replace(/\n/g, "");
}

/** 卡里各块回复正文看得见的字。 */
function blockWords(c: Element): string[] {
  return [...c.querySelectorAll(".block-text")].map(words);
}

/** UTF-16 下标（后端换好的那一种）：这一段在原文里第几次出现的起止。 */
function at(text: string, piece: string, nth = 0): { start: number; end: number } {
  let i = -1;
  for (let k = 0; k <= nth; k++) i = text.indexOf(piece, i + 1);
  return { start: i, end: i + piece.length };
}

beforeEach(() => {
  fetchMock.mockReset();
  emitMock.mockClear();
});

describe("回复正文：后端回标好引用的树，照它重排", () => {
  it("★ 两处 ⇒ 编号原样，各跟一个标题（粗体里的那一处照旧粗）", async () => {
    const doc = para("😀 先看 ", { t: "b", k: ["A1-1"] }, "，再看 A1-2/1 的写法。");
    const marked = para("😀 先看 ", { t: "b", k: [ref("A1-1", "甲的读入")] }, "，再看 ", ref("A1-2/1", "甲的写出、甲的读入", ["A1-2", "A1-1"]), " 的写法。");
    fetchMock.mockResolvedValueOnce({ workspace: "/w", slice: "alpha", refs: [], docs: [marked] } satisfies PlanMsgRefs);
    const c = card(doc);
    expect(blockWords(c)).toEqual(["😀 先看 A1-1，再看 A1-2/1 的写法。"]);
    askRefs(c, "devbox", "sid-1");
    await __refsSettledForTests();
    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(fetchMock.mock.calls[0]).toEqual(["devbox", "sid-1", [], [doc]]);
    const tails = [...c.querySelectorAll<HTMLButtonElement>(".block-text button")];
    expect(tails.map((b) => b.textContent)).toEqual(["甲的读入", "甲的写出、甲的读入"]);
    expect(c.querySelector(".block-text strong")?.textContent).toBe("A1-1甲的读入");
    expect(tails[1].previousSibling?.textContent).toBe("A1-2/1");
    expect(blockWords(c)).toEqual(["😀 先看 A1-1甲的读入，再看 A1-2/1甲的写出、甲的读入 的写法。"]);
  });

  it("回空（认不出片 · 一处都没有）⇒ 正文逐字原样", async () => {
    const doc = para("这里提到 A1 但图上没有，", { t: "c", x: "A2" }, " 在代码里。");
    const replies: PlanMsgRefs[] = [
      { workspace: null, slice: null, refs: [], docs: [null] },
      { workspace: "/w", slice: "alpha", refs: [], docs: [null] },
    ];
    for (const reply of replies) {
      fetchMock.mockResolvedValueOnce(reply);
      const c = card(doc);
      const before = c.innerHTML;
      askRefs(c, "devbox", "sid-1");
      await __refsSettledForTests();
      expect(c.innerHTML).toBe(before);
    }
  });

  it("点标题 ⇒ 开计划页、选中那一格（头一个编号）", async () => {
    fetchMock.mockResolvedValueOnce({
      workspace: "/w",
      slice: "alpha",
      refs: [],
      docs: [para("见 ", ref("A1-2/1", "甲的写出、甲的读入", ["A1-2", "A1-1"]), "。")],
    } satisfies PlanMsgRefs);
    const c = card(para("见 A1-2/1。"));
    askRefs(c, "devbox", "sid-1");
    await __refsSettledForTests();
    c.querySelector<HTMLButtonElement>(".block-text button")!.click();
    expect(emitMock).toHaveBeenCalledWith(PLAN_OPEN_EVENT, { origin: "devbox", workspace: "/w", slice: "alpha", id: "A1-2" });
  });

  it("长回复：后来点开的那几段里的引用也挂上标题", async () => {
    const doc: Doc = { blocks: [0, 1, 2].map((i) => ({ t: "p" as const, k: [`第${i}段 A${i}`] })), segs: [1, 2] };
    const marked: Doc = { blocks: [0, 1, 2].map((i) => ({ t: "p" as const, k: [`第${i}段 `, ref(`A${i}`, `标${i}`)] })), segs: [1, 2] };
    fetchMock.mockResolvedValueOnce({ workspace: "/w", slice: "alpha", refs: [], docs: [marked] } satisfies PlanMsgRefs);
    const c = card(doc);
    askRefs(c, "devbox", "sid-1");
    await __refsSettledForTests();
    const tails = () => [...c.querySelectorAll(".block-text button:not(.block-body-show-full)")].map((b) => b.textContent);
    expect(tails()).toEqual(["标0"]);
    c.querySelector<HTMLButtonElement>(".block-body-show-full")!.click();
    await vi.waitFor(() => expect(tails()).toEqual(["标0", "标1", "标2"]), UNTIL_CEILING);
  });

  it("同一帧几张卡 · 几块 ⇒ 只问一次，回来的各落各的那一块", async () => {
    const a = para("甲说 A1。");
    const b = para("乙说 A1-1。");
    const cc = para("丙什么都没说。");
    fetchMock.mockResolvedValueOnce({
      workspace: "/w",
      slice: "alpha",
      refs: [],
      docs: [para("甲说 ", ref("A1", "甲功能"), "。"), para("乙说 ", ref("A1-1", "甲的读入"), "。"), null],
    } satisfies PlanMsgRefs);
    const c1 = card(a, b);
    const c2 = card(cc);
    askRefs(c1, "devbox", "sid-1");
    askRefs(c2, "devbox", "sid-1");
    await __refsSettledForTests();
    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(fetchMock.mock.calls[0][3]).toEqual([a, b, cc]);
    expect(blockWords(c1)).toEqual(["甲说 A1甲功能。", "乙说 A1-1甲的读入。"]);
    expect(blockWords(c2)).toEqual(["丙什么都没说。"]);
  });

  it("没记过的块（思考 · 工具输出）不送；问没成 ⇒ 原样", async () => {
    const c = document.createElement("div");
    c.innerHTML = "<div class='block-text'>A1</div>";
    askRefs(c, "devbox", "sid-1");
    await __refsSettledForTests();
    expect(fetchMock).not.toHaveBeenCalled();

    fetchMock.mockRejectedValueOnce(new Error("断了"));
    const d = card(para("说 A1。"));
    const before = d.innerHTML;
    askRefs(d, "devbox", "sid-1");
    await __refsSettledForTests();
    expect(d.innerHTML).toBe(before);
  });
});

// 人说的话（用户气泡）：同一个口子 —— 与回复正文同一批送；气泡照原文排（不过 markdown），折起的粘贴块不送。
describe("用户气泡里的编号", () => {
  const said = (text: string, pasted: object[] = []): HTMLElement => {
    const rec = { agent: "claude", t: "said", id: "u", at: "2026-01-01T14:02:00.000Z", blocks: [{ type: "text", text }], who: { speaker: { kind: "human" }, text, pasted } };
    const r = renderMessage(rec as never as LineRecord, ctx());
    if (r.kind !== "card") throw new Error(r.kind);
    return r.element;
  };
  const span = (text: string, open: string, body: string, close: string, lines: number) => {
    const start = text.indexOf(open);
    const bodyStart = start + open.length;
    return { start, end: bodyStart + body.length + close.length, bodyStart, bodyEnd: bodyStart + body.length, lines };
  };

  it("气泡与回复同一批问一次；编号原样跟标题，星号照原文不变粗，点了开计划页", async () => {
    const text = "看 *A1-1* 再说\n第二行";
    const reply = para("回 A1。");
    fetchMock.mockResolvedValueOnce({
      workspace: "/w",
      slice: "alpha",
      refs: [[{ ...at(text, "A1-1"), ids: ["A1-1"], label: "甲的读入" }]],
      docs: [para("回 ", ref("A1", "甲功能"), "。")],
    } satisfies PlanMsgRefs);
    const u = said(text);
    const a = card(reply);
    askRefs(u, "devbox", "sid-1");
    askRefs(a, "devbox", "sid-1");
    await __refsSettledForTests();
    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(fetchMock.mock.calls[0].slice(2)).toEqual([[text], [reply]]);
    const body = u.querySelector(".card-body")!;
    expect(body.querySelector("em, strong")).toBeNull();
    expect(body.querySelectorAll("br").length).toBe(1);
    expect(body.textContent).toBe("看 *A1-1甲的读入* 再说第二行");
    body.querySelector("button")!.click();
    expect(emitMock).toHaveBeenCalledWith(PLAN_OPEN_EVENT, { origin: "devbox", workspace: "/w", slice: "alpha", id: "A1-1" });
    expect(blockWords(a)).toEqual(["回 A1甲功能。"]);
  });

  it("带粘贴块：露在外面的几段送，折起的那一块不送", async () => {
    const long = Array.from({ length: 13 }, (_, i) => `A9-${i}`).join("\n");
    const short = "见 A2";
    const text = `先 A1：<P1>${long}</P1>再<P2>${short}</P2>完`;
    const u = said(text, [span(text, "<P1>", long, "</P1>", 13), span(text, "<P2>", short, "</P2>", 1)]);
    fetchMock.mockImplementationOnce(async (_o: string, _s: string, texts: string[]) => ({
      workspace: "/w",
      slice: "alpha",
      refs: texts.map((t) => {
        const i = t.search(/A[12]\b/);
        return i < 0 ? [] : [{ start: i, end: i + 2, ids: [t.slice(i, i + 2)], label: `标${t.slice(i, i + 2)}` }];
      }),
      docs: [],
    }));
    askRefs(u, "devbox", "sid-1");
    await __refsSettledForTests();
    expect(fetchMock).toHaveBeenCalledTimes(1);
    const sent = fetchMock.mock.calls[0][2] as string[];
    expect(sent.some((t) => t.includes("A9-"))).toBe(false);
    expect(sent.join("|")).toContain("先 A1：");
    expect(sent.join("|")).toContain(short);
    expect([...u.querySelectorAll(".card-body button")].map((b) => b.textContent)).toEqual(["标A1", "标A2"]);
    expect(u.querySelector(".paste-fold button")).toBeNull();
  });
});
