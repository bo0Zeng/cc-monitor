/**
 * 活卡的判据（界面那一半：照壳交来的整份或尾巴画；同一张卡逐帧复用节点、正文前面没变的块不动、只重建尾巴）。
 *
 * 对账 · 缺口 · 记录到了整轮覆盖 · 子运行归位 · 截头 · 挤谁都在后端（`src/backend/stream/live.rs`，判据 `tests/backend/stream/live_tests.rs`）；
 * 一帧一帧折好在共享核心（`src/common/face-core/src/live.rs`，判据 `tests/common/face-core/live_tests.rs`）。
 * 这里只钉：整份照画；尾巴接在那张卡前 `keep` 块后面；空的一格 ⇒ 那一处撤；没这个 tab ⇒ 不留；主 tab 只画主运行；那台看不见了 ⇒ 全撤。
 * T8：活卡那一路怎么来、怎么断，都不碰记录那条对的路（真 TabManager，记录建出的 DOM 逐字相等）。
 */
import { describe, it, expect, vi, afterEach } from "vitest";
import { owned } from "../../test-support/owned-tab-manager.ts";
import { announced } from "../../test-support/session-faces";

vi.mock("@tauri-apps/api/core", async () => {
  const rig = await import("../../test-support/session-viewer-rig");
  const { withSessionReads } = await import("../../test-support/chan-fake");
  return {
    invoke: vi.fn(
      withSessionReads(async (cmd: string, args: Record<string, unknown>) =>
        cmd === "list_user_inputs" ? rig.answerListUserInputs(args as { fromOffset: number }) : undefined,
      ),
    ),
  };
});
vi.mock("@tauri-apps/plugin-opener", () => ({ openPath: vi.fn().mockResolvedValue(undefined) }));
vi.mock("../../../src/frontend/ui/tasks-panel", () => ({ fetchSessionTasks: vi.fn().mockResolvedValue([]) }));
vi.mock("../../../src/frontend/ui/behavior", () => ({
  getBehavior: vi.fn().mockResolvedValue({ resumeCommand: "cct" }),
}));
vi.mock("../../../src/frontend/ui/fork-flow", () => ({ runForkFlow: vi.fn().mockResolvedValue(undefined) }));

import { LiveCards, type LiveCardState } from "../../../src/frontend/ui/live-card";
import { installViewerRig, line, userLine, type RigPayload } from "../../test-support/session-viewer-rig";
import { TabManager } from "../../../src/frontend/ui/tabs";
import { livePainter } from "../../../src/frontend/ui/live-card-view";
import type { LiveCardNow } from "../../../src/frontend/ui/generated/LiveCardNow";
import type { MdBlock } from "../../../src/frontend/ui/generated/MdBlock";
import type { SessionLiveCardsPayload } from "../../../src/frontend/ui/generated/SessionLiveCardsPayload";
import { nonEmpty } from "../../test-support/non-empty.ts";

// ─── 手写的整份与尾巴（期望只从这里来） ─────────────────────────────────────────

const O = "<local>";

/** 一段字的块。 */
const p = (s: string): MdBlock => ({ t: "p", k: [s] });

/** 一张卡此刻的样子：头一行 · 正文 · 「最近」那一截是核心写好的（这里是结构占位）。 */
function now(card: number, rid: string, text: string, phase: "streaming" | "awaiting" = "streaming"): LiveCardNow {
  return {
    card,
    rid,
    phase,
    clipped: false,
    head: `核心·头·${phase}`,
    body: [p(`核心·正文·${text}`)],
    ...(phase === "streaming" ? { last: `核心·最近·${rid}` } : {}),
  };
}

/** 壳交来的一格：那台那一处此刻整份的卡。 */
const pay = (sid: string, cards: LiveCardNow[], origin = O, run?: string): SessionLiveCardsPayload => ({
  origin,
  live: { t: "whole", whole: { sid, ...(run ? { run } : {}), cards } },
});

/** 壳交来的一格：某张卡前 `keep` 块不变，后面换成 `blocks`。 */
const tail = (sid: string, card: number, keep: number, blocks: MdBlock[], last?: string): SessionLiveCardsPayload => ({
  origin: O,
  live: { t: "tail", tail: { sid, card, keep, blocks, ...(last === undefined ? {} : { last }) } },
});

