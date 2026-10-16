/**
 * 会话读面三问（`src/frontend/ui/session-reads.ts`）的判据。
 *
 * | 性质 | 判据 |
 * |---|---|
 * | **后端真出的**成品就是生成的那几个类型 —— 同一份跨语言金样，后端那侧 `read_face_tests::the_three_products_match_the_cross_language_golden` 写它（异源：Rust 造、TS 读）；形状由握手指纹核 | 「金样」那一条 |
 * | 会话事实的令牌原样交回（只去掉本机记的收到时刻） | 「需手动记上本机收到的时刻」那一条 |
 * | 失败种类只看通道的层，不看文字：不认 ⇒ `oldBackend` · 装不下 ⇒ `truncated` · 其余 ⇒ `transport` | 「失败种类」那一条 |
 * | 三问各自经通道说**对的帧命令、对的请求体**，本机也走同一条路（`<local>`），失败折成 `available:false`（不抛） | 「三问」那两条 |
 *
 * 买不到：真 Tauri IPC 与真后端（后端那一侧的判据在 Rust 里；monitor 那一跳由 `webview_tests` 量）。
 */
import type { Branch } from "../../../src/frontend/ui/generated/Branch";
import type { HistoryLines } from "../../../src/frontend/ui/generated/HistoryLines";
import type { HistoryPage } from "../../../src/frontend/ui/generated/HistoryPage";
import type { HistoryRun } from "../../../src/frontend/ui/generated/HistoryRun";
import type { HistoryFind } from "../../../src/frontend/ui/generated/HistoryFind";
import type { HistoryIndex } from "../../../src/frontend/ui/generated/HistoryIndex";
import type { HistoryTurns } from "../../../src/frontend/ui/generated/HistoryTurns";
import type { UserInputs } from "../../../src/frontend/ui/generated/UserInputs";
import type { SessionFacts as WireFacts } from "../../../src/frontend/ui/generated/SessionFacts";
import type { NeedsAsk } from "../../../src/frontend/ui/generated/NeedsAsk";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { invoke } from "@tauri-apps/api/core";
import { ChanError } from "../../../src/comms/inward/chan";
import {
  readTurns,
  factsOf,
  unpackIndex,
  failureOf,
  findInSession,
  FIND_LIMIT,
  listUserInputs,
  probeSessionRecord,
  readSessionFacts,
  readSessionIndex,
} from "../../../src/frontend/ui/session-reads";
import { REPO_ROOT } from "../../test-support/repo-root";
import { chanArgsJson, chanReply, refusedReply, UNSUPPORTED, NO_CHANNEL, type ChanCallArgs } from "../../test-support/chan-fake";
import { copyText } from "../../../src/frontend/ui/copy-table";
import { copyPattern } from "../../test-support/copy-pattern";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;
const golden = JSON.parse(readFileSync(resolve(REPO_ROOT, "tests/__fixtures__/session-reads.golden.json"), "utf8")) as Record<
  string,
  unknown
>;

beforeEach(() => invokeMock.mockReset());

