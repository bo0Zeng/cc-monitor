/**
 * 活卡的判据。
 *
 * 守的要求（住址 · 逐字）：
 * -：「SSE 只保快（临时态），jsonl 到了整轮覆盖，对账键是 `message.id`，不做记录级合并」（对账键今天由后端给：
 *   记录那一侧 `line.rid`、流那一侧归一事件 `start.rid`）·
 *   「按 sid 与 jsonl 对账（`<key>` 段就是 sid；对不上的只能当匿名流）」·「前端现有的去重层就是吸收层」。
 * -：「`Gap` 必须在流里的原位」—— 这里是每个响应里连续的位置号 `n`，缺口由接收侧纯算术看出来。
 * -：「不买抄流的完整性 —— 抄流可以有缺口，主路不可以」⇒ T8：SSE 断 / 丢不影响 jsonl 那条路。
 * 设计与上界表住仓外。
 *
 * 分两段：T6 纯状态机（期望手写，喂的是归一事件）· T8 真 TabManager（jsonl 那条路三种 tap 情形下建出的 DOM 逐字相等）。
 * T7（台架夹具：真 jsonl × 真 tap 按对账键比正文）随上游协议的折法搬进后端：`tests/backend/agents/sse_anthropic_tests.rs`。
 */
import { describe, it, expect, vi, afterEach } from "vitest";

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
vi.mock("../../../src/frontend/ui/turn-notify", () => ({ turnEndNotifier: { observe: vi.fn() } }));
vi.mock("../../../src/frontend/ui/behavior", () => ({
  getBehavior: vi.fn().mockResolvedValue({ resumeCommandLocal: "", resumeCommandRemote: "cct" }),
}));
vi.mock("../../../src/frontend/ui/remote-launch-run", () => ({
  runRemoteResume: vi.fn().mockResolvedValue(undefined),
  runRemoteResumeTmux: vi.fn().mockResolvedValue(undefined),
  runLocalResumeIntoExistingTmux: vi.fn().mockResolvedValue(true),
  runRemoteResumeIntoExistingTmux: vi.fn().mockResolvedValue(true),
  runRemoteAttach: vi.fn().mockResolvedValue(undefined),
}));
vi.mock("../../../src/frontend/ui/fork-flow", () => ({ runForkFlow: vi.fn().mockResolvedValue(undefined) }));

import {
  LiveCore,
  LIVE_PER_TAB,
  LIVE_STREAMS_KEEP,
  LIVE_TEXT_KEEP,
  renderCardText,
  type TapPayload,
} from "../../../src/frontend/ui/live-card";
import { copyText } from "../../../src/frontend/ui/copy-table";
import { installViewerRig, line, userLine, type RigPayload } from "../../test-support/session-viewer-rig";
import { TabManager } from "../../../src/frontend/ui/tabs";
import { livePainter } from "../../../src/frontend/ui/live-card-view";
import type { StreamEv } from "../../../src/frontend/ui/generated/StreamEv";

// ─── 手写的一轮 SSE（期望只从这里来） ─────────────────────────────────────────

const O = "<local>";

/** 一轮应答的归一事件（后端按上游协议折好的）：思考块 ＋ 正文块 ＋ 工具块。 */
function round(id: string, text: string[]): StreamEv[] {
  return [
    { t: "start", rid: id },
    { t: "block", i: 0, kind: "thinking" },
    { t: "block", i: 1, kind: "text" },
    ...text.map((x): StreamEv => ({ t: "text", i: 1, s: x })),
    { t: "block", i: 2, kind: "tool", tool: "Read" },
    { t: "stop", ok: true },
  ];
}

function taps(stream: string, resp: number, evs: StreamEv[], end?: "done" | "broken", run?: string): TapPayload[] {
  const out: TapPayload[] = evs.map((ev, n) => ({ origin: O, stream, resp, n, ev, ...(run ? { run } : {}) }));
  if (end) out.push({ origin: O, stream, resp, n: evs.length, end, ...(run ? { run } : {}) });
  return out;
}

