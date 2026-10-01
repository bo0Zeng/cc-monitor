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
vi.mock("../../../src/frontend/ui/account-restart", () => ({
  restartWithAccount: vi.fn().mockResolvedValue(undefined),
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

    const many = Array.from({ length: LIVE_STREAMS_KEEP + 5 }, (_, i) => `t${i}`);
    const g = core(many);
    many.forEach((sid, i) => feed(g, taps(sid, i, round(`m${i}`, ["a"]).slice(0, 3))));
    expect(g.size).toBe(LIVE_STREAMS_KEEP);
    expect(g.cardsOf("t0")).toEqual([]); // 最老的被挤
    expect(g.cardsOf(`t${many.length - 1}`)).toHaveLength(1);

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
    tm.setLivePainter(livePainter({ timeline: () => document.createElement("div"), closed: () => {} }));
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
});