describe("〔C4b〕会话读面三问：按形状收", () => {
  it("★★ 金样：后端真出的三份成品就是生成的那几个类型（索引装回逐行）", () => {
    const index = golden["history-index"] as HistoryIndex;
    const idx = { from: index.from, end: index.end, rows: unpackIndex(index) };
    expect([idx.from, idx.end, idx.rows.length, index.next]).toEqual([0, 537, 4, null]);
    expect(idx.rows.map((r) => [r.o, r.n, r.u, r.t])).toEqual([
      [0, 100, "in-1", "said"],
      [100, 194, "out-1", "reply"],
      [295, 129, "meta-1", "said"],
      [424, 113, "in-2", "said"],
    ]);
    const ui = golden["history-user-inputs"] as UserInputs;
    expect([ui.from, ui.end, ui.next]).toEqual([0, 537, null]);
    expect(ui.entries).toEqual([
      { at: 0, uuid: "in-1", excerpt: "alpha zqx beta", timestamp: "t1" },
      { at: 424, uuid: "in-2", excerpt: "delta", timestamp: "t2" },
    ]);
    const f = golden["history-find"] as HistoryFind;
    expect(f.total).toBe(2);
    expect(f.hits.map((h) => [h.uuid, h.kind, h.before, h.matched, h.after, h.turn, h.tsMs])).toEqual([
      ["in-1", "user", "alpha", "zqx", "beta", 1, 0],
      ["out-1", "assistant", "gamma", "zqx", "", 1, 0],
    ]);
  });

  it("★★ 失败种类只看通道的层：不认 ⇒ oldBackend · 装不下 ⇒ truncated · 其余 ⇒ transport", () => {
    const body = (code: string) => new TextEncoder().encode(JSON.stringify({ code, message: "m" }));
    expect(failureOf({ layer: "peer", why: "unsupported" })).toBe("oldBackend");
    expect(failureOf({ layer: "peer", why: "refused", body: body("too_large") })).toBe("truncated");
    expect(failureOf({ layer: "peer", why: "refused", body: body("failed") })).toBe("transport");
    expect(failureOf({ layer: "peer", why: "refused", body: new Uint8Array([0xff]) })).toBe("transport");
    expect(failureOf({ layer: "hop", at: { idx: 1, tag: "open" }, reach: "NotSent", why: "Unreachable" })).toBe("transport");
    expect(failureOf({ layer: "ours", why: "Cancelled" })).toBe("transport");
  });
});

describe("〔C4b〕会话读面三问：经通道说对的帧命令", () => {
  /** 这一趟唯一那一发 `chan_call` 的 `(origin, op, 请求体)`。 */
  function sent(): [string, string, unknown] {
    const calls = invokeMock.mock.calls;
    expect(calls.map((c) => c[0] as string)).toEqual(["chan_call"]);
    const a = calls[0][1] as ChanCallArgs;
    return [a.origin, a.op, chanArgsJson(a)];
  }

  it("★★ 三问各自的帧命令与请求体；本机也走同一条路（`<local>`）；成品原样交回", async () => {
    invokeMock.mockResolvedValueOnce(chanReply(golden["history-index"]));
    const idx = await readSessionIndex("<local>", "/p/s.jsonl", 7);
    expect(sent()).toEqual(["<local>", "history-index", { path: "/p/s.jsonl", offset: 7 }]);
    expect([idx.available, idx.end, idx.rows.length]).toEqual([true, 537, 4]);

    invokeMock.mockReset().mockResolvedValueOnce(chanReply(golden["history-user-inputs"]));
    const ui = await listUserInputs("devbox", "/p/s.jsonl", 42);
    expect(sent()).toEqual(["devbox", "history-user-inputs", { path: "/p/s.jsonl", from: 42 }]);
    expect([ui.available, ui.failure, ui.entries.length]).toEqual([true, undefined, 2]);

    invokeMock.mockReset().mockResolvedValueOnce(chanReply(golden["history-find"]));
    const f = await findInSession("devbox", "/p/s.jsonl", "--force", true);
    expect(sent()).toEqual([
      "devbox",
      "history-find",
      { path: "/p/s.jsonl", query: "--force", include_tools: true, limit: FIND_LIMIT },
    ]);
    expect([f.available, f.total, f.hits.length]).toEqual([true, 2, 2]);
  });

  it("★ 失败一律折成 `available:false`（不抛）：种类与原因按层给；对端说「不行」时原因原样带上", async () => {
    invokeMock.mockRejectedValueOnce(UNSUPPORTED);
    const a = await listUserInputs("devbox", "/p/s.jsonl", 5);
    expect([a.available, a.failure, a.from, a.end, a.entries]).toEqual([false, "oldBackend", 5, 5, []]);
    expect(a.reason).toMatch(copyPattern("peerVersion.said.old"));

    invokeMock.mockReset().mockRejectedValueOnce(refusedReply("failed", "past EOF"));
    const b = await listUserInputs("devbox", "/p/s.jsonl", 5);
    expect([b.available, b.failure]).toEqual([false, "transport"]);
    expect(b.reason).toContain("past EOF");

    invokeMock.mockReset().mockRejectedValueOnce(refusedReply("too_large", "big"));
    expect((await listUserInputs("devbox", "/p/s.jsonl", 0)).failure).toBe("truncated");

    invokeMock.mockReset().mockRejectedValueOnce(NO_CHANNEL);
    const c = await readSessionIndex("devbox", "/p/s.jsonl", 0);
    expect([c.available, c.rows]).toEqual([false, []]);
    expect(c.reason).toContain(copyText("chanCaller.said.unreachable"));

  });

  it("正控：`ChanError` 是经通道那一层抛出来的那一个（折叠认得它）", async () => {
    invokeMock.mockRejectedValueOnce(UNSUPPORTED);
    const { chan } = await import("../../../src/comms/inward/chan");
    await expect(chan.call("devbox", "history-index", new Uint8Array(), { until: performance.now() + 1000 })).rejects.toBeInstanceOf(
      ChanError,
    );
  });
});