const textOf = (b: readonly MdBlock[]): string[] => b.map((x) => (x.t === "p" ? x.k.map((k) => (typeof k === "string" ? k : "")).join("") : x.t));

const board = (route: (origin: string, sid: string) => boolean = () => true): LiveCards => new LiveCards(route, () => null);

describe("活卡照画（期望手写）", () => {
  it("★ 一张卡：头一行 · 正文照核心写好的摆（不按块的种类取字）", () => {
    const b = board();
    b.onCards(pay("s1", [now(7, "m1", "你好", "awaiting")]));
    const host = document.createElement("div");
    livePainter.cards(host, b.cardsOf("s1"));
    expect(host.textContent).toBe("核心·头·awaiting核心·正文·你好");
    expect(host.querySelector("p")?.textContent, "正文照树排成段落").toBe("核心·正文·你好");
    expect(b.cardsOf("s1")[0]!.messageId).toBe("m1");
  });

  it("每一格换掉那一处手上那一份；空的一格 ⇒ 那一处不画", () => {
    const b = board();
    b.onCards(pay("s1", [now(7, "m1", "a")]));
    b.onCards(pay("s1", [now(7, "m1", "尾巴")]));
    expect(b.cardsOf("s1").map((c) => textOf(c.body))).toEqual([["核心·正文·尾巴"]]);
    b.onCards(pay("s1", []));
    expect(b.size).toBe(0);
  });

  it("尾巴一格：那张卡前 keep 块还是同一批对象，后面换成交来的；手上没有那张卡 · 那一处 ⇒ 不收", () => {
    const b = board();
    const two: LiveCardNow = { ...now(7, "m1", "一"), body: [p("一"), p("二")] };
    b.onCards(pay("s1", [now(3, "m0", "另一张"), two]));
    const first = b.cardsOf("s1")[1]!.body[0];
    b.onCards(tail("s1", 7, 1, [p("二三"), p("四")], "核心·最近·新"));
    const c = b.cardsOf("s1")[1]!;
    expect(textOf(c.body)).toEqual(["一", "二三", "四"]);
    expect(c.body[0]).toBe(first);
    expect(textOf(b.cardsOf("s1")[0]!.body), "别的卡不动").toEqual(["核心·正文·另一张"]);
    b.onCards(tail("s1", 99, 0, [p("没开过")]));
    expect(b.cardsOf("s1").flatMap((x) => textOf(x.body))).not.toContain("没开过");
    b.onCards(tail("s9", 7, 0, [p("别的会话")]));
    expect(b.cardsOf("s9")).toEqual([]);
  });

  it("这台上没有这个 tab ⇒ 不留；子运行的卡不上主 tab，给 agent 面板那一行「最近」", () => {
    const b = board((origin, sid) => origin === O && sid === "s1");
    b.onCards(pay("s9", [now(1, "m", "x")]));
    b.onCards(pay("s1", [now(2, "m", "x")], "pi"));
    expect(b.size).toBe(0);
    b.onCards(pay("s1", [now(3, "r", "子")], O, "a1"));
    expect(b.cardsOf("s1")).toEqual([]);
    expect(b.cardsOf("s1", "a1")).toHaveLength(1);
    expect(b.liveLastOf("s1", "a1")).toBe("核心·最近·r");
    b.onCards(pay("s1", [now(3, "r", "子", "awaiting")], O, "a1"));
    expect(b.liveLastOf("s1", "a1"), "说完了 ⇒ 没有那一截").toBeNull();
  });

  it("那台看不见了 ⇒ 那台的卡全撤；tab 没了 ⇒ 它的卡不留", () => {
    const b = board();
    b.onCards(pay("s1", [now(1, "m", "x")]));
    b.onCards(pay("s2", [now(2, "m", "x")], "pi"));
    b.dropOrigin("pi");
    expect(b.cardsOf("s2")).toEqual([]);
    b.dropTab("s1");
    expect(b.size).toBe(0);
  });
});

