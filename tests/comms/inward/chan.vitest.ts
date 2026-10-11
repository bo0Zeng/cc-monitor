/**
 * 通道在 webview 手里那一半（`src/comms/inward/chan.ts`）的判据。
 *
 * | 性质 | 判据 |
 * |---|---|
 * | 三层错误按 monitor 交回的线上形状解回 —— 与 Rust 那侧打的**同一份金标准**对拍（两侧异源：Rust 造、TS 解） | 「金标准」那一条 |
 * | 期限过了一个字节都不发（`Hop{0 write, NotSent, Overrun}`）；过线的是「还剩多少」 | 「期限」那两条 |
 * | 本地撤单立即回（`Ours{Cancelled}`），不等 monitor | 「撤单」那两条 |
 * | 载荷两个方向逐字节原样（含非 UTF-8 字节） | 「载荷」那一条 |
 * | 解不出来的拒绝一律 `ours/Broken`，不猜 | 「解不出」那一条 |
 * | 成员本身不解释载荷：生产段零 `JSON.parse` / `JSON.stringify`（正控：调用方那一侧的同一个识别器命中） | 最后一条 |
 *
 * `subscribe` 那一半（末尾「S5」那一组）：
 *
 * | 性质 | 判据 |
 * |---|---|
 * | 每种格按 monitor 交回的线上形状解回 —— 与 Rust 那侧打的**同一份金标准**对拍 | 「S5 金标准」 |
 * | 编号本侧给、先记 sink 再登记；不认识的编号 / 撤了之后到的格一格都不交 | 「S5 编号」 |
 * | `want` / `stop` 各恰好一次 IPC（撤了之后 `want` 不再发） | 「S5 往回说」 |
 * | 解不出的一格 ⇒ 交 `closed{ours: Broken}` 并撤掉，不猜 | 「S5 解不出」 |
 * | 登记那一跳失败 ⇒ 原位 `closed{ours: Broken}`，`subscribe` 本身不抛（`§3.3.5`） | 「S5 不失败」 |
 * | 报信用那一跳失败 ⇒ 同样原位 `closed{ours: Broken}` 并撤掉（原先被吞 ⇒ 静默停流） | 「S5 信用报不上去」 |
 *
 * 买不到：真 Tauri IPC 那一跳（要一个活的 webview）—— 这里 mock 的是 `invoke`，
 * 它之后的那一跳由 Rust 侧 `tests/frontend/shell/chan/webview_tests.rs` 用合成句柄 ＋ 真 `router::settle` 量。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  Channel: class {
    onmessage: ((v: unknown) => void) | null = null;
  },
}));
// 交格事件按窗口作用域听：记下那个回调，判据往里投递。
const itemsListeners: Array<(e: { payload: unknown }) => void> = [];
vi.mock("@tauri-apps/api/webviewWindow", () => ({
  getCurrentWebviewWindow: () => ({
    listen: vi.fn((event: string, cb: (e: { payload: unknown }) => void) => {
      if (event === "chan-items") itemsListeners.push(cb);
      return Promise.resolve(() => {});
    }),
  }),
}));

import { invoke } from "@tauri-apps/api/core";
import { ChanError, chan, decodeFail, decodeItem, remaining, type CallError, type Item } from "../../../src/comms/inward/chan";
import { budgetWithin, saidFrom } from "../../../src/frontend/ui/ipc/chan-caller";
import { REPO_ROOT } from "../../test-support/repo-root";
import { stripComments } from "../../test-support/strip-comments";
import { copyText } from "../../../src/frontend/ui/copy-table";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;

async function failOf(p: Promise<unknown>): Promise<CallError> {
  try {
    await p;
  } catch (e) {
    expect(e, "失败必须是 ChanError（分好层的那一个）").toBeInstanceOf(ChanError);
    return (e as ChanError).error;
  }
  throw new Error("期望失败，却成功了");
}

beforeEach(() => {
  invokeMock.mockReset();
});

describe("〔C4a〕webview 通道客户端", () => {
  it("★★ 金标准：Rust 造的那四种失败形状，TS 逐个解回 `§3.3.1` 的三层", () => {
    const golden = JSON.parse(
      readFileSync(resolve(REPO_ROOT, "tests/__fixtures__/chan-webview-fail.golden.json"), "utf8"),
    ) as Record<string, unknown>;
    expect(Object.keys(golden).sort(), "金标准的档数变了 —— 两侧要同拍改").toEqual(
      ["hop", "ours", "refused", "unsupported"],
    );
    expect(decodeFail(golden.hop)).toEqual({
      layer: "hop",
      at: { idx: 1, tag: "wait" },
      reach: "Unknown",
      why: "Overrun",
    });
    expect(decodeFail(golden.unsupported)).toEqual({ layer: "peer", why: "unsupported" });
    const refused = decodeFail(golden.refused);
    expect(refused.layer === "peer" && refused.why === "refused").toBe(true);
    if (refused.layer === "peer" && refused.why === "refused") {
      expect(new TextDecoder().decode(refused.body)).toBe('{"code":"no"}');
    }
    expect(decodeFail(golden.ours)).toEqual({ layer: "ours", why: "Misuse" });
  });

  it("★ 期限已过：一个字节都不发，答 `Hop{0 write, NotSent, Overrun}`", async () => {
    const e = await failOf(chan.call("devbox", "op", new Uint8Array([1]), { until: performance.now() - 1 }));
    expect(e).toEqual({ layer: "hop", at: { idx: 0, tag: "write" }, reach: "NotSent", why: "Overrun" });
    expect(invokeMock, "过期了还发了").not.toHaveBeenCalled();
  });

  it("★ 过线的是「还剩多少」：`leftMs` 在 (0, 给的毫秒] 之内，绝对时刻不上线", async () => {
    invokeMock.mockResolvedValue(new Uint8Array([7]).buffer);
    await chan.call("<local>", "op", new Uint8Array(), budgetWithin(5_000));
    const args = invokeMock.mock.calls[0][1] as { leftMs: number; origin: string; op: string };
    expect(invokeMock.mock.calls[0][0]).toBe("chan_call");
    expect(args.origin).toBe("<local>");
    expect(args.op).toBe("op");
    expect(args.leftMs).toBeGreaterThan(0);
    expect(args.leftMs).toBeLessThanOrEqual(5_000);
    expect(remaining({ until: performance.now() - 10 }), "过了期限不许是负数").toBe(0);
  });

  it("★ 出口声明（view）原样搬进 `chan_call` 的那一格，不进载荷；没给 ⇒ `null`", async () => {
    invokeMock.mockResolvedValue(new Uint8Array([7]).buffer);
    const view = { omit: { record: ["blocks[type=tool_use].input"] } };
    const payload = new Uint8Array([1, 2, 3]);
    await chan.call("<local>", "op", payload, budgetWithin(5_000), view);
    await chan.call("<local>", "op", payload, budgetWithin(5_000));
    const [withView, without] = invokeMock.mock.calls.map((c) => c[1] as { view: unknown; payload: number[] });
    expect(withView.view).toEqual(view);
    expect(withView.payload, "声明不许塞进载荷").toEqual([1, 2, 3]);
    expect(without.view).toBeNull();
  });

  it("★ 撤单：已经拨下的不发；在飞时拨下立即回 `Ours{Cancelled}`，不等 monitor", async () => {
    const pre = new AbortController();
    pre.abort();
    expect(await failOf(chan.call("devbox", "op", new Uint8Array(), budgetWithin(5_000, pre.signal)))).toEqual({
      layer: "ours",
      why: "Cancelled",
    });
    expect(invokeMock).not.toHaveBeenCalled();

    let answer: (v: ArrayBuffer) => void = () => {};
    invokeMock.mockReturnValue(new Promise<ArrayBuffer>((r) => (answer = r)));
    const ac = new AbortController();
    const p = chan.call("devbox", "op", new Uint8Array(), budgetWithin(5_000, ac.signal));
    ac.abort();
    expect(await failOf(p)).toEqual({ layer: "ours", why: "Cancelled" });
    answer(new ArrayBuffer(0)); // monitor 那一侧晚到的结局没人收了 —— 不许炸
  });

  it("🔴〔MIG-3b 续 · 撤单不许回退〕撤单过 webview 那一跳：带撤单的那一问带编号，拨下 ⇒ 恰一发 `chan_cancel` 带着同一个编号；不带撤单的不带编号、不发", async () => {
    invokeMock.mockImplementation((cmd: string) =>
      cmd === "chan_call" ? new Promise<ArrayBuffer>(() => {}) : Promise.resolve(cmd === "chan_cancel" ? true : null),
    );
    const ac = new AbortController();
    const p = chan.call("devbox", "slow", new Uint8Array(), budgetWithin(5_000, ac.signal));
    const callArgs = invokeMock.mock.calls.find((c) => c[0] === "chan_call")?.[1] as { callId: string | null };
    expect(typeof callArgs.callId).toBe("string");
    expect(invokeMock.mock.calls.some((c) => c[0] === "chan_cancel"), "还没撤就发了撤单").toBe(false);
    ac.abort();
    await failOf(p);
    const cancels = invokeMock.mock.calls.filter((c) => c[0] === "chan_cancel");
    expect(cancels).toEqual([["chan_cancel", { id: callArgs.callId }]]);

    invokeMock.mockClear();
    void chan.call("devbox", "fire", new Uint8Array(), budgetWithin(5_000));
    const plain = invokeMock.mock.calls.find((c) => c[0] === "chan_call")?.[1] as { callId: string | null };
    expect(plain.callId, "不带撤单的那一问不该带编号").toBeNull();
  });

  it("〔NET2〕本地撤单按手里那份 Offer 说清「那台可能还在跑」：撤不动的带 runsOn、撤得动的不带", async () => {
    let answer: (v: ArrayBuffer) => void = () => {};
    invokeMock.mockImplementation((cmd: string) =>
      cmd === "chan_offer"
        ? Promise.resolve({ ops: ["cancel", "slow", "quick"], unavailable: [["kill", "no_tmux"]], stoppable: ["quick"] })
        : new Promise<ArrayBuffer>((r) => (answer = r)),
    );
    const offer = await chan.offer("net2-m");
    expect(offer?.stoppable).toEqual(["quick"]);
    expect(chan.cachedOffer("net2-m")).toEqual(offer);
    for (const [op, runsOn] of [["slow", true], ["quick", false]] as const) {
      const ac = new AbortController();
      const p = chan.call("net2-m", op, new Uint8Array(), budgetWithin(5_000, ac.signal));
      ac.abort();
      const e = await failOf(p);
      expect(e, op).toEqual(runsOn ? { layer: "ours", why: "Cancelled", runsOn: true } : { layer: "ours", why: "Cancelled" });
      expect(saidFrom(new ChanError(e), "net2-m").includes(copyText("chanCaller.said.withdrawnRunsOn")), op).toBe(runsOn);
    }
    answer(new ArrayBuffer(0));
    // monitor 交回的那一形（`WireErr::OursRunsOn`）解回同一格。
    expect(decodeFail({ err: { OursRunsOn: "Cancelled" }, body: [] })).toEqual({ layer: "ours", why: "Cancelled", runsOn: true });
  });

  it("★ 载荷两个方向逐字节原样（含 NUL 与非 UTF-8 字节）", async () => {
    const out = new Uint8Array([0, 0xff, 0xfe, 0x80, 123]);
    const back = new Uint8Array([0xc3, 0x28, 0, 255]);
    invokeMock.mockResolvedValue(back.buffer);
    const got = await chan.call("devbox", "op", out, budgetWithin(5_000));
    expect([...got]).toEqual([...back]);
    const sent = invokeMock.mock.calls[0][1] as { payload: number[] };
    expect(sent.payload).toEqual([...out]);
  });

  it("★ 解不出来的拒绝（IPC 自己的报错串 / 认不出的形状）一律 `ours/Broken`，不猜", async () => {
    for (const raw of [
      "command chan_call not found",
      null,
      { err: "SomethingNew", body: [] },
      { err: { Hop: { idx: 1, tag: "teleport", reach: "Sent", why: "Dropped" } }, body: [] },
      { err: "Refused", body: [256] },
    ]) {
      invokeMock.mockRejectedValueOnce(raw);
      expect(await failOf(chan.call("devbox", "op", new Uint8Array(), budgetWithin(5_000))), JSON.stringify(raw)).toEqual({
        layer: "ours",
        why: "Broken",
      });
    }
  });

  it("★ 成员不解释载荷：`chan.ts` 生产段零 `JSON.parse` / `JSON.stringify`（正控：调用方那一侧命中）", () => {
    const code = (rel: string): string => stripComments(readFileSync(resolve(REPO_ROOT, rel), "utf8"), "ts");
    const jsonUses = (c: string): number => (c.match(/\bJSON\.(?:parse|stringify)\s*\(/g) ?? []).length;
    expect(jsonUses(code("src/frontend/ui/ipc/chan-caller.ts")), "正控：调用方那一侧恰好两处（`jsonBody` 的 stringify · `readJson` 的 parse）").toBe(2);
    expect(jsonUses(code("src/comms/inward/chan.ts")), "通道成员自己解释了载荷 ——：载荷是不透明字节").toBe(0);
  });
});

// ════════════════════════════════════════════════════════════════════════════
// `subscribe` 那一半
//
// （`Item` 五个变体；「丢必须说」「`Gap` 必须在流里的原位」）·
//  `§3.3.5`「`call` 会失败、`subscribe` 不会」。
// ════════════════════════════════════════════════════════════════════════════

/** 往交格事件里投一次（`{sub, items}`）。 */
function deliver(sub: number, items: unknown[]): void {
  expect(itemsListeners.length, "交格事件没人听 —— 本组会零命中地绿").toBeGreaterThan(0);
  for (const l of itemsListeners) l({ payload: { sub, items } });
}