// 第四问：resume 之前问记录还在不在（`history-record`）。最后一条
//   「对方那份记录也没了 ⇒ 重开必失败，要诚实报错，不许静默变成『起了个新会话』」。
describe("〔C4c〕记录还在不在：经通道问 `history-record`", () => {
  it("★★ 帧命令与请求体（只收 sid）；本机也走同一条路；成品原样交回", async () => {
    invokeMock.mockResolvedValueOnce(chanReply({ present: false, root: "/h/.claude/projects" }));
    expect(await probeSessionRecord("<local>", "s-1")).toEqual({ present: false, root: "/h/.claude/projects" });
    const a = invokeMock.mock.calls[0][1] as ChanCallArgs;
    expect([invokeMock.mock.calls[0][0], a.origin, a.op, chanArgsJson(a)]).toEqual([
      "chan_call",
      "<local>",
      "history-record",
      { sid: "s-1" },
    ]);
  });
  it("★ 问不到（没有控制通道 / 后端不认）⇒ 抛（调用方当「不知道」），不折成一个答案", async () => {
    invokeMock.mockRejectedValueOnce(NO_CHANNEL);
    await expect(probeSessionRecord("devbox", "s-1")).rejects.toBeInstanceOf(ChanError);
    invokeMock.mockRejectedValueOnce(UNSUPPORTED);
    await expect(probeSessionRecord("devbox", "s-1")).rejects.toBeInstanceOf(ChanError);
  });
});