describe("活卡增量画（同一张卡的节点复用，正文前面没变的块不动、只重建尾巴）", () => {
  const st = (key: string, body: readonly MdBlock[], phase: "streaming" | "awaiting" = "streaming", head = "头"): LiveCardState => ({
    key,
    messageId: "m",
    phase,
    head,
    body,
  });
  /** 那张卡正文那一格里的块节点。 */
  const blocksOf = (host: HTMLElement, key: string): Element[] => {
    const card = [...host.children].find((c) => (c as HTMLElement).dataset.liveKey === key);
    return [...(card?.lastElementChild?.children ?? [])];
  };

  it("★ 一张卡连喂 500 段（每段 40 字，每 10 段一个自然段）⇒ 卡节点只建一次，尾巴之外的块节点一次都不换", () => {
    const b = new LiveCards(
      () => true,
      () => host,
    );
    const host = document.createElement("div");
    b.setPainter(livePainter);
    b.onCards(pay("s1", [{ ...now(1, "m", ""), body: [] }]));
    const card = host.firstElementChild;
    expect(card).not.toBeNull();
    const seg = "字".repeat(40);
    const create = vi.spyOn(document, "createElement");
    let para = "";
    let built = 0;
    try {
      for (let k = 0; k < 500; k++) {
        if (k % 10 === 0) para = "";
        para += seg;
        const keep = Math.floor(k / 10);
        const before = blocksOf(host, "<local>\u00001").slice(0, keep);
        const calls = create.mock.calls.length;
        b.onCards(tail("s1", 1, keep, [p(para)]));
        built += create.mock.calls.length - calls;
        expect(host.firstElementChild, `第 ${k} 段换了卡节点`).toBe(card);
        const after = blocksOf(host, "<local>\u00001");
        expect(after.length).toBe(keep + 1);
        after.slice(0, keep).forEach((n, i) => expect(n, `第 ${k} 段换了尾巴之外的第 ${i} 块`).toBe(before[i]));
      }
    } finally {
      create.mockRestore();
    }
    expect(built, "每段只重建尾巴那一块").toBe(500);
    expect(blocksOf(host, "<local>\u00001").map((n) => n.textContent)).toEqual(Array.from({ length: 50 }, () => seg.repeat(10)));
  });

  it("这一帧的块与上一帧不是同一批对象（整份换了）⇒ 从第一处不同起重建；样子 · 头一行跟着这一帧", () => {
    const host = document.createElement("div");
    const a = [p("一"), p("二")];
    livePainter.cards(host, [st("k1", a)]);
    const card = host.firstElementChild as HTMLElement;
    const [n0, n1] = blocksOf(host, "k1");
    const streamingClass = card.className;
    livePainter.cards(host, [st("k1", [a[0]!, p("二改")], "awaiting", "头2")]);
    expect(host.firstElementChild).toBe(card);
    expect(blocksOf(host, "k1")[0]).toBe(n0);
    expect(blocksOf(host, "k1")[1]).not.toBe(n1);
    expect(card.firstElementChild?.textContent).toBe("头2");
    expect(card.className).not.toBe(streamingClass);
    livePainter.cards(host, [st("k1", [], "streaming", "头2")]);
    expect(card.className).toBe(streamingClass);
    expect(host.textContent).toBe("头2");
    livePainter.cards(host, [st("k1", [p("再来")], "streaming", "头2")]);
    expect(host.textContent).toBe("头2再来");
  });

  it("卡的增减与先后照这一帧：留下的那张节点复用，撤掉的不留，新来的排在后面", () => {
    const host = document.createElement("div");
    livePainter.cards(host, [st("k1", [p("a")]), st("k2", [p("b")])]);
    const k2 = host.querySelector('[data-live-key="k2"]');
    livePainter.cards(host, [st("k2", [p("bc")]), st("k3", [p("x")])]);
    expect([...host.children].map((c) => (c as HTMLElement).dataset.liveKey)).toEqual(["k2", "k3"]);
    expect(host.firstElementChild).toBe(k2);
    expect(host.textContent).toBe("头bc头x");
    livePainter.cards(host, [st("k3", [p("xy")]), st("k2", [p("bc")])]);
    expect([...host.children].map((c) => (c as HTMLElement).dataset.liveKey)).toEqual(["k3", "k2"]);
    expect(host.lastElementChild).toBe(k2);
    livePainter.cards(host, []);
    expect(host.childElementCount).toBe(0);
  });
});

