/**
 * 〔C4a · 第四波〕通道在 webview 手里那一半（`src/ipc/chan.ts`）的判据。
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
 * 买不到：真 Tauri IPC 那一跳（要一个活的 webview）—— 这里 mock 的是 `invoke`，
 * 它之后的那一跳由 Rust 侧 `tests/bridge/chan/webview_tests.rs` 用合成句柄 ＋ 真 `router::settle` 量。
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

import { invoke } from "@tauri-apps/api/core";
import { ChanError, chan, decodeFail, remaining, type CallError } from "../../src/ipc/chan";
import { budgetWithin } from "../../src/ipc/chan-caller";
import { REPO_ROOT } from "../test-support/repo-root";
import { stripComments } from "../test-support/strip-comments";

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
    const e = await failOf(chan.call("aya", "op", new Uint8Array([1]), { until: performance.now() - 1 }));
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

  it("★ 撤单：已经拨下的不发；在飞时拨下立即回 `Ours{Cancelled}`，不等 monitor", async () => {
    const pre = new AbortController();
    pre.abort();
    expect(await failOf(chan.call("aya", "op", new Uint8Array(), budgetWithin(5_000, pre.signal)))).toEqual({
      layer: "ours",
      why: "Cancelled",
    });
    expect(invokeMock).not.toHaveBeenCalled();

    let answer: (v: ArrayBuffer) => void = () => {};
    invokeMock.mockReturnValue(new Promise<ArrayBuffer>((r) => (answer = r)));
    const ac = new AbortController();
    const p = chan.call("aya", "op", new Uint8Array(), budgetWithin(5_000, ac.signal));
    ac.abort();
    expect(await failOf(p)).toEqual({ layer: "ours", why: "Cancelled" });
    answer(new ArrayBuffer(0)); // monitor 那一侧晚到的结局没人收了 —— 不许炸
  });

  it("★ 载荷两个方向逐字节原样（含 NUL 与非 UTF-8 字节）", async () => {
    const out = new Uint8Array([0, 0xff, 0xfe, 0x80, 123]);
    const back = new Uint8Array([0xc3, 0x28, 0, 255]);
    invokeMock.mockResolvedValue(back.buffer);
    const got = await chan.call("aya", "op", out, budgetWithin(5_000));
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
      expect(await failOf(chan.call("aya", "op", new Uint8Array(), budgetWithin(5_000))), JSON.stringify(raw)).toEqual({
        layer: "ours",
        why: "Broken",
      });
    }
  });

  it("★ 成员不解释载荷：`chan.ts` 生产段零 `JSON.parse` / `JSON.stringify`（正控：调用方那一侧命中）", () => {
    const code = (rel: string): string => stripComments(readFileSync(resolve(REPO_ROOT, rel), "utf8"), "ts");
    const jsonUses = (c: string): number => (c.match(/\bJSON\.(?:parse|stringify)\s*\(/g) ?? []).length;
    expect(jsonUses(code("src/ipc/chan-caller.ts")), "正控：调用方那一侧恰好两处（`jsonBody` 的 stringify · `readJson` 的 parse）").toBe(2);
    expect(jsonUses(code("src/ipc/chan.ts")), "通道成员自己解释了载荷 —— `设计/05 §2`：载荷是不透明字节").toBe(0);
  });
});
