/**
 * 活卡的判据（界面那一半：照后端的成品画）。
 *
 * 对账 · 缺口 · 记录到了整轮覆盖 · 子运行归位 · 截头 · 挤谁都在后端（`src/backend/stream/live.rs`，判据 `tests/backend/stream/live_tests.rs`）；
 * 这里只钉：几种 `op` 照画；`n` 接不上 ⇒ 那张先不画、等下一张整卡；没这个 tab ⇒ 不留；主 tab 只画主运行；那台看不见了 ⇒ 全撤。
 * T8：活卡那一路怎么来、怎么断，都不碰记录那条对的路（真 TabManager，记录建出的 DOM 逐字相等）。
 */
import { describe, it, expect, vi, afterEach } from "vitest";
import { owned } from "../../test-support/owned-tab-manager.ts";

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

import { LiveBoard, renderCardText } from "../../../src/frontend/ui/live-card";
import { copyText } from "../../../src/frontend/ui/copy-table";
import { installViewerRig, line, userLine, type RigPayload } from "../../test-support/session-viewer-rig";
import { TabManager } from "../../../src/frontend/ui/tabs";
import { livePainter } from "../../../src/frontend/ui/live-card-view";
import type { LiveCard } from "../../../src/frontend/ui/generated/LiveCard";
import type { SessionLiveCardPayload } from "../../../src/frontend/ui/generated/SessionLiveCardPayload";

// ─── 手写的帧（期望只从这里来） ─────────────────────────────────────────

const O = "<local>";

/** 一张卡从开到说完：整卡（思考块 ＋ 空正文块）→ 几段字 → 工具块 → 说完了。`n` 从 0 连着编。 */
function card(sid: string, no: number, rid: string, text: string[], run?: string): LiveCard[] {
  const base = { sid, card: no, ...(run ? { run } : {}) };
  const ops: LiveCard[] = [
    {
      ...base,
      n: 0,
      op: "open",
      rid,
      phase: "streaming",
      clipped: false,
      blocks: [{ kind: "thinking" }, { kind: "text", text: "" }],
    },
    ...text.map((s, k): LiveCard => ({ ...base, n: k + 1, op: "text", i: 1, s })),
  ];
  ops.push({ ...base, n: ops.length, op: "block", i: 2, block: "tool", tool: "Read" });
  ops.push({ ...base, n: ops.length, op: "awaiting" });
  return ops;
}

const close = (sid: string, no: number, n: number): LiveCard => ({ sid, card: no, n, op: "close", why: "record" });
const pay = (c: LiveCard, origin = O): SessionLiveCardPayload => ({ origin, card: c });