// 第五问：会话事实（`history-facts`）。金样同一份文件的 `history-facts` 一格，
// 后端那侧 `read_face_tests::the_three_products_match_the_cross_language_golden` 写它（异源：Rust 造、TS 解）。
describe("〔STC〕第五问：会话事实", () => {
  const wire = golden["history-facts"] as WireFacts;

  it("★★ 金样：后端真出的会话事实就是生成的那个类型（逐字段）", () => {
    const f = factsOf(wire);
    expect(f).toEqual({
      end: 3131,
      rev: 1,
      forkedFrom: "src-0",
      touchedFiles: ["/w/a.ts"],
      usage: {
        promptTokens: 6,
        model: "m-g",
        peakPromptTokens: 6,
        limit: 1_000_000,
        limitFrom: "assumed",
        percent: null,
        contextText: "6",
        contextTone: "plain",
        promptTokensText: "6",
        limitText: "1M",
        limitFromText: null,
      },
      agent: "claude",
      projectDir: "/g/proj",
      writers: [],
      pending: [
        { id: "tu-1", name: "Edit", what: "/w/a.ts", at: "t3", atMs: null, state: "unclear", why: "noWriter", text: copyText("stream.step.unclear"), whyText: copyText("stream.step.unclearNoWriter") },
        { id: "tu-3", name: "Agent", what: null, at: "t4", atMs: null, state: "unclear", why: "noWriter", text: copyText("stream.step.unclear"), whyText: copyText("stream.step.unclearNoWriter") },
      ],
      lastSay: { text: "done", at: "t5", atMs: null },
      needs: null,
      handedBack: ["ag-7"],
      retries: [
        { id: "rt-1", outcome: "recovered" },
        { id: "rt-2", outcome: "retrying" },
      ],
      permissionMode: "acceptEdits",
      permissionModeText: copyText("beAsk.choice.planEdits"),
      modelText: null,
      tokens: {
        input: 1,
        output: 0,
        cacheRead: 3,
        cacheWrite5m: 2,
        cacheWrite1h: 0,
        requests: 1,
        text: copyText("beSpend.tokens.line", { input: "1", output: "0", read: "3", write: "2" }),
        last: { id: "", tokens: [1, 0, 3, 2, 0] },
      },
      cost: { micros: 424200, partial: false, text: copyText("beSpend.cost.exact", { usd: "0.42" }) },
      bgTasks: [{ call: "tu-b1", task: "bb1", cmd: "make test-all", at: "2026-10-09T08:00:00.000Z" }],
      background: null,
      mcp: [
        { name: "m-f", status: "failed", detail: "e-f", at: null },
        { name: "m-n", status: "needsLogin", detail: null, at: null },
      ],
      answerOff: null,
      autoAnswer: null,
      autoAnswered: null,
      startedBy: null,
      children: [],
      quietUnread: { count: 1, kinds: ["golden-future-ledger"], text: copyText("record.unread.quiet", { n: "1", types: "golden-future-ledger" }) },
    });
  });

  it("★ 需手动记上本机收到的时刻（不在线上）", () => {
    const needs: WireFacts["needs"] = { kind: "approve", tool: "Bash", call: "toolu_1", what: "rm -rf build/", sinceMs: 42, text: copyText("beSession.needs.approve"), head: "等批准 · Bash", headCode: "rm -rf build/", tone: "need", rank: 1, waitedMs: 1000, waitedText: "1 秒", ask: null, manualText: copyText("beAsk.manual.terminal") };
    const f = factsOf({ ...wire, needs }, 777);
    expect(f.needs).toEqual({ ...needs, receivedAt: 777 });
  });

  it("★★ 答题口的成品（needs.ask）：后端真出的四种（跨语言金样 needs-ask.golden.json）原样收下", () => {
    const asks = JSON.parse(readFileSync(resolve(REPO_ROOT, "tests/__fixtures__/needs-ask.golden.json"), "utf8")) as Record<string, NeedsAsk>;
    const needs = { kind: "approve", tool: "Bash", call: "toolu_1", what: "w", sinceMs: 42, text: "t", head: "h", headCode: null, tone: "need", rank: 1, waitedMs: 1, waitedText: "1s", manualText: null } as const;
    const read = (ask: NeedsAsk | null) => factsOf({ ...wire, needs: { ...needs, ask } }).needs!.ask;
    expect(Object.keys(asks).sort()).toEqual(["approve", "plan", "question", "subAgent"]);
    for (const [name, a] of Object.entries(asks)) expect(read(a), name).toEqual(a);
    expect(read(asks.subAgent)?.run).toEqual({ id: "a1", kind: "general-purpose" });
    expect(read(asks.question)?.questions[0].options[0].detail).toBe("warm");
    expect(read(null), "没有挂着的").toBeNull();
  });

  it("★ 说对的帧命令、对的请求体：没有令牌 ⇒ 只带 path；有 ⇒ prior 只带上一份的 {end, rev}；失败折成 available:false ＋ 种类", async () => {
    const product = factsOf(wire);
    invokeMock.mockResolvedValueOnce(chanReply(golden["history-facts"]));
    const r1 = await readSessionFacts("<local>", "/p/s.jsonl", null);
    expect(r1).toEqual({ available: true, facts: product });
    const a1 = invokeMock.mock.calls[0][1] as ChanCallArgs;
    expect([a1.origin, a1.op]).toEqual(["<local>", "history-facts"]);
    expect(chanArgsJson(a1)).toEqual({ path: "/p/s.jsonl" });

    invokeMock.mockResolvedValueOnce(chanReply(golden["history-facts"]));
    await readSessionFacts("pi", "/p/s.jsonl", product);
    const a2 = invokeMock.mock.calls[1][1] as ChanCallArgs;
    expect(a2.origin).toBe("pi");
    expect(chanArgsJson(a2)).toEqual({ path: "/p/s.jsonl", prior: { end: wire.end, rev: wire.rev } });
    invokeMock.mockClear();

    invokeMock.mockRejectedValueOnce(UNSUPPORTED);
    const r3 = await readSessionFacts("pi", "/p/s.jsonl", null);
    expect(r3.available).toBe(false);
    expect(r3.available === false && r3.failure).toBe("oldBackend");
    expect(r3.available === false && r3.reason).toMatch(copyPattern("peerVersion.said.old"));

    invokeMock.mockRejectedValueOnce(refusedReply("failed", "past EOF"));
    const r4 = await readSessionFacts("pi", "/p/s.jsonl", product);
    expect(r4.available === false && r4.failure).toBe("transport");
  });
});

