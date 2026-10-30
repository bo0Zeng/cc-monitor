/**
 * 活卡的判据（界面那一半：照壳交来的整份画）。
 *
 * 对账 · 缺口 · 记录到了整轮覆盖 · 子运行归位 · 截头 · 挤谁都在后端（`src/backend/stream/live.rs`，判据 `tests/backend/stream/live_tests.rs`）；
 * 一帧一帧折成整份在端核心（`src/comms/resident-client/live.rs`，判据 `tests/comms/resident-client/live_tests.rs`）。
 * 这里只钉：整份照画；空的一格 ⇒ 那一处撤；没这个 tab ⇒ 不留；主 tab 只画主运行；那台看不见了 ⇒ 全撤。
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

import { LiveCards } from "../../../src/frontend/ui/live-card";
import { installViewerRig, line, userLine, type RigPayload } from "../../test-support/session-viewer-rig";
import { TabManager } from "../../../src/frontend/ui/tabs";
import { livePainter } from "../../../src/frontend/ui/live-card-view";
import type { LiveCardNow } from "../../../src/frontend/ui/generated/LiveCardNow";
import type { SessionLiveCardsPayload } from "../../../src/frontend/ui/generated/SessionLiveCardsPayload";
import { nonEmpty } from "../../test-support/non-empty.ts";

// ─── 手写的整份（期望只从这里来） ─────────────────────────────────────────

const O = "<local>";

/** 一张卡此刻的样子：思考块 ＋ 正文块（＋ 工具块）；头一行 · 正文 · 「最近」那一截是核心写好的（这里是结构占位）。 */
function now(card: number, rid: string, text: string, phase: "streaming" | "awaiting" = "streaming", tool?: string): LiveCardNow {
  return {
    card,
    rid,
    phase,
    clipped: false,
    blocks: [{ kind: "thinking" }, { kind: "text", text }, ...(tool ? [{ kind: "tool" as const, tool }] : [])],
    head: `核心·头·${phase}`,
    body: `核心·正文·${text}`,
    ...(phase === "streaming" ? { last: `核心·最近·${rid}` } : {}),
  };
}

/** 壳交来的一格：那台那一处此刻整份的卡。 */
const pay = (sid: string, cards: LiveCardNow[], origin = O, run?: string): SessionLiveCardsPayload => ({
  origin,
  live: { sid, ...(run ? { run } : {}), cards },
});

const board = (route: (origin: string, sid: string) => boolean = () => true): LiveCards => new LiveCards(route, () => null);

describe("活卡照画（期望手写）", () => {
  it("★ 一张卡：头一行 · 正文照核心写好的摆（不按块的种类取字）", () => {
    const b = board();
    b.onCards(pay("s1", [now(7, "m1", "你好", "awaiting", "Read")]));
    const host = document.createElement("div");
    livePainter.cards(host, b.cardsOf("s1"));
    expect(host.textContent).toBe("核心·头·awaiting核心·正文·你好");
    expect(b.cardsOf("s1")[0]!.messageId).toBe("m1");
  });

  it("每一格换掉那一处手上那一份；空的一格 ⇒ 那一处不画", () => {
    const b = board();
    b.onCards(pay("s1", [now(7, "m1", "a")]));
    b.onCards(pay("s1", [now(7, "m1", "尾巴")]));
    expect(b.cardsOf("s1").map((c) => c.body)).toEqual(["核心·正文·尾巴"]);
    b.onCards(pay("s1", []));
    expect(b.size).toBe(0);
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
    for (const p of seq) tm.onLiveCard(p);
    const trailer = (): string =>
      [...streamRootEl.querySelectorAll(".stream")].map((s) => [...s.children].slice(1).map((c) => c.textContent).join("|")).join("#");
    const liveMidway = trailer();
    for (const p of jsonl.slice(1)) tm.onLine((p === jsonl[1] ? { ...p, rid: "msg_T8" } : p) as never);
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