/** 取最近一次 `chan_subscribe` 用的编号（本侧给的）。 */
function lastSubId(): number {
  const calls = invokeMock.mock.calls.filter((c) => c[0] === "chan_subscribe");
  expect(calls.length, "没登记到 monitor").toBeGreaterThan(0);
  return (calls[calls.length - 1][1] as { id: number }).id;
}

describe("〔CF2〕webview 通道客户端 · subscribe", () => {
  it("★★ S5 金标准：Rust 造的那八种格，TS 逐个解回 `§3.3.4` 的 `Item`", () => {
    const golden = JSON.parse(
      readFileSync(resolve(REPO_ROOT, "tests/__fixtures__/chan-webview-items.golden.json"), "utf8"),
    ) as { sub: number; items: unknown[] };
    expect(golden.items.length, "金标准的格数变了 —— 两侧要同拍改").toBe(8);
    const want: Item[] = [
      { t: "frame", seq: 7, body: '{"line":{"x":1}}' },
      { t: "gap", fromSeq: 8, toSeq: 12 },
      { t: "gap", fromSeq: 13, toSeq: null }, // 知道丢了、不知道丢到哪
      { t: "unseen", at: { idx: 1, tag: "open" }, why: "Unreachable" },
      { t: "seen", from: null },
      { t: "seen", from: Uint8Array.from([1, 2]) },
      { t: "closed", by: { peer: '{"code":"no-such-stream"}' } },
      { t: "closed", by: { ours: "Broken" } },
    ];
    expect(golden.items.map(decodeItem)).toEqual(want);
    // 解不出的：认不出的标签 · 缺字段 · 倒着的 gap · 越界字节
    for (const bad of [
      { t: "frame", seq: 1 },
      { t: "frame", seq: -1, body: "" },
      { t: "gap", from_seq: 5, to_seq: 3 },
      { t: "unseen", idx: 1, tag: "sleep", why: "Dropped" },
      { t: "seen", from: [256] },
      { t: "closed_by_ours", why: "Tired" },
      { t: "nope" },
      null,
    ]) {
      expect(decodeItem(bad), JSON.stringify(bad)).toBeNull();
    }
  });

  it("★ S5 编号 · 往回说：先记 sink 再登记；本编号的格交给它、别的编号不交；want / stop 各恰好一次 IPC", async () => {
    invokeMock.mockResolvedValue(undefined);
    const got: Item[][] = [];
    const sub = await chan.subscribe("<local>", "session-lines", null, 5, (items) => got.push(items));
    const id = lastSubId();
    expect(invokeMock.mock.calls.filter((c) => c[0] === "chan_subscribe")[0][1]).toEqual({
      origin: "<local>",
      kind: "session-lines",
      from: null,
      want: 5,
      id,
    });
    deliver(id, [{ t: "frame", seq: 0, body: "x" }]);
    deliver(id + 1000, [{ t: "frame", seq: 0, body: "别人的" }]);
    expect(got).toEqual([[{ t: "frame", seq: 0, body: "x" }]]);
    sub.want(3);
    expect(invokeMock.mock.calls.filter((c) => c[0] === "chan_want")).toEqual([
      ["chan_want", { id, more: 3 }],
    ]);
    sub.stop();
    sub.stop();
    sub.want(9);
    expect(invokeMock.mock.calls.filter((c) => c[0] === "chan_stop")).toEqual([["chan_stop", { id }]]);
    expect(invokeMock.mock.calls.filter((c) => c[0] === "chan_want").length, "撤了之后还在报 credit").toBe(1);
    deliver(id, [{ t: "frame", seq: 1, body: "y" }]);
    expect(got.length, "撤了之后还在交格（本地撤单是立即的）").toBe(1);
  });

  it("★ S5 解不出：一格解不出 ⇒ 前面解得出的照交、末尾补 `closed{ours: Broken}`，并撤掉这条", async () => {
    invokeMock.mockResolvedValue(undefined);
    const got: Item[][] = [];
    await chan.subscribe("box", "session-lines", null, 5, (items) => got.push(items));
    const id = lastSubId();
    deliver(id, [{ t: "frame", seq: 0, body: "a" }, { t: "what" }, { t: "frame", seq: 1, body: "b" }]);
    expect(got).toEqual([[{ t: "frame", seq: 0, body: "a" }, { t: "closed", by: { ours: "Broken" } }]]);
    expect(invokeMock.mock.calls.filter((c) => c[0] === "chan_stop")).toEqual([["chan_stop", { id }]]);
    deliver(id, [{ t: "frame", seq: 2, body: "c" }]);
    expect(got.length).toBe(1);
  });

  it("★ S5 信用报不上去：`chan_want` 抛了 ⇒ 原位交一格 `closed{ours: Broken}`、撤单、之后的格不再交", async () => {
    // 〔`audit/E-compat.md §3.3`「`chan_want` 失败被吞 ⇒ 订阅拿不到信用，静默停流」〕
    // 要求：「`subscribe` 不会失败」—— 说不了的在流里原位说。
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "chan_want") throw new Error("ipc down");
      return undefined;
    });
    const got: Item[][] = [];
    const sub = await chan.subscribe("<local>", "session-lines", null, 5, (items) => got.push(items));
    const id = lastSubId();
    deliver(id, [{ t: "frame", seq: 0, body: "a" }]);
    sub.want(1);
    await new Promise((r) => setTimeout(r, 0));
    expect(got).toEqual([[{ t: "frame", seq: 0, body: "a" }], [{ t: "closed", by: { ours: "Broken" } }]]);
    expect(invokeMock.mock.calls.filter((c) => c[0] === "chan_stop")).toEqual([["chan_stop", { id }]]);
    deliver(id, [{ t: "frame", seq: 1, body: "b" }]);
    sub.want(1);
    await new Promise((r) => setTimeout(r, 0));
    expect(got.length, "撤了之后还在交格 / 又说了一遍").toBe(2);
    expect(invokeMock.mock.calls.filter((c) => c[0] === "chan_want").length, "撤了之后还在报 credit").toBe(1);
  });

  it("★ S5 不失败：登记那一跳抛了 ⇒ 原位交 `closed{ours: Broken}`，`subscribe` 本身照常返回", async () => {
    invokeMock.mockRejectedValue(new Error("ipc down"));
    const got: Item[][] = [];
    const sub = await chan.subscribe("<local>", "session-lines", null, 5, (items) => got.push(items));
    expect(got).toEqual([[{ t: "closed", by: { ours: "Broken" } }]]);
    expect(typeof sub.want).toBe("function");
  });
});
