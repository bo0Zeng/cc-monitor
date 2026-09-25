/**
 * 〔C4e · 第四波 4C〕界面直接说的 tmux 控制类帧命令（`src/tmux-control.ts`）的判据。
 *
 * 守的要求：`设计/05 §14.3` 逐字「正路是**把解释挪进后端、直接出成品**：后端帧命令的应答就是界面要的那个形状，
 * 前端经 `chan.call` 直接问、按形状收（不解释），monitor 那一份解释与发送点一起删」·「成品的两侧对拍：界面按形状严格收
 * （多一格 / 缺一格 / 类型不对 ⇒ 抛「两端契约对不上」，不猜）；线上形状由一份跨语言金样钉住」。
 * 另守 `§34` Gate 1 的本地那一格（空目标不许发出去：`=:` 会被 tmux 读成「当前会话」）与 `设计/01 §5 D7`
 * 「失败要显式、归因要准确」（本机与远端的下一步不同，话就不许一样）。
 *
 * | 性质 | 判据 |
 * |---|---|
 * | TS 解码器读得懂后端真出的成品 —— 同一份跨语言金样，后端那侧 `capture_pane_tests::the_capture_product_matches_the_cross_language_golden` 对拍它（异源：Rust 构造器造、TS 解） | 「金样」 |
 * | 形状不对 ⇒ 抛（多一格 / 缺一格 / 类型不对），不猜 | 「形状不对」 |
 * | 空目标就地拒，一个字节都不发 | 「空目标」 |
 * | 本机与远端同一条路（`<local>` 照样经通道问），通道不在时两句话不同、远端那句点得出是哪台 | 「本机」「通道不在」 |
 * | 拒绝码逐码一句、两两不同、带上会话名与后端原话；认不出的码原样带出去、不被猜成已知档（码集合取自金样，不是手抄） | 「拒绝码」 |
 *
 * 买不到：真 Tauri IPC 与真后端（后端那一侧在 Rust 里；monitor 那一跳由 `webview_tests` 量）；真 tmux 会话上的一屏。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { invoke } from "@tauri-apps/api/core";
import { LOCAL_ORIGIN } from "../src/ipc/origin";
import { capturePane, decodeCapture, ControlError } from "../src/tmux-control";
import { REPO_ROOT } from "./test-support/repo-root";
import { chanArgsJson, chanReply, refusedReply, UNSUPPORTED, NO_CHANNEL, type ChanCallArgs } from "./test-support/chan-fake";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;

interface GoldenOp {
  request: Record<string, unknown>;
  reply: Record<string, unknown>;
  codes: string[];
}
const golden = JSON.parse(readFileSync(resolve(REPO_ROOT, "tests/__fixtures__/tmux-control.golden.json"), "utf8")) as Record<
  string,
  GoldenOp
>;
const CAP = golden["capture-pane"];

// ⚠ 花括号不能省：`mockReset()` 返回 mock 本身，箭头函数直接返回它 ⇒ vitest 把它当成清理钩子在用例结束时再调一次。
beforeEach(() => {
  invokeMock.mockReset();
});

/** 这一趟全部 `chan_call` 的 `(origin, op, 请求体)`。 */
function sentCalls(): [string, string, unknown][] {
  return invokeMock.mock.calls
    .filter(([cmd]) => cmd === "chan_call")
    .map(([, a]) => {
      const args = a as ChanCallArgs;
      return [args.origin, args.op, chanArgsJson(args)];
    });
}

/** 让那一跳按 `reply` 答（`reply` 是一份成品 ⇒ 字节；是一个失败值 ⇒ 抛它）。 */
function answer(reply: { ok: unknown } | { fail: unknown }): void {
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd !== "chan_call") throw new Error(`没想到会调 ${cmd}`);
    if ("ok" in reply) return chanReply(reply.ok);
    throw reply.fail;
  });
}

/** 跑一趟抓屏，拿抛出来的那一句（没抛 ⇒ 判据失败）。 */
async function saidBy(origin: string, target: string): Promise<string> {
  try {
    await capturePane(origin, target);
  } catch (e) {
    expect(e, "抛的不是 ControlError —— 调用方拿不到那一句").toBeInstanceOf(ControlError);
    return (e as ControlError).message;
  }
  throw new Error("本该失败却成功了");
}