/** 按 sid 路由：`stream` 就是 tab 的 sid 才算。 */
function core(tabs: string[] = ["s1"]): LiveCore {
  return new LiveCore((origin, stream) => (origin === O && tabs.includes(stream) ? stream : null));
}

const feed = (c: LiveCore, ts: TapPayload[]): void => {
  for (const t of ts) c.tap(t);
};

// ─── T6：纯状态机 ────────────────────────────────────────────────────────────

describe("T6 活卡状态机（期望手写）", () => {
  it("一轮流进来：正文拼成手写那一串；思考只显示一行、工具显示名字；说完之前是「生成中」，说完是「等待写入记录」", () => {
    const c = core();
    const data = round("msg_1", ["你好", "，", "世界"]);
    feed(c, taps("s1", 0, data.slice(0, 6)));
    let [card] = c.cardsOf("s1");
    expect(card?.phase).toBe("streaming");
    expect(renderCardText(card!).head).toBe(copyText("liveCard.state.streaming"));
    feed(c, taps("s1", 0, data).slice(6));
    [card] = c.cardsOf("s1");
    expect(card?.messageId).toBe("msg_1");
    expect(card?.phase).toBe("awaiting");
    expect(renderCardText(card!)).toEqual({
      head: copyText("liveCard.state.awaitingRecord"),
      body: [copyText("liveCard.block.thinking"), "你好，世界", copyText("liveCard.block.toolUse", { tool: "Read" })].join("\n"),
    });
  });

  it("记录那一轮到了（同对账键）⇒ 整张撤、进墓碑：之后同 id 的 tap 一律不收；别的 id 的记录不碰还在说的卡", () => {
    const c = core();
    const data = round("msg_1", ["a", "b"]);
    feed(c, taps("s1", 0, data.slice(0, 4)));
    c.record("s1", "msg_other"); // 不同 id、卡还没说完 ⇒ 不动
    expect(c.cardsOf("s1").map((x) => x.messageId)).toEqual(["msg_1"]);
    c.record("s1", "msg_1"); // 同 id 第一条 ⇒ 整张撤（不数块）
    expect(c.cardsOf("s1")).toEqual([]);
    feed(c, taps("s1", 0, data).slice(4)); // 同一个响应后面的 tap ⇒ 不收
    expect(c.cardsOf("s1")).toEqual([]);
    // 墓碑：换个响应号又来一遍同 id（重投）⇒ 也不收
    feed(c, taps("s1", 1, data, "done"));
    expect(c.cardsOf("s1")).toEqual([]);
  });

  it("缺口（n 连不上）⇒ 撤；开头就缺（第一件不是 0 号）⇒ 不收", () => {
    const c = core();
    const data = round("msg_1", ["a", "b", "c"]);
    const ts = taps("s1", 0, data);
    feed(c, ts.slice(0, 4));
    expect(c.cardsOf("s1")).toHaveLength(1);
    feed(c, ts.slice(5)); // 4 号丢了
    expect(c.cardsOf("s1")).toEqual([]);
    const d = core();
    feed(d, ts.slice(1)); // 0 号（带对账键的开始）丢了
    expect(d.cardsOf("s1")).toEqual([]);
    expect(d.size).toBe(0);
  });

  it("broken：说完之前断了 ⇒ 撤；上游说完之后才断（内容已全）⇒ 当说完了、等落盘", () => {
    const c = core();
    const data = round("msg_1", ["a"]);
    feed(c, taps("s1", 0, data.slice(0, 4)));
    c.tap({ origin: O, stream: "s1", resp: 0, n: 4, end: "broken" });
    expect(c.cardsOf("s1")).toEqual([]);
    feed(c, taps("s1", 1, round("msg_2", ["b"]), "broken"));
    expect(c.cardsOf("s1").map((x) => [x.messageId, x.phase])).toEqual([["msg_2", "awaiting"]]);
  });

  it("匿名流（stream 对不上任何 tab / 机器不对）⇒ 不显示、不留", () => {
    const c = core(["s1"]);
    feed(c, taps("nonce-123", 0, round("msg_1", ["a"])));
    feed(c, taps("s1", 1, round("msg_2", ["a"])).map((t) => ({ ...t, origin: "pi" })));
    expect(c.cardsOf("s1")).toEqual([]);
    expect(c.cardsOf("nonce-123")).toEqual([]);
    expect(c.size).toBe(0);
  });

  it("撤卡理由 ⑤：已收尾而同 tab 又来一条不同 id 的记录 ⇒ 撤（那个 id 不会再落进这份记录了）", () => {
    const c = core();
    feed(c, taps("s1", 0, round("msg_side", ["x"]), "done"));
    expect(c.cardsOf("s1")).toHaveLength(1);
    c.record("s1", null);
    expect(c.cardsOf("s1")).toEqual([]);
  });

  it("撤卡理由 ③：tab 结束 / 转灰 / 关掉 ⇒ 全撤", () => {
    const c = core(["s1", "s2"]);
    feed(c, taps("s1", 0, round("m1", ["a"]).slice(0, 5)));
    feed(c, taps("s2", 1, round("m2", ["a"]).slice(0, 5)));
    c.dropTab("s1");
    expect(c.cardsOf("s1")).toEqual([]);
    expect(c.cardsOf("s2")).toHaveLength(1);
  });

  it("上界：每 tab ≤ LIVE_PER_TAB（先挤已收尾的）· 全局 ≤ LIVE_STREAMS_KEEP · 正文 ≤ LIVE_TEXT_KEEP（截头、标出来）", () => {
    const c = core(["s1"]);
    feed(c, taps("s1", 0, round("m0", ["a"]), "done")); // 已收尾
    feed(c, taps("s1", 1, round("m1", ["b"]).slice(0, 5))); // 还在说
    feed(c, taps("s1", 2, round("m2", ["c"]).slice(0, 5))); // 第三个 ⇒ 挤掉已收尾的 m0
    expect(c.cardsOf("s1").map((x) => x.messageId)).toEqual(["m1", "m2"]);
    expect(c.cardsOf("s1")).toHaveLength(LIVE_PER_TAB);

    // 全局：子运行占满了再来 ⇒ 挤最老的，总数守在上界（正在流的主运行不挤，见「满载」那一组）。
    const g = core(["busy"]);
    for (let i = 0; i < LIVE_STREAMS_KEEP + 5; i++) feed(g, taps("busy", i, round(`m${i}`, ["a"]).slice(0, 3), undefined, `w${i}`));
    expect(g.size).toBe(LIVE_STREAMS_KEEP);
    expect(g.cardsOf("busy", "w0")).toEqual([]); // 最老的被挤
    expect(g.cardsOf("busy", `w${LIVE_STREAMS_KEEP + 4}`)).toHaveLength(1);

    const big = core();
    const chunk = "字".repeat(10_000);
    feed(big, taps("s1", 0, round("mb", [chunk, chunk, chunk, chunk])));
    const [card] = big.cardsOf("s1");
    const text = card!.blocks.find((b) => b.kind === "text")!.text;
    expect(text.length).toBe(LIVE_TEXT_KEEP);
    expect(card!.clipped).toBe(true);
    expect(renderCardText(card!).body.startsWith(copyText("liveCard.body.clipped"))).toBe(true);
  });

  it("上游报错收尾（stop ok=false）⇒ 撤", () => {
    const d = core();
    feed(d, taps("s1", 0, [{ t: "start", rid: "m1" }, { t: "stop", ok: false }]));
    expect(d.cardsOf("s1")).toEqual([]);
  });
});

