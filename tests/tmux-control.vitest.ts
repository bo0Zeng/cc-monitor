/**
 * 〔C4e · 第四波 4C〕界面直接说的 tmux 控制类帧命令（`src/tmux-control.ts`）的判据。
 *
 * 守的要求：`设计/05 §14.3` 逐字「正路是**把解释挪进后端、直接出成品**：后端帧命令的应答就是界面要的那个形状，
 * 前端经 `chan.call` 直接问、按形状收（不解释），monitor 那一份解释与发送点一起删」·「成品的两侧对拍：界面按形状严格收
 * （多一格 / 缺一格 / 类型不对 ⇒ 抛「两端契约对不上」，不猜）；线上形状由一份跨语言金样钉住」。
 * 另守 `§34` Gate 1 〔DUP3〕「界面不判目标名」（Gate 1 并进 `gate-core` 那一族、TS 零：空目标原样交给后端，后端拒了照原话说）与 `设计/01 §5 D7`
 * 「失败要显式、归因要准确」（本机与远端的下一步不同，话就不许一样）。
 *
 * | 性质 | 判据 |
 * |---|---|
 * | TS 解码器读得懂后端真出的成品 —— 同一份跨语言金样，后端那侧 `capture_pane_tests::the_capture_product_matches_the_cross_language_golden` 对拍它（异源：Rust 构造器造、TS 解） | 「金样」 |
 * | 形状不对 ⇒ 抛（多一格 / 缺一格 / 类型不对），不猜 | 「形状不对」 |
 * | 〔DUP3〕界面不判目标名：空目标原样交给后端；后端 `invalid_args` ⇒ 各动作那句「后端不接受这个会话名」带后端原话 | 「空目标」 |
 * | 本机与远端同一条路（`<local>` 照样经通道问），通道不在时两句话不同、远端那句点得出是哪台 | 「本机」「通道不在」 |
 * | 拒绝码逐码一句、两两不同、带上会话名与后端原话；认不出的码原样带出去、不被猜成已知档（码集合取自金样，不是手抄） | 「拒绝码」 |
 * | 〔批 2〕结束会话 / 发按键：请求体 == 金样；`enter` 落在两个 mode 名上；`killed` / `typed` 不为真不当成功；门拒绝 ≠ 通道不在 | 「结束会话 · 发按键」两组 |
 * | 〔批 2〕就地 resume（F14）：只有能证明没发出去才回落 —— TS `provablyNotSent` == Rust `route_call_error`（跨语言金样 `reach-collapse.golden.json`，Rust 侧 `chan/webview_tests.rs` 产） | 「就地 resume」 |
 *
 * 买不到：真 Tauri IPC 与真后端（后端那一侧在 Rust 里；monitor 那一跳由 `webview_tests` 量）；真 tmux 会话上的一屏。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { invoke } from "@tauri-apps/api/core";
import { LOCAL_ORIGIN } from "../src/ipc/origin";
import {
  capturePane,
  ControlError,
  decodeCapture,
  decodeKilled,
  decodeTyped,
  killSession,
  sendInto,
  sendKeys,
} from "../src/tmux-control";
import { decodeFail } from "../src/comms/inward/chan";
import { provablyNotSent } from "../src/ipc/chan-caller";
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
const KILL = golden.kill;
const LAUNCH = golden.launch;

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

/** 跑一趟抓屏，拿抛出来的那一句。 */
const saidBy = (origin: string, target: string): Promise<string> => saidOf(() => capturePane(origin, target));

describe("〔C4e〕抓一屏：按形状收", () => {
  it("★★ 金样：解码器读得懂后端真出的成品；请求体就是金样那一份", async () => {
    expect(decodeCapture("aya", CAP.reply)).toBe(CAP.reply.screen);
    answer({ ok: CAP.reply });
    await expect(capturePane("aya", String(CAP.request.name))).resolves.toBe(CAP.reply.screen);
    expect(sentCalls()).toEqual([["aya", "capture-pane", CAP.request]]);
  });

  it("★ 空屏是合法的成功（刚建起来、什么都没打印的 pane），不是失败", () => {
    expect(decodeCapture("aya", { name: "demo-cc", screen: "" })).toBe("");
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
      expect(() => decodeCapture("aya", v), JSON.stringify(v)).toThrow(/读不懂/);
    }
  });
});

