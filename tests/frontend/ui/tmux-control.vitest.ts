/**
 * 界面直接说的 tmux 控制类帧命令（`src/frontend/ui/tmux-control.ts`）的判据。
 *
 * 守的要求：「正路是**把解释挪进后端、直接出成品**：后端帧命令的应答就是界面要的那个形状，
 * 前端经 `chan.call` 直接问、按形状收（不解释），monitor 那一份解释与发送点一起删」·「成品的两侧对拍：界面按形状严格收
 * （多一格 / 缺一格 / 类型不对 ⇒ 抛「两端契约对不上」，不猜）；线上形状由一份跨语言金样钉住」。
 * 另守 `§34` Gate 1 「界面不判目标名」（Gate 1 并进 `gate-core` 那一族、TS 零：空目标原样交给后端，后端拒了照原话说）与
 * 「失败要显式、归因要准确」（本机与远端的下一步不同，话就不许一样）。
 *
 * | 性质 | 判据 |
 * |---|---|
 * | TS 解码器读得懂后端真出的成品 —— 同一份跨语言金样，后端那侧 `kill_tests` 对拍它（异源：Rust 构造器造、TS 解） | 「金样」 |
 * | 形状不对 ⇒ 抛（多一格 / 缺一格 / 类型不对），不猜 | 「形状不对」 |
 * | 界面不判目标名：空目标原样交给后端；后端 `bad_args` ⇒ 各动作那句「后端不接受这个会话名」带后端原话 | 「空目标」 |
 * | 本机与远端同一条路（`<local>` 照样经通道问），通道不在时两句话不同、远端那句点得出是哪台 | 「本机」「通道不在」 |
 * | 拒绝码逐码一句、两两不同、带上会话名与后端原话；认不出的码不上屏（只说原话，码在诊断里）、不被猜成已知档（码集合取自金样，不是手抄） | 「拒绝码」 |
 * | 结束会话：请求体 == 金样；`killed` 不为真不当成功；门拒绝 ≠ 通道不在 | 「结束会话」两组 |
 *
 * 买不到：真 Tauri IPC 与真后端（后端那一侧在 Rust 里；monitor 那一跳由 `webview_tests` 量）；真 tmux 会话上的一屏。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { invoke } from "@tauri-apps/api/core";
import { LOCAL_ORIGIN } from "../../../src/frontend/ui/ipc/origin";
import {
  ControlError,
  decodeKilled,
  killSession,
} from "../../../src/frontend/ui/tmux-control";
import { REPO_ROOT } from "../../test-support/repo-root";
import { chanArgsJson, chanReply, refusedReply, NO_CHANNEL, type ChanCallArgs } from "../../test-support/chan-fake";
import { copyText, type CopyKey } from "../../../src/frontend/ui/copy-table";
import { copyPattern } from "../../test-support/copy-pattern";

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
const KILL = golden.kill;

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

/** 跑一趟 `act`，拿抛出来的那一句（没抛 ⇒ 判据失败）。 */
async function saidOf(act: () => Promise<unknown>): Promise<string> {
  try {
    await act();
  } catch (e) {
    expect(e, "抛的不是 ControlError —— 调用方拿不到那一句").toBeInstanceOf(ControlError);
    return (e as ControlError).message;
  }
  throw new Error("本该失败却成功了");
}


/** 失败那一下的诊断（可复制的那一份）：错误码在这里，不在给人看的那一句里。 */
async function detailOf(act: () => Promise<unknown>): Promise<string> {
  try {
    await act();
  } catch (e) {
    expect(e).toBeInstanceOf(ControlError);
    return (e as ControlError).detail;
  }
  throw new Error("本该失败却成功了");
}

// ════════════════════════════════════════════════════════════════════════════
//  结束会话（`kill`）
// ════════════════════════════════════════════════════════════════════════════

/** 动作「跑一趟、拿那一句」。 */
const ACTIONS: [string, (origin: string, target: string) => Promise<unknown>, CopyKey][] = [
  [copyText("tabSessionActions.kill.action"), (o, t) => killSession(o, t), "tmuxControl.kill.badName"],
];