// 会话正文那几条：TS 解码器读同一份跨语言金样（后端 `read_face_tests.rs` 钉着它 == 帧面现打）。
describe("〔MOD〕会话正文：按形状收那台后端出的成品", () => {
  const recordGolden = JSON.parse(
    readFileSync(resolve(REPO_ROOT, "tests/__fixtures__/record-reads.golden.json"), "utf8"),
  ) as Record<string, unknown>;

  it("★★ 金样：整份读 · 按行号 · 一个子运行（按运行读）三份成品读得懂；远端那台的载荷带上它的名字、本机不带", async () => {
    const { pageOf, linesOf } = await import("../../../src/frontend/ui/record-reads");
    const page = pageOf("<local>", recordGolden["history-page"] as HistoryPage);
    expect([page.next, page.nextSeq, page.eof]).toEqual([273, 3, true]);
    expect(page.payloads.map((p) => [p.seq, p.session_id, p.cwd, p.record.id])).toEqual([
      [1, "r", "/w", "r-1"],
      [2, "r", null, "r-2"],
    ]);
    expect(page.payloads.every((p) => p.origin === undefined)).toBe(true);
    const lines = linesOf("devbox", recordGolden["history-lines"] as HistoryLines);
    expect([lines.from, lines.next, lines.eof]).toEqual([1, 3, true]);
    expect(lines.payloads.map((p) => [p.seq, p.origin])).toEqual([
      [1, "devbox"],
      [2, "devbox"],
    ]);
    const run = recordGolden["history-run"] as HistoryRun;
    expect([run.run, run.rows.length, run.rows.map((r) => r.rid ?? null), run.more]).toEqual(["a1", 2, [null, "m-s2"], false]);
  });

  it("★★ 金样：主线外清单（history-branch）的成品就是生成的那个类型", () => {
    // 金样那份会话两条都没有链（r-2 不接 r-1）⇒ 两个根、r-1 是不接任何回复的人那一句 ⇒ 后端判它回退掉了。
    const branch: Branch = recordGolden["history-branch"] as Branch;
    expect(branch).toEqual({ off: ["r-1"], end: 273, next: null });
  });
});

// 第六问：每轮的摘要（`history-turns`）。金样同一份文件的 `history-turns` 一格（异源：Rust 造、TS 解）。
describe("主窗口第 2 批：每轮的摘要", () => {
  it("★★ 金样：两轮 —— 第一轮收尾（思考 1 · 中间的话 · 调用 2 · 失败 1〔被拒不算〕· 结论 · 回复头三行 · 两段过程各自的字与语气），第二轮还在跑", () => {
    const t = golden["history-turns"] as HistoryTurns;
    expect(t.from).toBe(0);
    expect(t.turns.map((x) => [x.uuid, x.said, x.tools, x.thinking, x.agents, x.background, x.retries, x.peers, x.fails, x.shown, x.reply, x.done, x.phase])).toEqual([
      ["t-1", "first line", 2, 1, 0, 0, 0, 0, 1, ["t-3", "t-8"], "r1\nr2\nr3", true, "idle"],
      ["t-9", "next", 1, 0, 0, 0, 0, 0, 0, [], "", false, "idle"],
    ]);
    expect(t.turns[0].folds.map((f) => [f.after, f.parts.map((p) => p.tone), f.phase])).toEqual([
      ["t-1", ["plain", "plain"], "idle"],
      ["t-3", ["plain", "plain", "fail"], "idle"],
    ]);
    expect(t.turns[0].folds[0].span).toEqual({ text: "", from: null, to: null });
    expect(t.turns[1].at).toBeGreaterThan(0);
  });

  it("★ 要不到 ⇒ available:false（不猜）；入参照类型发", async () => {
    invokeMock.mockReset().mockResolvedValueOnce(chanReply(golden["history-turns"]));
    const ok = await readTurns("devbox" as never, "/p/s.jsonl", 0);
    expect(ok.available && ok.turns.length).toBe(2);
    const a = invokeMock.mock.calls[0][1] as ChanCallArgs;
    expect([a.origin, a.op, chanArgsJson(a)]).toEqual(["devbox", "history-turns", { path: "/p/s.jsonl", from: 0 }]);
    invokeMock.mockReset().mockRejectedValueOnce(refusedReply("failed", "m"));
    expect((await readTurns("devbox" as never, "/p/s.jsonl", 0)).available).toBe(false);
  });
});