describe("〔C4e〕抓一屏：发出去之前", () => {
  it("★ 〔DUP3〕空目标不在界面判：原样交给后端（Gate 1 住 gate-core / 后端入口），后端拒了照原话说", async () => {
    answer({ fail: refusedReply("invalid_args", "`name` 为空") });
    const said = await saidBy("aya", "");
    expect(sentCalls(), "界面自己把空目标拦下了 —— Gate 1 在界面又长了一份").toEqual([["aya", "capture-pane", { name: "" }]]);
    expect(said).toMatch(/后端不接受这个会话名/);
    expect(said).toContain("`name` 为空");
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
    const said = await saidBy("aya", "demo-cc");
    expect(said).toMatch(/aya 的后端版本不对/);
  });

  it("★ 连接断了 / 等回复超时 ⇒ 说不知道做完没有；撤回 ⇒ 说撤回了", async () => {
    answer({ fail: { err: { Hop: { idx: 1, tag: "wait", reach: "Unknown", why: "Overrun" } }, body: [] } });
    expect(await saidBy("aya", "demo-cc")).toMatch(/不知道 aya 那边做完了没有/);
    answer({ fail: { err: { Ours: "Cancelled" }, body: [] } });
    expect(await saidBy("aya", "demo-cc")).toMatch(/撤回/);
  });

  it("★★ 拒绝码（取自金样）逐码一句、两两不同、带会话名与后端原话；认不出的码原样带出去、不被猜成已知档", async () => {
    const said: string[] = [];
    for (const code of CAP.codes) {
      answer({ fail: refusedReply(code, "RAW-WORDS") });
      said.push(await saidBy("aya", "demo-cc"));
    }
    expect(said.length, "金样里一个码都没有 —— 下面全是空转").toBeGreaterThan(0);
    expect(new Set(said).size, `有两档被压成了同一句：${JSON.stringify(said)}`).toBe(said.length);
    for (const s of said) {
      expect(s).toContain("demo-cc");
      expect(s, "后端的原话被吃掉了").toContain("RAW-WORDS");
    }
    answer({ fail: refusedReply("zzz_new_code", "RAW-WORDS") });
    const unknown = await saidBy("aya", "demo-cc");
    expect(unknown).toContain("zzz_new_code");
    expect(said).not.toContain(unknown);
    // 拒绝体读不出来 ⇒ 仍是一句「被拒」，不是空串、不是崩。
    answer({ fail: { err: "Refused", body: [0xff] } });
    expect(await saidBy("aya", "demo-cc")).toMatch(/被拒/);
  });
});

// ════════════════════════════════════════════════════════════════════════════
//  结束会话（`kill`）· 发按键（`launch{send-into}`）· 就地 resume（`launch{send-into}` ＋ F14）
// ════════════════════════════════════════════════════════════════════════════

/** 三个动作各自「跑一趟、拿那一句」。 */
const ACTIONS: [string, (origin: string, target: string) => Promise<unknown>][] = [
  ["结束会话", (o, t) => killSession(o, t)],
  ["发按键", (o, t) => sendKeys(o, t, "/exit")],
];

describe("〔C4e〕结束会话 · 发按键：按形状收", () => {
  it("★★ 金样：解码器读得懂后端真出的成品；请求体就是金样那一份", async () => {
    expect(() => decodeKilled("aya", "demo-cc", KILL.reply)).not.toThrow();
    expect(() => decodeTyped("aya", "demo-cc", LAUNCH.reply)).not.toThrow();
    answer({ ok: KILL.reply });
    await killSession("aya", String(KILL.request.name));
    answer({ ok: LAUNCH.reply });
    await sendKeys("aya", String(LAUNCH.request.name), String(LAUNCH.request.payload));
    expect(sentCalls()).toEqual([
      ["aya", "kill", KILL.request],
      ["aya", "launch", LAUNCH.request],
    ]);
  });

  it("★★ 发按键只有 `send-into` 一形（键入 ＋ 回车；〔RST 续 · V41〕裸键 mode 已删）", async () => {
    answer({ ok: LAUNCH.reply });
    await sendKeys("aya", "demo-cc", "/compact");
    const bodies = sentCalls().map(([, , b]) => b as Record<string, unknown>);
    expect(bodies.map((b) => [b.mode, b.payload])).toEqual([["send-into", "/compact"]]);
    for (const b of bodies) expect(Object.keys(b).sort(), "请求里多了一格（旧后端会静默忽略它）").toEqual(["mode", "name", "payload"]);
  });

  it("★★ 没明说做成了 ⇒ 不当成功：`killed` / `typed` 为假、多一格 / 缺一格 / 类型不对都抛", () => {
    expect(() => decodeKilled("aya", "demo-cc", { ...KILL.reply, killed: false })).toThrow(/没有确认 demo-cc 已经结束/);
    expect(() => decodeTyped("aya", "demo-cc", { ...LAUNCH.reply, typed: false })).toThrow(/没有确认按键已经送到 demo-cc/);
    for (const bad of [{ ...KILL.reply, extra: 1 }, { killed: true }, { session: "demo-cc", killed: "yes" }, null]) {
      expect(() => decodeKilled("aya", "demo-cc", bad), JSON.stringify(bad)).toThrow(/读不懂/);
    }
    for (const bad of [{ ...LAUNCH.reply, extra: 1 }, { session: "demo-cc", typed: true }, { ...LAUNCH.reply, created: 1 }]) {
      expect(() => decodeTyped("aya", "demo-cc", bad), JSON.stringify(bad)).toThrow(/读不懂/);
    }
  });
});

