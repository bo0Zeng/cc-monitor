/**
 * 〔TAP · V124〕活卡的判据。
 *
 * 守的要求（住址 · 逐字）：
 * - `设计/20 §8`（V24）：「SSE 只保快（临时态），jsonl 到了整轮覆盖，对账键是 `message.id`，不做记录级合并」·
 *   「按 sid 与 jsonl 对账（`<key>` 段就是 sid；对不上的只能当匿名流）」·「前端现有的去重层就是吸收层」。
 * - `设计/05 §3.3.4`：「`Gap` 必须在流里的原位」—— 这里是每个响应里连续的位置号 `n`，缺口由接收侧纯算术看出来。
 * - `设计/05 §4.5.3` ③：「不买抄流的完整性 —— 抄流可以有缺口，主路不可以」⇒ T8：SSE 断 / 丢不影响 jsonl 那条路。
 * 设计与上界表住仓外 `调研/第四波记录/TAP.md §2 · §4 · §5 · §7`。
 *
 * 分三段：T6 纯状态机（期望手写）· T8 真 TabManager（jsonl 那条路三种 tap 情形下建出的 DOM 逐字相等）·
 * T7 台架夹具（真 claude 写的 jsonl × 真中转抄出来的 tap，按 `message.id` 比正文）。
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

vi.mock("@tauri-apps/api/core", async () => {
  const rig = await import("./test-support/session-viewer-rig");
  const { withSessionReads } = await import("./test-support/chan-fake");
  return {
    invoke: vi.fn(
      withSessionReads(async (cmd: string, args: Record<string, unknown>) =>
        cmd === "list_user_inputs" ? rig.answerListUserInputs(args as { fromOffset: number }) : undefined,
      ),
    ),
  };
});
vi.mock("@tauri-apps/plugin-opener", () => ({ openPath: vi.fn().mockResolvedValue(undefined) }));
vi.mock("../src/tasks-panel", () => ({ fetchSessionTasks: vi.fn().mockResolvedValue([]) }));
vi.mock("../src/turn-notify", () => ({ turnEndNotifier: { observe: vi.fn() } }));
vi.mock("../src/behavior", () => ({
  getBehavior: vi.fn().mockResolvedValue({ resumeCommandLocal: "", resumeCommandRemote: "cct" }),
}));
vi.mock("../src/remote-launch-run", () => ({
  runRemoteResume: vi.fn().mockResolvedValue(undefined),
  runRemoteResumeTmux: vi.fn().mockResolvedValue(undefined),
  runLocalResumeIntoExistingTmux: vi.fn().mockResolvedValue(true),
  runRemoteResumeIntoExistingTmux: vi.fn().mockResolvedValue(true),
  runRemoteAttach: vi.fn().mockResolvedValue(undefined),
}));
vi.mock("../src/account-restart", () => ({
  restartWithAccount: vi.fn().mockResolvedValue(undefined),
  DEFAULT_EXIT_WAIT_MS: 10_000,
}));
vi.mock("../src/fork-flow", () => ({ runForkFlow: vi.fn().mockResolvedValue(undefined) }));

import {
  LiveCore,
  LIVE_PER_TAB,
  LIVE_STREAMS_KEEP,
  LIVE_TEXT_KEEP,
  renderCardText,
  type TapPayload,
} from "../src/live-card";
import { copyText } from "../src/copy-table";
import { installViewerRig, line, userLine, type RigPayload } from "./test-support/session-viewer-rig";
import { TabManager } from "../src/tabs";
import { REPO_ROOT } from "./test-support/repo-root.ts";

// ─── 手写的一轮 SSE（期望只从这里来） ─────────────────────────────────────────

const O = "<local>";

/** 一轮响应的 `data:` 原文：思考块 ＋ 正文块 ＋ 工具块。 */
function round(id: string, text: string[]): string[] {
  return [
    JSON.stringify({ type: "message_start", message: { id, model: "m" } }),
    JSON.stringify({ type: "content_block_start", index: 0, content_block: { type: "thinking", thinking: "" } }),
    JSON.stringify({ type: "content_block_delta", index: 0, delta: { type: "thinking_delta", thinking: "想" } }),
    JSON.stringify({ type: "content_block_stop", index: 0 }),
    JSON.stringify({ type: "content_block_start", index: 1, content_block: { type: "text", text: "" } }),
    JSON.stringify({ type: "ping" }),
    ...text.map((t) => JSON.stringify({ type: "content_block_delta", index: 1, delta: { type: "text_delta", text: t } })),
    JSON.stringify({ type: "content_block_stop", index: 1 }),
    JSON.stringify({ type: "content_block_start", index: 2, content_block: { type: "tool_use", name: "Read", input: {} } }),
    JSON.stringify({ type: "content_block_delta", index: 2, delta: { type: "input_json_delta", partial_json: "{}" } }),
    JSON.stringify({ type: "content_block_stop", index: 2 }),
    JSON.stringify({ type: "message_delta", delta: { stop_reason: "tool_use" } }),
    JSON.stringify({ type: "message_stop" }),
  ];
}