// ─── 满载：十几路子运行同时在问，主运行长回合先思考后出字 ────────────────────────

describe("满载：十几路子运行并发时，正在流的主运行活卡留得住", () => {
  /** 主运行一段长回合：先只有思考（迟迟不出字）。 */
  const thinking = (sid: string, resp: number, rid: string): TapPayload[] =>
    taps(sid, resp, [
      { t: "start", rid },
      { t: "block", i: 0, kind: "thinking" },
    ]);
  /** 这段终于出字了（接在 `thinking` 那两件之后）。 */
  const speaks = (sid: string, resp: number, s: string): TapPayload[] =>
    [
      { t: "block", i: 1, kind: "text" },
      { t: "text", i: 1, s },
    ].map((ev, k): TapPayload => ({ origin: O, stream: sid, resp, n: 2 + k, ev: ev as StreamEv }));
  /** 子运行的一段：开头两件（还在说）· 收尾两件（说完 ＋ done）。 */
  const subOpen = (sid: string, run: string, resp: number): TapPayload[] =>
    taps(sid, resp, [{ t: "start", rid: `r${resp}` }, { t: "block", i: 0, kind: "tool", tool: "Bash" }], undefined, run);
  const subClose = (sid: string, run: string, resp: number): TapPayload[] => [
    { origin: O, stream: sid, run, resp, n: 2, ev: { t: "stop", ok: true } },
    { origin: O, stream: sid, run, resp, n: 3, end: "done" },
  ];

  it("★ 那个现场：14 路子运行一段接一段地问、峰值 17 段同时在说；主运行先思考很久、最后出字 ⇒ 它的活卡一直在、字上得来", () => {
    const c = core(["main", "busy"]);
    feed(c, thinking("main", 0, "m-main"));
    let resp = 1;
    for (let round = 0; round < 5; round++) {
      const open: [string, number][] = [];
      for (let w = 0; w < 14; w++) {
        const r = resp++;
        feed(c, subOpen("busy", `w${w}`, r));
        open.push([`w${w}`, r]);
      }
      // 洪峰：另有三路同时开着（这一刻 17 段子运行 ＋ 1 段主运行在说）。
      for (let w = 14; w < 17; w++) {
        const r = resp++;
        feed(c, subOpen("busy", `w${w}`, r));
        open.push([`w${w}`, r]);
      }
      expect(c.cardsOf("main").map((x) => x.messageId), `第 ${round} 轮洪峰里主运行的活卡没了`).toEqual(["m-main"]);
      for (const [w, r] of open) feed(c, subClose("busy", w, r));
    }
    feed(c, speaks("main", 0, "答一句"));
    const [card] = c.cardsOf("main");
    expect(card?.phase).toBe("streaming");
    expect(renderCardText(card!).body).toBe([copyText("liveCard.block.thinking"), "答一句"].join("\n"));
    expect(c.size, "说完的子运行段还占着坑").toBe(1);
  });

  it("子运行说完的段：它的时间线没开着 ⇒ 当场撤（不占坑）；开着 ⇒ 留着「等写入记录」，记录到了 / 时间线收起才撤", () => {
    const open = new Set<string>(["busy\u0000w1"]);
    const c = new LiveCore(
      (origin, stream) => (origin === O && ["busy"].includes(stream) ? stream : null),
      (sid, run) => open.has(`${sid}\u0000${run}`),
    );
    for (let r = 0; r < 3 * LIVE_STREAMS_KEEP; r++) {
      feed(c, [...subOpen("busy", "w0", r), ...subClose("busy", "w0", r)]);
      expect(c.size, `第 ${r} 段说完了还占着坑`).toBe(0);
    }
    feed(c, [...subOpen("busy", "w1", 100), ...subClose("busy", "w1", 100)]);
    expect(c.cardsOf("busy", "w1").map((x) => [x.messageId, x.phase])).toEqual([["r100", "awaiting"]]);
    feed(c, [...subOpen("busy", "w1", 101), ...subClose("busy", "w1", 101)]);
    c.record("busy", "r100", "w1"); // 记录到了 ⇒ 同 id 那张撤；另一张已收尾、id 不同 ⇒ 也撤
    expect(c.cardsOf("busy", "w1")).toEqual([]);
    feed(c, [...subOpen("busy", "w1", 102), ...subClose("busy", "w1", 102)]);
    expect(c.size).toBe(1);
    open.clear();
    c.unwatched("busy", "w1"); // 时间线收起 ⇒ 说完的那几段撤
    expect(c.size).toBe(0);
  });

  it("满了挤谁：先挤说完的、再挤子运行的（最老的先）；正在流的主运行一个不挤 —— 全是它们时主运行照收、子运行不收", () => {
    const tabs = Array.from({ length: LIVE_STREAMS_KEEP + 4 }, (_, i) => `t${i}`);
    const c = core([...tabs, "busy"]);
    feed(c, taps("t0", 0, round("done0", ["a"]), "done")); // 说完了、等记录
    feed(c, thinking("t1", 1, "m1")); // 正在流的主运行
    for (let r = 2; r < LIVE_STREAMS_KEEP; r++) feed(c, subOpen("busy", `w${r}`, r)); // 子运行在说
    expect(c.size).toBe(LIVE_STREAMS_KEEP);
    feed(c, thinking("t2", 50, "m2")); // 满了 ⇒ 挤说完的那张
    expect(c.cardsOf("t0")).toEqual([]);
    feed(c, thinking("t3", 51, "m3")); // 再来 ⇒ 挤最老的子运行
    expect(c.cardsOf("busy", "w2")).toEqual([]);
    expect(c.cardsOf("busy", "w3")).toHaveLength(1);
    expect(c.size).toBe(LIVE_STREAMS_KEEP);
    for (let i = 4; i < tabs.length; i++) feed(c, thinking(`t${i}`, 60 + i, `m${i}`)); // 子运行挤光、之后主运行照收
    expect(c.size).toBe(tabs.length - 1);
    for (let i = 1; i < tabs.length; i++) expect(c.cardsOf(`t${i}`), `t${i} 正在流却被挤了`).toHaveLength(1);
    feed(c, subOpen("busy", "late", 99)); // 全是正在流的主运行 ⇒ 子运行这段不收
    expect(c.cardsOf("busy", "late")).toEqual([]);
    expect(c.size).toBe(tabs.length - 1);
  });

  it("每一种丢都数、都出声（同一种第 1、2、4、8… 次说一行，带累计）；正常撤卡不算丢", () => {
    const said: string[] = [];
    const c = new LiveCore(
      (origin, stream) => (origin === O && ["s1", "busy"].includes(stream) ? stream : null),
      () => false,
      (why, n) => said.push(`${why}#${n}`),
    );
    for (let r = 0; r < 5; r++) feed(c, taps("nobody", r, round(`x${r}`, ["a"]))); // 匿名 ×5
    feed(c, taps("s1", 10, round("g", ["a", "b"])).filter((t) => t.n !== 3)); // 断号
    feed(c, taps("s1", 11, round("h", ["a"])).slice(1)); // 头件丢
    feed(c, [...taps("s1", 12, round("k", ["a"]).slice(0, 3)), { origin: O, stream: "s1", resp: 12, n: 3, end: "broken" }]); // 断在半路
    feed(c, taps("s1", 13, round("ok", ["a"]), "done"));
    c.record("s1", "ok"); // 定稿：正常撤，不算丢
    expect(c.lost).toEqual({ anon: 5, gap: 1, head: 1, broken: 1 });
    expect(said).toEqual(["anon#1", "anon#2", "anon#4", "gap#1", "head#1", "broken#1"]);
  });
});