// 8a b：那台按页给（一页有字节上限，后端 `history_face.rs::Pager`）——界面照 `next` 一页一页接着问、拼成一份；
// 骨架索引给了 `want` 只要前那么多行（每页 `limit` ＝ 还差几行，够了就停）。
describe("8a b：分页取", () => {
  const calls = (): Array<[string, unknown]> =>
    invokeMock.mock.calls.map((c) => {
      const a = c[1] as ChanCallArgs;
      return [a.op, chanArgsJson(a)];
    });

  it("★ 轮次 · 用户输入照 next 接着问，拼成一份；end 取最后一页的", async () => {
    const t = golden["history-turns"] as HistoryTurns;
    invokeMock
      .mockResolvedValueOnce(chanReply({ ...t, turns: [t.turns[0]], end: t.turns[1].at, next: t.turns[1].at }))
      .mockResolvedValueOnce(chanReply({ ...t, from: t.turns[1].at, turns: [t.turns[1]], next: null }));
    const got = await readTurns("devbox" as never, "/p/s.jsonl", 0);
    expect(calls()).toEqual([
      ["history-turns", { path: "/p/s.jsonl", from: 0 }],
      ["history-turns", { path: "/p/s.jsonl", from: t.turns[1].at }],
    ]);
    expect(got.available && [got.from, got.end, got.turns.map((x) => x.uuid)]).toEqual([0, t.end, ["t-1", "t-9"]]);

    invokeMock.mockReset();
    const ui = golden["history-user-inputs"] as UserInputs;
    invokeMock
      .mockResolvedValueOnce(chanReply({ ...ui, entries: [ui.entries[0]], end: 100, next: 100 }))
      .mockResolvedValueOnce(chanReply({ ...ui, from: 100, entries: [ui.entries[1]], next: null }));
    const u = await listUserInputs("devbox", "/p/s.jsonl", 0);
    expect(calls().map((c) => c[1])).toEqual([
      { path: "/p/s.jsonl", from: 0 },
      { path: "/p/s.jsonl", from: 100 },
    ]);
    expect([u.end, u.entries.length]).toEqual([ui.end, 2]);
  });

  it("★ 骨架索引：照 next 接着问；给了 want ⇒ 每页 limit 是还差几行，够了就停（不问到底）", async () => {
    const index = golden["history-index"] as HistoryIndex;
    const page = (from: number, next: number | null): HistoryIndex => ({ ...index, from, next });
    invokeMock.mockResolvedValueOnce(chanReply(page(0, 300))).mockResolvedValueOnce(chanReply(page(300, null)));
    const all = await readSessionIndex("<local>", "/p/s.jsonl", 0);
    expect(calls().map((c) => c[1])).toEqual([
      { path: "/p/s.jsonl", offset: 0 },
      { path: "/p/s.jsonl", offset: 300 },
    ]);
    expect(all.rows.length).toBe(2 * index.count);

    invokeMock.mockReset();
    invokeMock.mockResolvedValueOnce(chanReply(page(0, 300))).mockResolvedValueOnce(chanReply(page(300, 600)));
    const some = await readSessionIndex("<local>", "/p/s.jsonl", 0, index.count + 1);
    expect(calls().map((c) => c[1])).toEqual([
      { path: "/p/s.jsonl", offset: 0, limit: index.count + 1 },
      { path: "/p/s.jsonl", offset: 300, limit: 1 },
    ]);
    expect(some.rows.length).toBeGreaterThanOrEqual(index.count + 1);
  });
});