describe("〔C4e〕抓一屏：按形状收", () => {
  it("★★ 金样：解码器读得懂后端真出的成品；请求体就是金样那一份", async () => {
    expect(decodeCapture("devbox", CAP.reply)).toBe(CAP.reply.screen);
    answer({ ok: CAP.reply });
    await expect(capturePane("devbox", String(CAP.request.name))).resolves.toBe(CAP.reply.screen);
    expect(sentCalls()).toEqual([["devbox", "capture-pane", CAP.request]]);
  });

  it("★ 空屏是合法的成功（刚建起来、什么都没打印的 pane），不是失败", () => {
    expect(decodeCapture("devbox", { name: "demo-cc", screen: "" })).toBe("");
  });

  it("★ 形状不对 ⇒ 抛（多一格 / 缺一格 / 类型不对 / 不是对象），不猜", () => {
    const bad = [
      { ...CAP.reply, extra: 1 },
      { name: "demo-cc" },
      { name: "demo-cc", screen: 3 },
      { lines: [] },
      null,
      "screen",
    ];
    for (const v of bad) {
      expect(() => decodeCapture("devbox", v), JSON.stringify(v)).toThrow(/读不懂/);
    }
  });
});

describe("〔C4e〕抓一屏：发出去之前", () => {
  it("★ 空目标就地拒，一个字节都不发（`=:` 会被 tmux 读成「当前会话」）", async () => {
    answer({ ok: CAP.reply });
    const said = await saidBy("devbox", "");
    expect(said).toMatch(/没有指定 tmux 会话/);
    expect(invokeMock, "空目标也发出去了").not.toHaveBeenCalled();
  });

  it("★ 本机照样经通道问（`<local>` 也是一台机器），不是死胡同", async () => {
    answer({ ok: CAP.reply });
    await expect(capturePane(LOCAL_ORIGIN, "demo-cc")).resolves.toBe(CAP.reply.screen);
    expect(sentCalls()).toEqual([[LOCAL_ORIGIN, "capture-pane", { name: "demo-cc" }]]);
    const leftMs = (invokeMock.mock.calls[0][1] as ChanCallArgs).leftMs;
    expect(leftMs, "期限没交（X6：调用点显式给）").toBeGreaterThan(0);
  });
});

describe("〔C4e〕抓一屏：失败怎么说", () => {
  it("★★ 通道不在：本机与远端两句话不同；远端那句点得出是哪台；都不说「未找到远端配置」", async () => {
    answer({ fail: NO_CHANNEL });
    const local = await saidBy(LOCAL_ORIGIN, "demo-cc");
    const remote = await saidBy("kr-remote-label", "demo-cc");
    expect(local).not.toBe(remote);
    expect(local).toMatch(/本机后端/);
    expect(remote).toContain("kr-remote-label");
    expect(remote).not.toMatch(/本机/);
    for (const s of [local, remote]) expect(s).not.toMatch(/未找到远端配置/);
  });

  it("★ 那台后端比这条命令老（对端事前说不认）⇒ 说版本不对，不说连不上", async () => {
    answer({ fail: UNSUPPORTED });
    const said = await saidBy("devbox", "demo-cc");
    expect(said).toMatch(/devbox 的后端版本不对/);
  });

  it("★ 连接断了 / 等回复超时 ⇒ 说不知道做完没有；撤回 ⇒ 说撤回了", async () => {
    answer({ fail: { err: { Hop: { idx: 1, tag: "wait", reach: "Unknown", why: "Overrun" } }, body: [] } });
    expect(await saidBy("devbox", "demo-cc")).toMatch(/不知道 devbox 那边做完了没有/);
    answer({ fail: { err: { Ours: "Cancelled" }, body: [] } });
    expect(await saidBy("devbox", "demo-cc")).toMatch(/撤回/);
  });

  it("★★ 拒绝码（取自金样）逐码一句、两两不同、带会话名与后端原话；认不出的码原样带出去、不被猜成已知档", async () => {
    const said: string[] = [];
    for (const code of CAP.codes) {
      answer({ fail: refusedReply(code, "RAW-WORDS") });
      said.push(await saidBy("devbox", "demo-cc"));
    }
    expect(said.length, "金样里一个码都没有 —— 下面全是空转").toBeGreaterThan(0);
    expect(new Set(said).size, `有两档被压成了同一句：${JSON.stringify(said)}`).toBe(said.length);
    for (const s of said) {
      expect(s).toContain("demo-cc");
      expect(s, "后端的原话被吃掉了").toContain("RAW-WORDS");
    }
    answer({ fail: refusedReply("zzz_new_code", "RAW-WORDS") });
    const unknown = await saidBy("devbox", "demo-cc");
    expect(unknown).toContain("zzz_new_code");
    expect(said).not.toContain(unknown);
    // 拒绝体读不出来 ⇒ 仍是一句「被拒」，不是空串、不是崩。
    answer({ fail: { err: "Refused", body: [0xff] } });
    expect(await saidBy("devbox", "demo-cc")).toMatch(/被拒/);
  });
});