// ─── T8：SSE 断 / 丢不影响 jsonl 那条路（真 TabManager，jsonl 那条路建出的 DOM 三向逐字相等） ──

describe("T8 SSE 断 / 丢不碰 jsonl 那条对的路（抄流可以有缺口，主路不可以）", () => {
  afterEach(() => vi.unstubAllGlobals());

  const jsonl: RigPayload[] = [
    userLine(0, "u0", "问一句"),
    line(1, {
      type: "assistant",
      uuid: "a1",
      timestamp: "2026-09-10T00:00:01.000Z",
      message: { id: "msg_T8", role: "assistant", content: [{ type: "text", text: "答一句" }] },
      sessionId: "s1",
      requestId: null,
      parentUuid: null,
      forkedFrom: null,
      isApiErrorMessage: false,
      error: null,
      apiErrorStatus: null,
    }),
    userLine(2, "u2", "再问"),
  ];

  /** 跑一遍：先建 tab（第一条 jsonl），再按给定的 tap 序列喂，再喂剩下的 jsonl；回 jsonl 那一侧的 DOM 与流中途那一刻的活卡。 */
  function run(tapSeq: TapPayload[]): { content: string; liveMidway: string; liveAfter: string } {
    installViewerRig();
    const barEl = document.createElement("div");
    const streamRootEl = document.createElement("div");
    document.body.append(barEl, streamRootEl);
    const tm = new TabManager(barEl, streamRootEl);
    tm.setLivePainter(livePainter);
    tm.onLine(jsonl[0] as never);
    for (const t of tapSeq) tm.onSessionTap(t);
    const trailer = (): string =>
      [...streamRootEl.querySelectorAll(".stream")].map((s) => [...s.children].slice(1).map((c) => c.textContent).join("|")).join("#");
    const liveMidway = trailer();
    for (const p of jsonl.slice(1)) tm.onLine((p === jsonl[1] ? { ...p, rid: "msg_T8" } : p) as never);
    const content = [...streamRootEl.querySelectorAll(".stream-content")].map((e) => e.innerHTML).join("\n");
    return { content, liveMidway, liveAfter: trailer() };
  }

  it("无 tap · tap 中途缺口 · tap broken：jsonl 建出的卡逐字相等；有 tap 时中途真有活卡（正控），jsonl 到了之后都撤干净", () => {
    const data = round("msg_T8", ["答", "一句"]);
    const none = run([]);
    const whole = run(taps("s1", 0, data.slice(0, 5)));
    const gapped = run(taps("s1", 0, data).filter((t) => t.n !== 3));
    const broken = run([...taps("s1", 0, data.slice(0, 5)), { origin: O, stream: "s1", resp: 0, n: 5, end: "broken" }]);

    expect(none.content.length, "jsonl 那条路一张卡都没建 —— 本组空转").toBeGreaterThan(0);
    expect(whole.content).toBe(none.content);
    expect(gapped.content).toBe(none.content);
    expect(broken.content).toBe(none.content);

    // 正控：tap 真被路由到了这个 tab、真画出了活卡（否则上面三条相等是空真）。
    expect(whole.liveMidway).toContain("答一");
    expect(none.liveMidway).toBe("");
    // 缺口 / broken ⇒ 当场撤；整轮到了 ⇒ 同 id 的 jsonl 一到就撤。
    expect(gapped.liveMidway).toBe("");
    expect(broken.liveMidway).toBe("");
    expect(whole.liveAfter).toBe("");
  });

  it("tab 按旧机器标签建出来（固定的 tab 复活）、后端宣告这个会话在另一台 ⇒ tab 的机器改成宣告的那台，那台来的流上得了活卡、标题前缀跟着变", () => {
    installViewerRig();
    const barEl = document.createElement("div");
    const streamRootEl = document.createElement("div");
    document.body.append(barEl, streamRootEl);
    const tm = new TabManager(barEl, streamRootEl);
    tm.setLivePainter(livePainter);
    const trailer = (): string =>
      [...streamRootEl.querySelectorAll(".stream")].map((s) => [...s.children].slice(1).map((c) => c.textContent).join("|")).join("#");
    tm.createSkeletonTab("s9", "/w/p", "old-host"); // 盘上存的那份标签
    tm.createSkeletonTab("s9", "/w/p", "new-host"); // 后端宣告
    for (const t of taps("s9", 0, round("m9", ["这台", "说的"]).slice(0, 5))) tm.onSessionTap({ ...t, origin: "new-host" });
    expect(trailer()).toContain("这台说的");
    expect(tm.snapshotSessions().map((x) => [x.sessionId, x.origin, x.title.includes("[new-host]"), x.title.includes("old-host")])).toEqual([
      ["s9", "new-host", true, false],
    ]);
    for (const t of taps("s9", 1, round("m10", ["旧的"]).slice(0, 5))) tm.onSessionTap({ ...t, origin: "old-host" });
    expect(trailer()).not.toContain("旧的");
  });
});