describe("T8 活卡断 / 丢不碰 jsonl 那条对的路（抄流可以有缺口，主路不可以）", () => {
  afterEach(() => vi.unstubAllGlobals());

  const jsonl: RigPayload[] = [
    userLine(0, "u0", "问一句"),
    line(1, {
      t: "reply",
      id: "a1",
      at: "2026-09-10T00:00:01.000Z",
      blocks: [{ type: "text", text: "答一句" }],
      autoReply: false,
    }),
    userLine(2, "u2", "再问"),
  ];

  /** 跑一遍：先建 tab（第一条 jsonl），再按给定的几格喂，再喂剩下的 jsonl（随后到壳交来那一处空的一格）；回 jsonl 那一侧的 DOM 与中途那一刻的活卡。 */
  function run(seq: SessionLiveCardsPayload[], closeAfter = true): { content: string; liveMidway: string; liveAfter: string } {
    installViewerRig();
    const barEl = document.createElement("div");
    const streamRootEl = document.createElement("div");
    document.body.append(barEl, streamRootEl);
    const tm = owned(announced(new TabManager(barEl, streamRootEl)));
    tm.setLivePainter(livePainter);
    tm.onLine(jsonl[0] as never);
    for (const x of seq) tm.onLiveCard(x);
    const trailer = (): string =>
      [...streamRootEl.querySelectorAll(".stream")].map((s) => [...s.children].slice(1).map((c) => c.textContent).join("|")).join("#");
    const liveMidway = trailer();
    for (const x of jsonl.slice(1)) tm.onLine((x === jsonl[1] ? { ...x, rid: "msg_T8" } : x) as never);
    if (closeAfter && seq.length > 0) tm.onLiveCard(pay("s1", []));
    const content = [...streamRootEl.querySelectorAll(".stream-content")].map((e) => e.innerHTML).join("\n");
    return { content, liveMidway, liveAfter: trailer() };
  }

  it("没有活卡 · 活卡接不上（核心撤下那张）· 活卡撤了：jsonl 建出的卡逐字相等；有活卡时中途真画出来（正控），撤卡之后撤干净", () => {
    const whole = pay("s1", [now(0, "msg_T8", "答一句")]);
    const none = run([]);
    const drawn = run([whole]);
    const gapped = run([pay("s1", [now(0, "msg_T8", "答")]), pay("s1", [])], false);
    const closed = run([whole, pay("s1", [])], false);

    nonEmpty(none.content);
    expect(drawn.content).toBe(none.content);
    expect(gapped.content).toBe(none.content);
    expect(closed.content).toBe(none.content);

    // 正控：活卡真被画到了这个 tab（否则上面几条相等是空真）。
    expect(drawn.liveMidway).toContain("答一句");
    expect(none.liveMidway).toBe("");
    // 接不上 ⇒ 核心交空的、先不画；撤卡 ⇒ 撤。
    expect(gapped.liveMidway).toBe("");
    expect(closed.liveMidway).toBe("");
    expect(drawn.liveAfter).toBe("");
  });

  it("tab 按旧机器标签建出来（固定的 tab 复活）、后端宣告这个会话在另一台 ⇒ tab 的机器改成宣告的那台，那台来的流上得了活卡、标题前缀跟着变", () => {
    installViewerRig();
    const barEl = document.createElement("div");
    const streamRootEl = document.createElement("div");
    document.body.append(barEl, streamRootEl);
    const tm = owned(announced(new TabManager(barEl, streamRootEl)));
    tm.setLivePainter(livePainter);
    const trailer = (): string =>
      [...streamRootEl.querySelectorAll(".stream")].map((s) => [...s.children].slice(1).map((c) => c.textContent).join("|")).join("#");
    tm.createSkeletonTab("s9", "/w/p", "old-host"); // 盘上存的那份标签
    tm.createSkeletonTab("s9", "/w/p", "new-host"); // 后端宣告
    tm.onLiveCard(pay("s9", [now(0, "m9", "这台说的")], "new-host"));
    expect(trailer()).toContain("这台说的");
    expect(tm.snapshotSessions().map((x) => [x.sessionId, x.origin, x.title.includes("[new-host]"), x.title.includes("old-host")])).toEqual([
      ["s9", "new-host", true, false],
    ]);
    tm.onLiveCard(pay("s9", [now(1, "m10", "旧的")], "old-host"));
    expect(trailer()).not.toContain("旧的");
  });
});