describe("FIX4 · 杀会话顺手注销的结局", () => {
  /** 设计/95 §6「杀会话顺手注销的结局只进日志：界面不说『顺手注销了谁 / 没注销成』；要说得给 kill 的成品加一格（界面、金样、文案同拍）」。 */
  it("★ bus 那一格 ⇒ 一句话：注销了谁 · 谁没注销成 · 名册读不到；全空不说；那一格缺 / 形状不对 ⇒ 读不懂", () => {
    const bus = (b: unknown) => ({ ...KILL.reply, bus: b });
    expect(decodeKilled("aya", "demo-cc", KILL.reply)).toBeNull();
    expect(decodeKilled("aya", "demo-cc", bus({ removed: ["p_cc", "q_cc"], failed: [], unread: null }))).toBe(
      "顺手从 cc-bus 名册里注销了 p_cc、q_cc。",
    );
    expect(decodeKilled("aya", "demo-cc", bus({ removed: [], failed: [{ id: "r_cc", why: "它不在" }], unread: null }))).toBe(
      "从 cc-bus 名册注销 r_cc 没成，名册里那一行还在：它不在",
    );
    expect(decodeKilled("aya", "demo-cc", bus({ removed: [], failed: [], unread: "cc-list 退出 1" }))).toBe(
      "读不到 cc-bus 名册，登记在这个会话上的 id 没注销：cc-list 退出 1",
    );
    for (const bad of [{ session: "demo-cc", killed: true }, bus({ removed: [], failed: [] }), bus({ removed: [1], failed: [], unread: null })]) {
      expect(() => decodeKilled("aya", "demo-cc", bad), JSON.stringify(bad)).toThrow(/读不懂/);
    }
  });
});

