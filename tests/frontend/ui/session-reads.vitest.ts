/**
 * 会话读面三问（`src/frontend/ui/session-reads.ts`）的判据。
 *
 * | 性质 | 判据 |
 * |---|---|
 * | TS 解码器读得懂**后端真出的**成品 —— 同一份跨语言金样，后端那侧 `read_face_tests::the_three_products_match_the_cross_language_golden` 写它（异源：Rust 造、TS 解） | 「金样」那一条 |
 * | 形状不对 ⇒ 抛（给人看的那句不带内部名），不猜、不补默认值 | 「形状不对」那一条 |
 * | 失败种类只看通道的层，不看文字：不认 ⇒ `oldBackend` · 装不下 ⇒ `truncated` · 其余 ⇒ `transport` | 「失败种类」那一条 |
 * | 三问各自经通道说**对的帧命令、对的请求体**，本机也走同一条路（`<local>`），失败折成 `available:false`（不抛） | 「三问」那两条 |
 *
 * 买不到：真 Tauri IPC 与真后端（后端那一侧的判据在 Rust 里；monitor 那一跳由 `webview_tests` 量）。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { invoke } from "@tauri-apps/api/core";
import { ChanError } from "../../../src/comms/inward/chan";
import {
  decodeFacts,
  decodeFind,
  decodeIndex,
  decodeRecord,
  decodeUserInputs,
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

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;
const golden = JSON.parse(readFileSync(resolve(REPO_ROOT, "tests/__fixtures__/session-reads.golden.json"), "utf8")) as Record<
  string,
  unknown
>;

beforeEach(() => invokeMock.mockReset());

describe("〔C4b〕会话读面三问：按形状收", () => {
  it("★★ 金样：TS 解码器读得懂后端真出的三份成品（逐字段）", () => {
    const idx = decodeIndex(golden["history-index"]);
    expect([idx.from, idx.end, idx.rows.length]).toEqual([0, 376, 4]);
    expect(idx.rows.map((r) => [r.o, r.n, r.u])).toEqual([
      [0, 86, "in-1"],
      [86, 137, "out-1"],
      [224, 75, "meta-1"],
      [299, 77, "in-2"],
    ]);
    const ui = decodeUserInputs(golden["history-user-inputs"]);
    expect([ui.from, ui.end]).toEqual([0, 376]);
    expect(ui.entries).toEqual([
      { uuid: "in-1", excerpt: "alpha zqx beta", timestamp: "t1" },
      { uuid: "in-2", excerpt: "delta", timestamp: "t2" },
    ]);
    const f = decodeFind(golden["history-find"]);
    expect(f.total).toBe(2);
    expect(f.hits.map((h) => [h.uuid, h.kind, h.before, h.matched, h.after])).toEqual([
      ["in-1", "user", "alpha", "zqx", "beta"],
      ["out-1", "assistant", "gamma", "zqx", ""],
    ]);
  });

  it("★ 形状不对 ⇒ 抛（缺键 / 类型不对 / 条目缺字段），不补默认值", () => {
    expect(() => decodeIndex({ from: 0, end: 1 })).toThrow(/读不懂/);
    expect(() => decodeIndex({ from: 0, end: 1, rows: [{ o: 0 }] })).toThrow(/读不懂/);
    expect(() => decodeUserInputs({ from: 0, end: "1", entries: [] })).toThrow(/读不懂/);
    expect(() => decodeUserInputs({ from: 0, end: 1, entries: [{ uuid: "a", excerpt: "b" }] })).toThrow(/读不懂/);
    expect(() => decodeFind({ lines: [] })).toThrow(/读不懂/);
    expect(() => decodeFind({ total: 1, hits: [{ uuid: "a" }] })).toThrow(/读不懂/);
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
    expect(a.leftMs, "期限没带上（`X6`：调用点显式给）").toBeGreaterThan(0);
    return [a.origin, a.op, chanArgsJson(a)];
  }

  it("★★ 三问各自的帧命令与请求体；本机也走同一条路（`<local>`）；成品原样交回", async () => {
    invokeMock.mockResolvedValueOnce(chanReply(golden["history-index"]));
    const idx = await readSessionIndex("<local>", "/p/s.jsonl", 7);
    expect(sent()).toEqual(["<local>", "history-index", { path: "/p/s.jsonl", offset: 7 }]);
    expect([idx.available, idx.end, idx.rows.length]).toEqual([true, 376, 4]);

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
    expect(a.reason).toContain("后端版本旧");

    invokeMock.mockReset().mockRejectedValueOnce(refusedReply("failed", "past EOF"));
    const b = await listUserInputs("devbox", "/p/s.jsonl", 5);
    expect([b.available, b.failure]).toEqual([false, "transport"]);
    expect(b.reason).toContain("past EOF");

    invokeMock.mockReset().mockRejectedValueOnce(refusedReply("too_large", "big"));
    expect((await listUserInputs("devbox", "/p/s.jsonl", 0)).failure).toBe("truncated");

    invokeMock.mockReset().mockRejectedValueOnce(NO_CHANNEL);
    const c = await readSessionIndex("devbox", "/p/s.jsonl", 0);
    expect([c.available, c.rows]).toEqual([false, []]);
    expect(c.reason).toContain("够不着");

    invokeMock.mockReset().mockResolvedValueOnce(chanReply({ lines: [] }));
    const d = await findInSession("devbox", "/p/s.jsonl", "q", false);
    expect([d.available, d.hits, d.total]).toEqual([false, [], 0]);
    expect(d.reason).toContain("读不懂");
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
  it("★★ 缺一格 / 多一格 / 类型不对 ⇒ 抛 —— **绝不**读成「不在」（`present:false` 会把接得上的 resume 拦掉）", () => {
    for (const bad of [
      { root: "/r" },
      { present: false },
      { present: "no", root: "/r" },
      { present: false, root: "/r", extra: 1 },
      { lines: [] },
    ]) {
      expect(() => decodeRecord(bad), JSON.stringify(bad)).toThrow();
    }
    expect(decodeRecord({ present: true, root: "/r" })).toEqual({ present: true, root: "/r" });
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
  it("★★ 金样：TS 解码器读得懂后端真出的会话事实（逐字段）", () => {
    const f = decodeFacts(golden["history-facts"]);
    expect(f).toEqual({
      end: 729,
      forkedFrom: "src-0",
      touchedFiles: ["/w/a.ts"],
      usage: { promptTokens: 6, model: "m-g" },
    });
  });

  it("★ 形状不对 ⇒ 抛：缺一格 / 多一格 / 类型不对（成品要原样当令牌交回去，不能收一份后端不认的）", () => {
    const good = golden["history-facts"] as Record<string, unknown>;
    const without = (k: string) => Object.fromEntries(Object.entries(good).filter(([x]) => x !== k));
    expect(() => decodeFacts(without("usage"))).toThrow(/读不懂/);
    expect(() => decodeFacts({ ...good, extra: 1 })).toThrow(/读不懂/);
    expect(() => decodeFacts({ ...good, end: "729" })).toThrow(/读不懂/);
    expect(() => decodeFacts({ ...good, touchedFiles: [1] })).toThrow(/读不懂/);
    expect(() => decodeFacts({ ...good, usage: { promptTokens: 1 } })).toThrow(/读不懂/);
    expect(decodeFacts({ ...good, usage: null, forkedFrom: null }).usage).toBeNull(); // null 是合法的「没有」
  });

  it("★ 说对的帧命令、对的请求体：没有令牌 ⇒ 只带 path；有 ⇒ 令牌原样放进 prior；失败折成 available:false ＋ 种类", async () => {
    const product = decodeFacts(golden["history-facts"]);
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
    expect(chanArgsJson(a2)).toEqual({ path: "/p/s.jsonl", prior: golden["history-facts"] });

    invokeMock.mockRejectedValueOnce(UNSUPPORTED);
    const r3 = await readSessionFacts("pi", "/p/s.jsonl", null);
    expect(r3.available).toBe(false);
    expect(r3.available === false && r3.failure).toBe("oldBackend");
    expect(r3.available === false && r3.reason).toMatch(/后端版本旧/);

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
    const { decodePage, decodeLines, decodeRun } = await import("../../../src/frontend/ui/record-reads");
    const page = decodePage("<local>", recordGolden["history-page"]);
    expect([page.next, page.nextSeq, page.eof]).toEqual([273, 3, true]);
    expect(page.payloads.map((p) => [p.seq, p.session_id, p.cwd, (p.message as { uuid?: string }).uuid])).toEqual([
      [1, "r", "/w", "r-1"],
      [2, "r", null, "r-2"],
    ]);
    expect(page.payloads.every((p) => p.origin === undefined)).toBe(true);
    const lines = decodeLines("devbox", recordGolden["history-lines"]);
    expect([lines.from, lines.next, lines.eof]).toEqual([1, 3, true]);
    expect(lines.payloads.map((p) => [p.seq, p.origin])).toEqual([
      [1, "devbox"],
      [2, "devbox"],
    ]);
    const run = decodeRun(recordGolden["history-run"]);
    expect([run.run, run.rows.length, run.rows.map((r) => r.rid ?? null), run.more]).toEqual(["a1", 2, [null, "m-s2"], false]);
    // 反向：外层多一格 ⇒ 不收（两端契约对不上，不猜）。
    expect(() => decodePage("<local>", { ...(recordGolden["history-page"] as object), extra: 1 })).toThrow();
  });
});