describe("〔C4e〕结束会话：按形状收", () => {
  it("★★ 金样：解码器读得懂后端真出的成品；请求体就是金样那一份", async () => {
    expect(() => decodeKilled("devbox", "demo-cc", KILL.reply)).not.toThrow();
    answer({ ok: KILL.reply });
    await killSession("devbox", String(KILL.request.name));
    expect(sentCalls()).toEqual([["devbox", "kill", KILL.request]]);
  });

  it("★★ 没明说做成了 ⇒ 不当成功：`killed` 为假、多一格 / 缺一格 / 类型不对都抛", () => {
    expect(() => decodeKilled("devbox", "demo-cc", { ...KILL.reply, killed: false })).toThrow(copyPattern("tmuxControl.kill.notConfirmed", { target: "demo-cc" }));
    for (const bad of [{ ...KILL.reply, extra: 1 }, { killed: true }, { session: "demo-cc", killed: "yes" }, null]) {
      expect(() => decodeKilled("devbox", "demo-cc", bad), JSON.stringify(bad)).toThrow(copyPattern("peerVersion.said.unreadable"));
    }
  });
});

describe("按 sid 找窗格：结束带上会话 ID", () => {
  it("★ 给了 sid ⇒ 请求里带上 `sid`，后端按它落在挂着它的那个窗格；不给 ⇒ 请求形状不变", async () => {
    answer({ ok: KILL.reply });
    await killSession("devbox", "demo-cc", "sid-a");
    await killSession("devbox", "demo-cc");
    expect(sentCalls()).toEqual([
      ["devbox", "kill", { name: "demo-cc", sid: "sid-a" }],
      ["devbox", "kill", { name: "demo-cc" }],
    ]);
  });
});

describe("FIX4 · 杀会话顺手注销的结局", () => {
  /** 「杀会话顺手注销的结局只进日志：界面不说『顺手注销了谁 / 没注销成』；要说得给 kill 的成品加一格（界面、金样、文案同拍）」。 */
  it("★ bus 那一格 ⇒ 一句话：注销了谁 · 谁没注销成 · 名册读不到；全空不说；那一格缺 / 形状不对 ⇒ 读不懂", () => {
    const bus = (b: unknown) => ({ ...KILL.reply, bus: b });
    expect(decodeKilled("devbox", "demo-cc", KILL.reply)).toBeNull();
    expect(decodeKilled("devbox", "demo-cc", bus({ removed: ["p_cc", "q_cc"], failed: [], unread: null }))).toBe(
      copyText("tmuxControl.kill.busRemoved", { ids: ["p_cc", "q_cc"].join(copyText("tmuxControl.kill.listSep")) }),
    );
    expect(decodeKilled("devbox", "demo-cc", bus({ removed: [], failed: [{ id: "r_cc", why: "它不在" }], unread: null }))).toBe(
      copyText("tmuxControl.kill.busFailed", { id: "r_cc", why: "它不在" }),
    );
    expect(decodeKilled("devbox", "demo-cc", bus({ removed: [], failed: [], unread: "cc-list 退出 1" }))).toBe(
      copyText("tmuxControl.kill.busUnread", { why: "cc-list 退出 1" }),
    );
    for (const bad of [{ session: "demo-cc", killed: true }, bus({ removed: [], failed: [] }), bus({ removed: [1], failed: [], unread: null })]) {
      expect(() => decodeKilled("devbox", "demo-cc", bad), JSON.stringify(bad)).toThrow(copyPattern("peerVersion.said.unreadable"));
    }
  });
});