describe("活卡照画（期望手写）", () => {
  it("一张卡：正文拼成手写那一串；思考只显示一行、工具显示名字；说完之前是「生成中」，说完是「等待写入记录」", () => {
    const b = new LiveBoard();
    const fs = card("s1", 7, "m1", ["你", "好"]);
    for (const f of fs.slice(0, 3)) b.apply(O, f);
    const [mid] = b.cardsOf("s1");
    expect(renderCardText(mid!)).toEqual({
      head: copyText("liveCard.state.streaming"),
      body: [copyText("liveCard.block.thinking"), "你好"].join("\n"),
    });
    for (const f of fs.slice(3)) b.apply(O, f);
    const [end] = b.cardsOf("s1");
    expect(renderCardText(end!)).toEqual({
      head: copyText("liveCard.state.awaitingRecord"),
      body: [copyText("liveCard.block.thinking"), "你好", copyText("liveCard.block.toolUse", { tool: "Read" })].join("\n"),
    });
    expect(end!.messageId).toBe("m1");
  });

  it("撤卡 ⇒ 不画；整卡（open）⇒ 整张换", () => {
    const b = new LiveBoard();
    const fs = card("s1", 7, "m1", ["a"]);
    for (const f of fs) b.apply(O, f);
    b.apply(O, { sid: "s1", card: 7, n: 0, op: "open", rid: "m1", phase: "streaming", clipped: true, blocks: [{ kind: "text", text: "尾巴" }] });
    expect(b.cardsOf("s1").map((c) => [c.clipped, c.blocks[0]!.text])).toEqual([[true, "尾巴"]]);
    b.apply(O, close("s1", 7, 1));
    expect(b.size).toBe(0);
  });

  it("`n` 接不上 ⇒ 那张先不画，之后的增量不收；下一张整卡来了照画", () => {
    const b = new LiveBoard();
    const fs = card("s1", 7, "m1", ["a", "b", "c"]);
    b.apply(O, fs[0]!);
    b.apply(O, fs[2]!); // 跳过了 n = 1
    expect(b.cardsOf("s1")).toEqual([]);
    b.apply(O, fs[3]!);
    expect(b.cardsOf("s1")).toEqual([]);
    b.apply(O, { ...(fs[0] as LiveCard & { op: "open" }), blocks: [{ kind: "text", text: "abc" }] });
    expect(b.cardsOf("s1").map((c) => c.blocks[0]!.text)).toEqual(["abc"]);
  });

  it("这台上没有这个 tab ⇒ 不留；子运行的卡不上主 tab，给 agent 面板那一行「最近」", () => {
    const b = new LiveBoard((origin, sid) => origin === O && sid === "s1");
    for (const f of card("s9", 1, "m", ["x"])) b.apply(O, f);
    for (const f of card("s1", 2, "m", ["x"])) b.apply("pi", f);
    expect(b.size).toBe(0);
    for (const f of card("s1", 3, "r", ["子"], "a1").slice(0, 2)) b.apply(O, f);
    expect(b.cardsOf("s1")).toEqual([]);
    expect(b.cardsOf("s1", "a1")).toHaveLength(1);
    expect(b.liveBlockOf("s1", "a1")).toEqual({ kind: "text", text: "子", tool: undefined });
  });

  it("那台看不见了 ⇒ 那台的卡全撤；tab 没了 ⇒ 它的卡不留", () => {
    const b = new LiveBoard();
    for (const f of card("s1", 1, "m", ["x"]).slice(0, 2)) b.apply(O, f);
    for (const f of card("s2", 2, "m", ["x"]).slice(0, 2)) b.apply("pi", f);
    expect([...b.dropOrigin("pi")]).toEqual(["s2"]);
    expect(b.dropTab("s1")).toBe(true);
    expect(b.size).toBe(0);
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

  /** 跑一遍：先建 tab（第一条 jsonl），再按给定的活卡帧喂，再喂剩下的 jsonl（随后到后端那一声撤卡）；回 jsonl 那一侧的 DOM 与中途那一刻的活卡。 */
  function run(seq: LiveCard[], closeAfter = true): { content: string; liveMidway: string; liveAfter: string } {
    installViewerRig();
    const barEl = document.createElement("div");
    const streamRootEl = document.createElement("div");
    document.body.append(barEl, streamRootEl);
    const tm = owned(new TabManager(barEl, streamRootEl));
    tm.setLivePainter(livePainter);
    tm.onLine(jsonl[0] as never);
    for (const f of seq) tm.onLiveCard(pay(f));
    const trailer = (): string =>
      [...streamRootEl.querySelectorAll(".stream")].map((s) => [...s.children].slice(1).map((c) => c.textContent).join("|")).join("#");
    const liveMidway = trailer();
    for (const p of jsonl.slice(1)) tm.onLine((p === jsonl[1] ? { ...p, rid: "msg_T8" } : p) as never);
    if (closeAfter && seq.length > 0) tm.onLiveCard(pay(close("s1", 0, seq.length)));
    const content = [...streamRootEl.querySelectorAll(".stream-content")].map((e) => e.innerHTML).join("\n");
    return { content, liveMidway, liveAfter: trailer() };
  }

  it("没有活卡 · 活卡接不上 · 活卡撤了：jsonl 建出的卡逐字相等；有活卡时中途真画出来（正控），后端撤卡之后撤干净", () => {
    const data = card("s1", 0, "msg_T8", ["答", "一句"]);
    const none = run([]);
    const whole = run(data.slice(0, 3));
    const gapped = run(data.filter((f) => f.n !== 1), false);
    const closed = run([...data.slice(0, 3), close("s1", 0, 3)], false);

    expect(none.content.length, "jsonl 那条路一张卡都没建 —— 本组空转").toBeGreaterThan(0);
    expect(whole.content).toBe(none.content);
    expect(gapped.content).toBe(none.content);
    expect(closed.content).toBe(none.content);

    // 正控：活卡真被画到了这个 tab（否则上面几条相等是空真）。
    expect(whole.liveMidway).toContain("答一句");
    expect(none.liveMidway).toBe("");
    // 接不上 ⇒ 先不画；撤卡 ⇒ 撤。
    expect(gapped.liveMidway).not.toContain("一句");
    expect(closed.liveMidway).toBe("");
    expect(whole.liveAfter).toBe("");
  });

  it("tab 按旧机器标签建出来（固定的 tab 复活）、后端宣告这个会话在另一台 ⇒ tab 的机器改成宣告的那台，那台来的流上得了活卡、标题前缀跟着变", () => {
    installViewerRig();
    const barEl = document.createElement("div");
    const streamRootEl = document.createElement("div");
    document.body.append(barEl, streamRootEl);
    const tm = owned(new TabManager(barEl, streamRootEl));
    tm.setLivePainter(livePainter);
    const trailer = (): string =>
      [...streamRootEl.querySelectorAll(".stream")].map((s) => [...s.children].slice(1).map((c) => c.textContent).join("|")).join("#");
    tm.createSkeletonTab("s9", "/w/p", "old-host"); // 盘上存的那份标签
    tm.createSkeletonTab("s9", "/w/p", "new-host"); // 后端宣告
    for (const f of card("s9", 0, "m9", ["这台", "说的"]).slice(0, 3)) tm.onLiveCard(pay(f, "new-host"));
    expect(trailer()).toContain("这台说的");
    expect(tm.snapshotSessions().map((x) => [x.sessionId, x.origin, x.title.includes("[new-host]"), x.title.includes("old-host")])).toEqual([
      ["s9", "new-host", true, false],
    ]);
    for (const f of card("s9", 1, "m10", ["旧的"]).slice(0, 2)) tm.onLiveCard(pay(f, "old-host"));
    expect(trailer()).not.toContain("旧的");
  });
});