describe("〔C4e〕结束会话 · 发按键：发出去之前与失败怎么说", () => {
  it("★ 〔DUP3〕空目标不在界面判：原样交给后端，后端拒了照原话说（两个动作各一遍）", async () => {
    answer({ fail: refusedReply("invalid_args", "`name` 为空") });
    for (const [label, act] of ACTIONS) {
      const said = await saidOf(() => act("aya", ""));
      expect(said, label).toMatch(/后端不接受/);
      expect(said, label).toContain("`name` 为空");
    }
    expect(sentCalls().map(([, op, body]) => [op, (body as { name?: unknown }).name]), "界面自己把空目标拦下了").toEqual([
      ["kill", ""],
      ["launch", ""],
    ]);
  });

  it("★★ 通道不在：本机与远端两句话不同、远端点得出是哪台（两个动作各一遍）；本机照样经通道问", async () => {
    answer({ fail: NO_CHANNEL });
    for (const [label, act] of ACTIONS) {
      const local = await saidOf(() => act(LOCAL_ORIGIN, "demo-cc"));
      const remote = await saidOf(() => act("kr-remote-label", "demo-cc"));
      expect(local, label).not.toBe(remote);
      expect(local, label).toMatch(/本机后端/);
      expect(remote, label).toContain("kr-remote-label");
      for (const x of [local, remote]) expect(x, label).not.toMatch(/未找到远端配置/);
    }
    expect(sentCalls().filter(([o]) => o === LOCAL_ORIGIN).length, "本机没经通道问").toBe(2);
  });

  it("★★ 拒绝码（取自金样）逐码一句、两两不同、带会话名与后端原话；身份门那一句 ≠ 通道不在那一句", async () => {
    for (const [op, codes, act] of [
      ["kill", KILL.codes, (t: string) => killSession("aya", t)],
      ["launch", LAUNCH.codes, (t: string) => sendKeys("aya", t, "/exit")],
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
      expect(unknown, op).toContain("zzz_new_code");
      expect(said, op).not.toContain(unknown);
      // 三态不许压成两态：门拒绝（身份门）与通道不在是两句话。
      answer({ fail: refusedReply("wrong_owner", "RAW-WORDS") });
      const gate = await saidOf(() => act("demo-cc"));
      answer({ fail: NO_CHANNEL });
      expect(await saidOf(() => act("demo-cc")), op).not.toBe(gate);
    }
  });
});

describe("〔C4e〕就地 resume（F14：只有能证明没发出去才许回落）", () => {
  it("★★ 金样：Rust `route_call_error` 那一收拢 == TS `provablyNotSent`（逐行，同一份文件）", () => {
    const rc = JSON.parse(readFileSync(resolve(REPO_ROOT, "tests/__fixtures__/reach-collapse.golden.json"), "utf8")) as {
      rows: { case: string; fail: unknown; provablyNotSent: boolean }[];
    };
    expect(rc.rows.length, "金样一行都没有 —— 下面是空转").toBeGreaterThan(0);
    expect(new Set(rc.rows.map((r) => r.provablyNotSent)), "金样只有一种结论 —— 分不出两档就等于没测").toEqual(new Set([true, false]));
    for (const r of rc.rows) expect(provablyNotSent(decodeFail(r.fail)), r.case).toBe(r.provablyNotSent);
    // 本侧多出来的一档（Rust 那一侧不产）：期限在发之前就过了 ⇒ 一个字节都没发 ⇒ 能证明。
    expect(provablyNotSent({ layer: "hop", at: { idx: 0, tag: "write" }, reach: "NotSent", why: "Overrun" })).toBe(true);
  });

  it("★★ 三态：键入了 ⇒ typed · 能证明没发出去 ⇒ fallback · 对端说了话 / 拿不准 ⇒ refused（一律不抛）", async () => {
    answer({ ok: LAUNCH.reply });
    expect(await sendInto("aya", "demo-cc", "PAYLOAD")).toEqual({ verdict: "typed" });
    expect(sentCalls().at(-1)).toEqual(["aya", "launch", { mode: "send-into", name: "demo-cc", payload: "PAYLOAD" }]);
    answer({ fail: NO_CHANNEL });
    expect((await sendInto("aya", "demo-cc", "PAYLOAD")).verdict).toBe("fallback");
    answer({ fail: UNSUPPORTED });
    expect((await sendInto("aya", "demo-cc", "PAYLOAD")).verdict, "对端事前就说不认 ⇒ 一个字节没发").toBe("fallback");
    for (const fail of [
      refusedReply("wrong_owner", "m"),
      { err: { Hop: { idx: 1, tag: "wait", reach: "Unknown", why: "Overrun" } }, body: [] },
      { err: { Hop: { idx: 1, tag: "read", reach: "Unknown", why: "Dropped" } }, body: [] },
      "ipc closed",
    ]) {
      answer({ fail });
      expect((await sendInto("aya", "demo-cc", "PAYLOAD")).verdict, JSON.stringify(fail)).toBe("refused");
    }
    // 后端答了、但没明说键进去了 ⇒ 拿不准 ⇒ 不回落。
    answer({ ok: { ...LAUNCH.reply, typed: false } });
    expect((await sendInto("aya", "demo-cc", "PAYLOAD")).verdict).toBe("refused");
  });

  it("★ 〔FIX · `99 §2 ㊹`〕会话名或载荷为空 ⇒ 原样交给后端，后端拒 ⇒ refused（界面零判定）", async () => {
    answer({ fail: refusedReply("invalid_args", "name is empty") });
    expect((await sendInto("aya", "  ", "PAYLOAD")).verdict).toBe("refused");
    expect(sentCalls().at(-1)).toEqual(["aya", "launch", { mode: "send-into", name: "  ", payload: "PAYLOAD" }]);
    expect((await sendInto("aya", "demo-cc", "")).verdict).toBe("refused");
    expect(sentCalls().at(-1)).toEqual(["aya", "launch", { mode: "send-into", name: "demo-cc", payload: "" }]);
  });
});


describe("〔NET2 · 主会话 09-27 裁 A〕那台握手时说过做不到的，菜单置灰并说为什么", () => {
  it("kill 在「没有 tmux」的那台上不可点、字后面带原因；那台没说的项与没问过的机器照常", async () => {
    const { gateByOffer } = await import("../src/tab-menu");
    const { chan } = await import("../src/comms/inward/chan");
    invokeMock.mockImplementation((cmd: string, args: { origin?: string }) =>
      Promise.resolve(
        cmd === "chan_offer" && args.origin === "net2-box"
          ? { ops: ["kill", "capture-pane"], unavailable: [["kill", "no_tmux"]], stoppable: [] }
          : null,
      ),
    );
    await chan.offer("net2-box");
    const click = vi.fn();
    const kill = gateByOffer("net2-box", { id: "kill", label: "结束会话", onClick: click });
    expect(kill.enabled).toBe(false);
    expect(kill.label).toContain("没有 tmux");
    kill.onClick?.();
    expect(click).not.toHaveBeenCalled();
    const preview = { id: "preview", label: "预览", onClick: click };
    expect(gateByOffer("net2-box", preview)).toBe(preview);
    const elsewhere = { id: "kill", label: "结束会话", onClick: click };
    expect(gateByOffer("net2-other", elsewhere)).toBe(elsewhere);
  });
});