describe("〔C4e〕结束会话：发出去之前与失败怎么说", () => {
  it("★ 〔DUP3〕空目标不在界面判：原样交给后端，后端拒了照原话说", async () => {
    answer({ fail: refusedReply("bad_args", "`name` 为空") });
    for (const [label, act, badKey] of ACTIONS) {
      const said = await saidOf(() => act("devbox", ""));
      expect(said, label).toMatch(copyPattern(badKey));
      expect(said, label).toContain("`name` 为空");
    }
    expect(sentCalls().map(([, op, body]) => [op, (body as { name?: unknown }).name]), "界面自己把空目标拦下了").toEqual([["kill", ""]]);
  });

  it("★★ 通道不在：本机与远端两句话不同、远端点得出是哪台；本机照样经通道问", async () => {
    answer({ fail: NO_CHANNEL });
    for (const [label, act] of ACTIONS) {
      const local = await saidOf(() => act(LOCAL_ORIGIN, "demo-cc"));
      const remote = await saidOf(() => act("kr-remote-label", "demo-cc"));
      expect(local, label).not.toBe(remote);
      expect(local, label).toMatch(copyText("control.channel.localDown"));
      expect(remote, label).toContain("kr-remote-label");
      for (const x of [local, remote]) expect(x, label).not.toMatch(copyPattern("rsLaunch.remote.noConfig"));
    }
    expect(sentCalls().filter(([o]) => o === LOCAL_ORIGIN).length, "本机没经通道问").toBe(1);
  });

  it("★★ 拒绝码（取自金样）逐码一句、两两不同、带会话名与后端原话；身份门那一句 ≠ 通道不在那一句", async () => {
    for (const [op, codes, act] of [
      ["kill", KILL.codes, (t: string) => killSession("devbox", t)],
    ] as const) {
      const said: string[] = [];
      for (const code of codes) {
        answer({ fail: refusedReply(code, "RAW-WORDS") });
        said.push(await saidOf(() => act("demo-cc")));
      }
      expect(said.length, `${op}：金样里一个码都没有`).toBeGreaterThan(0);
      expect(new Set(said).size, `${op}：有两档被压成了同一句：${JSON.stringify(said)}`).toBe(said.length);
      for (const x of said) {
        expect(x, op).toContain("demo-cc");
        expect(x, `${op}：后端的原话被吃掉了`).toContain("RAW-WORDS");
      }
      answer({ fail: refusedReply("zzz_new_code", "RAW-WORDS") });
      const unknown = await saidOf(() => act("demo-cc"));
      expect(unknown, op).toContain("RAW-WORDS");
      expect(unknown, `${op}：错误码上了屏`).not.toContain("zzz_new_code");
      expect(said, op).not.toContain(unknown);
      answer({ fail: refusedReply("zzz_new_code", "RAW-WORDS") });
      expect(await detailOf(() => act("demo-cc")), `${op}：诊断里没有码`).toContain("zzz_new_code");
      // 三态不许压成两态：门拒绝（身份门）与通道不在是两句话。
      answer({ fail: refusedReply("wrong_owner", "RAW-WORDS") });
      const gate = await saidOf(() => act("demo-cc"));
      answer({ fail: NO_CHANNEL });
      expect(await saidOf(() => act("demo-cc")), op).not.toBe(gate);
    }
  });
});

describe("那台握手时说过做不到的，菜单置灰并说为什么", () => {
  it("kill 在「没有 tmux」的那台上不可点、字后面带原因；那台没说的项与没问过的机器照常", async () => {
    const { gateByOffer } = await import("../../../src/frontend/ui/tab-menu");
    const { chan } = await import("../../../src/comms/inward/chan");
    invokeMock.mockImplementation((cmd: string, args: { origin?: string }) =>
      Promise.resolve(
        cmd === "chan_offer" && args.origin === "net2-box"
          ? { ops: ["kill", "terminal-preview"], unavailable: [["kill", "no_tmux"]], stoppable: [] }
          : null,
      ),
    );
    await chan.offer("net2-box");
    const click = vi.fn();
    const kill = gateByOffer("net2-box", { id: "kill", label: copyText("tabSessionActions.kill.action"), onClick: click });
    expect(kill.enabled).toBe(false);
    expect(kill.why, "第二行写为什么（不拼进项名）").toBe(copyText("control.unavailable.noTmux", { machine: "net2-box" }));
    expect(kill.label).toBe(copyText("tabSessionActions.kill.action"));
    kill.onClick?.();
    expect(click).not.toHaveBeenCalled();
    const preview = { id: "preview", label: "预览", onClick: click };
    expect(gateByOffer("net2-box", preview)).toBe(preview);
    const elsewhere = { id: "kill", label: copyText("tabSessionActions.kill.action"), onClick: click };
    expect(gateByOffer("net2-other", elsewhere)).toBe(elsewhere);
  });
});