function taps(stream: string, resp: number, data: string[], end?: "done" | "broken"): TapPayload[] {
  const out: TapPayload[] = data.map((d, n) => ({ origin: O, stream, resp, n, data: d }));
  if (end) out.push({ origin: O, stream, resp, n: data.length, end });
  return out;
}

/** 按 sid 路由：`stream` 就是 tab 的 sid 才算（`20 §8`）。 */
function core(tabs: string[] = ["s1"]): LiveCore {
  return new LiveCore((origin, stream) => (origin === O && tabs.includes(stream) ? stream : null));
}

const feed = (c: LiveCore, ts: TapPayload[]): void => {
  for (const t of ts) c.tap(t);
};

const assistantRec = (id: string): unknown => ({ type: "assistant", message: { id, role: "assistant" } });

// ─── T6：纯状态机 ────────────────────────────────────────────────────────────

describe("T6 活卡状态机（期望手写）", () => {
  it("一轮流进来：正文拼成手写那一串；思考只显示一行、工具显示名字；说完之前是「生成中」，说完是「等待写入记录」", () => {
    const c = core();
    const data = round("msg_1", ["你好", "，", "世界"]);
    feed(c, taps("s1", 0, data.slice(0, 8)));
    let [card] = c.cardsOf("s1");
    expect(card?.phase).toBe("streaming");
    expect(renderCardText(card!).head).toBe(copyText("liveCard.state.streaming"));
    feed(c, taps("s1", 0, data).slice(8));
    [card] = c.cardsOf("s1");
    expect(card?.messageId).toBe("msg_1");
    expect(card?.phase).toBe("awaiting");
    expect(renderCardText(card!)).toEqual({
      head: copyText("liveCard.state.awaitingRecord"),
      body: [copyText("liveCard.block.thinking"), "你好，世界", copyText("liveCard.block.toolUse", { tool: "Read" })].join("\n"),
    });
  });

  it("jsonl 那一轮到了（同 message.id）⇒ 整张撤、进墓碑：之后同 id 的 tap 一律不收；别的 id 的记录不碰还在说的卡", () => {
    const c = core();
    const data = round("msg_1", ["a", "b"]);
    feed(c, taps("s1", 0, data.slice(0, 7)));
    c.record("s1", assistantRec("msg_other")); // 不同 id、卡还没说完 ⇒ 不动
    expect(c.cardsOf("s1").map((x) => x.messageId)).toEqual(["msg_1"]);
    c.record("s1", assistantRec("msg_1")); // 同 id 第一条 ⇒ 整张撤（不数块、不看 apiBlockIndex）
    expect(c.cardsOf("s1")).toEqual([]);
    feed(c, taps("s1", 0, data).slice(7)); // 同一个响应后面的 tap ⇒ 不收
    expect(c.cardsOf("s1")).toEqual([]);
    // 墓碑：换个响应号又来一遍同 id（重投）⇒ 也不收
    feed(c, taps("s1", 1, data, "done"));
    expect(c.cardsOf("s1")).toEqual([]);
  });

  it("缺口（n 连不上）⇒ 撤；开头就缺（第一件不是 0 号）⇒ 不收", () => {
    const c = core();
    const data = round("msg_1", ["a", "b", "c"]);
    const ts = taps("s1", 0, data);
    feed(c, ts.slice(0, 5));
    expect(c.cardsOf("s1")).toHaveLength(1);
    feed(c, ts.slice(6)); // 5 号丢了
    expect(c.cardsOf("s1")).toEqual([]);
    const d = core();
    feed(d, ts.slice(1)); // 0 号（message_start）丢了
    expect(d.cardsOf("s1")).toEqual([]);
    expect(d.size).toBe(0);
  });

  it("broken：说完之前断了 ⇒ 撤；message_stop 之后才断（内容已全）⇒ 当说完了、等落盘", () => {
    const c = core();
    const data = round("msg_1", ["a"]);
    feed(c, taps("s1", 0, data.slice(0, 7)));
    c.tap({ origin: O, stream: "s1", resp: 0, n: 7, end: "broken" });
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

  it("撤卡理由 ⑤：已收尾而同 tab 又来一条不同 id 的 jsonl 记录 ⇒ 撤（那个 id 不会再落进这份 jsonl 了）", () => {
    const c = core();
    feed(c, taps("s1", 0, round("msg_side", ["x"]), "done"));
    expect(c.cardsOf("s1")).toHaveLength(1);
    c.record("s1", { type: "user", message: { role: "user", content: "hi" } });
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

  it("读不懂的原文 / 不认识的事件 ⇒ 不理（不当缺口、不撤卡）；error 事件 ⇒ 撤", () => {
    const c = core();
    const data = round("m1", ["a"]);
    const ts = taps("s1", 0, [...data.slice(0, 5), "not json", JSON.stringify({ type: "future_thing" }), ...data.slice(5)]);
    feed(c, ts);
    expect(c.cardsOf("s1").map((x) => x.phase)).toEqual(["awaiting"]);
    const d = core();
    feed(d, taps("s1", 0, [data[0]!, JSON.stringify({ type: "error", error: { type: "overloaded_error" } })]));
    expect(d.cardsOf("s1")).toEqual([]);
  });
});

// ─── T8：SSE 断 / 丢不影响 jsonl 那条路（真 TabManager，jsonl 那条路建出的 DOM 三向逐字相等） ──

describe("T8 SSE 断 / 丢不碰 jsonl 那条对的路（`05 §4.5.3` ③：抄流可以有缺口，主路不可以）", () => {
  afterEach(() => vi.unstubAllGlobals());

  const jsonl: RigPayload[] = [
    userLine(0, "u0", "问一句"),
    line(1, {
      type: "assistant",
      uuid: "a1",
      timestamp: "2026-09-10T00:00:01.000Z",
      message: { id: "msg_T8", role: "assistant", content: [{ type: "text", text: "答一句" }] },
      sessionId: "s1",
      isSidechain: false,
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
    tm.onLine(jsonl[0] as never);
    for (const t of tapSeq) tm.onSessionTap(t);
    const trailer = (): string =>
      [...streamRootEl.querySelectorAll(".stream")].map((s) => [...s.children].slice(1).map((c) => c.textContent).join("|")).join("#");
    const liveMidway = trailer();
    for (const p of jsonl.slice(1)) tm.onLine(p as never);
    const content = [...streamRootEl.querySelectorAll(".stream-content")].map((e) => e.innerHTML).join("\n");
    return { content, liveMidway, liveAfter: trailer() };
  }

  it("无 tap · tap 中途缺口 · tap broken：jsonl 建出的卡逐字相等；有 tap 时中途真有活卡（正控），jsonl 到了之后都撤干净", () => {
    const data = round("msg_T8", ["答", "一句"]);
    const none = run([]);
    const whole = run(taps("s1", 0, data.slice(0, 8)));
    const gapped = run(taps("s1", 0, data).filter((t) => t.n !== 3));
    const broken = run([...taps("s1", 0, data.slice(0, 8)), { origin: O, stream: "s1", resp: 0, n: 8, end: "broken" }]);

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

// ─── T7：台架夹具（真 claude 写的 jsonl × 真中转抄出来的 tap） ──────────────────

describe("T7 同一轮 SSE 拼出的正文 == claude 落盘的正文（按 message.id；夹具由 tests/evidence/TAP-bench.py 产出）", () => {
  interface Fixture {
    taps: TapPayload[];
    jsonl: unknown[];
    sid: string;
  }
  const load = (): Fixture =>
    JSON.parse(readFileSync(resolve(REPO_ROOT, "tests/__fixtures__/tap-bench.json"), "utf8")) as Fixture;

  it("夹具两侧都非空，且 tap 的 stream 就是 claude 那个会话的 sid（resume 形）", () => {
    const fx = load();
    expect(fx.taps.length).toBeGreaterThan(0);
    expect(fx.jsonl.length).toBeGreaterThan(0);
    expect(new Set(fx.taps.map((t) => t.stream))).toEqual(new Set([fx.sid]));
  });

  it("每个 message.id：生产状态机拼出的正文 == jsonl 里同 id 的 text 块按 apiBlockIndex 顺序拼起来", () => {
    const fx = load();
    // tap 侧：生产状态机，不做 jsonl 那一侧的覆盖（只看它拼出什么）。
    const c = new LiveCore((_o, s) => s);
    const byResp = new Map<number, string>();
    for (const t of fx.taps) {
      c.tap(t);
      for (const card of c.cardsOf(fx.sid)) {
        const text = card.blocks.filter((b) => b && b.kind === "text").map((b) => b.text).join("");
        if (card.messageId) byResp.set(t.resp, `${card.messageId}\u0000${text}`);
      }
    }
    const tapText = new Map<string, string>();
    for (const v of byResp.values()) {
      const [id, text] = v.split("\u0000") as [string, string];
      tapText.set(id, text);
    }
    // jsonl 侧：claude 写的记录，同 id 按 apiBlockIndex 排、只取 text 块。
    const recs = fx.jsonl.filter(
      (r): r is { message: { id: string; content: { type: string; text?: string }[] }; apiBlockIndex?: number } =>
        typeof r === "object" && r !== null && (r as { type?: string }).type === "assistant",
    );
    const jsonlText = new Map<string, string>();
    for (const id of new Set(recs.map((r) => r.message.id))) {
      const parts = recs
        .filter((r) => r.message.id === id)
        .sort((a, b) => (a.apiBlockIndex ?? 0) - (b.apiBlockIndex ?? 0))
        .flatMap((r) => r.message.content.filter((b) => b.type === "text").map((b) => b.text ?? ""));
      jsonlText.set(id, parts.join(""));
    }
    expect(jsonlText.size, "jsonl 里一个 assistant 的 message.id 都没有 —— 夹具空转").toBeGreaterThan(0);
    // 两向：jsonl 里有的每个 id，tap 侧都拼出了同样的正文；tap 侧拼出的 id 集合 == jsonl 的。
    expect(new Map([...tapText].filter(([id]) => jsonlText.has(id)))).toEqual(jsonlText);
    expect(new Set(tapText.keys())).toEqual(new Set(jsonlText.keys()));
  });
});
