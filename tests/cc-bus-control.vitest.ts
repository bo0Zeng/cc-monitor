/**
 * 〔C4e · 第四波 4C〕界面直接说的 cc-bus 那几条帧命令（`src/cc-bus-control.ts`）的判据。
 *
 * 守的要求：`设计/05 §14.3` 逐字「正路是**把解释挪进后端、直接出成品**：后端帧命令的应答就是界面要的那个形状，
 * 前端经 `chan.call` 直接问、按形状收（不解释），monitor 那一份解释与发送点一起删」·「成品的两侧对拍：界面按形状严格收
 * （多一格 / 缺一格 / 类型不对 ⇒ 抛「两端契约对不上」，不猜）；线上形状由一份跨语言金样钉住」；
 * 另守 `INVARIANTS` 那条「调用方不能靠对端校验」（坏 id / 坏形状一个字节都不发，好的发得出去 —— 正反各一格）
 * 与 `设计/01 §5 D7`「失败要显式、归因要准确」（问不到 ≠ 不在线；发到几个 · 跳过几个 · 失败几个分开说）。
 *
 * | 性质 | 判据 |
 * |---|---|
 * | 五条的请求体 == 金样那一份；解码器读得懂后端真出的成品（后端那侧 `cc_bus_tests::the_bus_products_match_the_cross_language_golden` 对拍同一份，异源：Rust 构造器造、TS 解） | 「金样」 |
 * | agent id 规则 == monitor `is_valid_bus_id`（同一份 `ids`，Rust 侧 `the_bus_id_rule_agrees_with_the_front_end_on_the_shared_samples`） | 「id」 |
 * | 坏 id / 空正文 / 坏派生形状就地拒，一个字节都不发；好的发得出去 | 「发出去之前」 |
 * | 查在线只回确定的答案，问不到一律抛（结构上造不出灭灯） | 「查在线」 |
 * | 回值几态逐态一句、两两不同；破坏性的形状不认识 ⇒「不知道动没动」 | 「发消息」「收掉」「派生」「广播」 |
 * | 拒绝码（取自金样）逐码一句、两两不同、带后端原话；认不出的码原样带出去 | 「拒绝码」 |
 * | 本机与远端同一条路，通道不在时两句话不同 | 「本机」 |
 *
 * 买不到：真 Tauri IPC 与真后端（后端那一侧在 Rust 里，`tests/e2e/backend-cc-bus.sh` 真起过假 agent）；真机上的驾驶舱。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { invoke } from "@tauri-apps/api/core";
import { LOCAL_ORIGIN } from "../src/ipc/origin";
import {
  agentOnline,
  broadcast,
  checkSpawnShape,
  decodeAgents,
  isValidBusId,
  killAgent,
  MONITOR_BUS_ID,
  saidOfBroadcast,
  saidOfDelivery,
  saidOfKill,
  saidOfSpawn,
  sendMessage,
  spawnAgent,
} from "../src/cc-bus-control";
import { ControlError } from "../src/control-said";
import { REPO_ROOT } from "./test-support/repo-root";
import { chanArgsJson, chanReply, refusedReply, NO_CHANNEL, type ChanCallArgs } from "./test-support/chan-fake";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;

interface GoldenOp {
  request: Record<string, unknown>;
  reply: Record<string, unknown>;
  codes: string[];
}
const golden = JSON.parse(readFileSync(resolve(REPO_ROOT, "tests/__fixtures__/cc-bus-control.golden.json"), "utf8")) as Record<
  string,
  unknown
>;
const LIST = golden["bus-list"] as GoldenOp;
const SEND = golden["bus-send"] as GoldenOp;
const KILL = golden["bus-kill"] as GoldenOp;
const SPAWN = golden["bus-spawn"] as GoldenOp & { requestAccount: Record<string, unknown> };
const BCAST = golden["bus-broadcast"] as GoldenOp;
const IDS = golden.ids as { ok: string[]; bad: string[] };

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

/** 让那一跳按 `reply` 答（成品 ⇒ 字节；失败值 ⇒ 抛它）。 */
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

/** 同步版。 */
function thrownBy(act: () => unknown): string {
  try {
    act();
  } catch (e) {
    expect(e).toBeInstanceOf(ControlError);
    return (e as ControlError).message;
  }
  throw new Error("本该抛却没抛");
}

describe("〔C4e〕金样：请求体 == 金样、解码器读得懂后端真出的成品", () => {
  it("★★ 查在线（bus-list）", async () => {
    expect(decodeAgents("aya", LIST.reply).map((a) => [a.id, a.live])).toEqual([
      ["alpha_cc", true],
      ["gone_cc", false],
      ["x_cc", true],
    ]);
    answer({ ok: LIST.reply });
    await expect(agentOnline("aya", "alpha_cc")).resolves.toBe(true);
    await expect(agentOnline("aya", "gone_cc")).resolves.toBe(false);
    expect(sentCalls()[0]).toEqual(["aya", "bus-list", LIST.request]);
  });

  it("★★ 发消息（bus-send）：以 cc-monitor 的身份发", async () => {
    answer({ ok: SEND.reply });
    await expect(sendMessage("aya", String(SEND.request.to), String(SEND.request.text))).resolves.toMatch(/已投递给 alpha_cc/);
    expect(sentCalls()).toEqual([["aya", "bus-send", SEND.request]]);
    expect(SEND.request.from).toBe(MONITOR_BUS_ID);
  });

  it("★★ 收掉（bus-kill）", async () => {
    answer({ ok: KILL.reply });
    await expect(killAgent("aya", String(KILL.request.id))).resolves.toMatch(/已收掉 alpha_cc/);
    expect(sentCalls()).toEqual([["aya", "bus-kill", KILL.request]]);
  });

  it("★★ 派生（bus-spawn）：没选账号 ⇒ 显式 `base:true`；选了 ⇒ `account`，两样不同时出现", async () => {
    answer({ ok: SPAWN.reply });
    await expect(spawnAgent("aya", { tool: "claude", dir: "/w/proj", task: "t" })).resolves.toMatch(/已派生 proj_cc/);
    await spawnAgent("aya", { tool: "claude", dir: "/w/proj", task: "t", account: "" });
    await spawnAgent("aya", { tool: "claude", dir: "/w/proj", task: "t", account: "z" });
    expect(sentCalls()).toEqual([
      ["aya", "bus-spawn", SPAWN.request],
      ["aya", "bus-spawn", SPAWN.request],
      ["aya", "bus-spawn", SPAWN.requestAccount],
    ]);
  });

  it("★★ 广播（bus-broadcast）", async () => {
    answer({ ok: BCAST.reply });
    await expect(broadcast("aya", String(BCAST.request.text))).resolves.toMatch(/已广播给 1 个在线的 agent，跳过 1 个不在线的。1 个失败：x_cc（timed_out）/);
    expect(sentCalls()).toEqual([["aya", "bus-broadcast", BCAST.request]]);
  });

  it("★ 每一趟都显式交了期限（X6：调用点给）", async () => {
    answer({ ok: KILL.reply });
    await killAgent("aya", "alpha_cc");
    const leftMs = (invokeMock.mock.calls[0][1] as ChanCallArgs).leftMs;
    expect(leftMs, "期限没交").toBeGreaterThan(0);
  });

  it("★ 形状不对 ⇒ 抛（多一格 / 缺一格 / 类型不对），不猜", () => {
    const badList = [{ ...LIST.reply, extra: 1 }, { agents: [{ id: "a" }] }, { agents: "x" }, null];
    for (const v of badList) expect(() => decodeAgents("aya", v), JSON.stringify(v)).toThrow(ControlError);
    const badSend = [{ ...SEND.reply, extra: 1 }, { ...SEND.reply, sent: false }, { ...SEND.reply, registered: "yes" }];
    for (const v of badSend) expect(() => saidOfDelivery("aya", "alpha_cc", v), JSON.stringify(v)).toThrow(ControlError);
    const badBcast = [
      { ...BCAST.reply, extra: 1 },
      { ...BCAST.reply, sent: "1" },
      { ...BCAST.reply, failed: [{ id: "x_cc", code: "timed_out", message: "d" }] },
    ];
    for (const v of badBcast) expect(() => saidOfBroadcast("aya", v), JSON.stringify(v)).toThrow(ControlError);
  });
});

describe("〔C4e〕id：与 monitor 那一份同一条规则", () => {
  it("★★ 金样 `ids`：该放的放、该拒的拒（Rust 侧读同一份）", () => {
    expect(IDS.ok.length * IDS.bad.length, "金样的 ids 空了 —— 下面是空转").toBeGreaterThan(0);
    for (const id of IDS.ok) expect(isValidBusId(id), id).toBe(true);
    for (const id of IDS.bad) expect(isValidBusId(id), JSON.stringify(id)).toBe(false);
  });
});

describe("〔C4e〕发出去之前：调用方不能靠对端校验", () => {
  it("★★ 坏 id 就地拒、一个字节都不发（查在线 · 发消息 · 收掉）；好 id 发得出去", async () => {
    answer({ ok: KILL.reply });
    for (const bad of IDS.bad) {
      for (const act of [() => agentOnline("aya", bad), () => sendMessage("aya", bad, "hi"), () => killAgent("aya", bad)]) {
        expect(await saidOf(act)).toMatch(/agent 名不合法/);
      }
    }
    expect(invokeMock, "坏 id 也发出去了").not.toHaveBeenCalled();
    await killAgent("aya", "alpha_cc");
    expect(sentCalls().length, "好 id 没发出去 —— 判定被焊成恒拒了").toBe(1);
  });

  it("★ 空正文就地拒（发消息 · 广播），一个字节都不发", async () => {
    answer({ ok: SEND.reply });
    expect(await saidOf(() => sendMessage("aya", "alpha_cc", "  \n"))).toMatch(/消息是空的/);
    expect(await saidOf(() => broadcast("aya", ""))).toMatch(/广播内容是空的/);
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("★★ 派生的形状先核：tool / 目录非空、账号名过字符集；**不白名单 tool**（认不认归 cc-spawn）", async () => {
    answer({ ok: SPAWN.reply });
    expect(thrownBy(() => checkSpawnShape({ tool: "", dir: "/w", task: "" }))).toMatch(/没选派生哪种 agent/);
    expect(thrownBy(() => checkSpawnShape({ tool: "claude", dir: " ", task: "" }))).toMatch(/工作目录是空的/);
    for (const bad of ["--help", "a b", "a;rm"]) {
      expect(await saidOf(() => spawnAgent("aya", { tool: "claude", dir: "/w", task: "", account: bad }))).toMatch(/账号名不合法/);
    }
    expect(invokeMock, "坏形状也发出去了").not.toHaveBeenCalled();
    // 没见过的 tool 照样放行 —— 本侧不维护第二份名单。
    expect(() => checkSpawnShape({ tool: "some-future-agent", dir: "/w", task: "" })).not.toThrow();
    await spawnAgent("aya", { tool: "some-future-agent", dir: "/w", task: "", account: "z" });
    expect(sentCalls().length).toBe(1);
  });
});

describe("〔C4e〕查在线：问不到 ≠ 不在线", () => {
  it("★★ 不在名单里 / `live` 是 null / 通道不在 / 被拒 ⇒ 一律抛，没有一条路回 false", async () => {
    const withNull = { agents: [{ id: "n_cc", target: "n_cc:0.0", unread: 0, live: null, ccm_sid: null }] };
    const cases: [string, { ok: unknown } | { fail: unknown }, string][] = [
      ["不在名单里", { ok: LIST.reply }, "nobody_cc"],
      ["live 是 null", { ok: withNull }, "n_cc"],
      ["通道不在", { fail: NO_CHANNEL }, "alpha_cc"],
      ["被拒", { fail: refusedReply("timed_out", "RAW") }, "alpha_cc"],
    ];
    for (const [what, reply, id] of cases) {
      answer(reply);
      const said = await saidOf(() => agentOnline("aya", id));
      expect(said, what).toMatch(/问不到|找不到 aya 的后端|连不上|不在/);
      expect(said, `${what}：说成了「不在线」`).not.toMatch(/^.*当前不在线/);
    }
    answer({ ok: withNull });
    expect(await saidOf(() => agentOnline("aya", "n_cc"))).toMatch(/这是问不到，不是不在线/);
  });
});

describe("〔C4e〕回值几态逐态一句", () => {
  it("★★ 发消息：在线 / 不在线 / 没登记过 / 问不到在不在线 —— 四句两两不同", () => {
    const said = [
      saidOfDelivery("aya", "a_cc", { ...SEND.reply, live: true }),
      saidOfDelivery("aya", "a_cc", { ...SEND.reply, live: false }),
      saidOfDelivery("aya", "a_cc", { ...SEND.reply, registered: false, live: null }),
      saidOfDelivery("aya", "a_cc", { ...SEND.reply, live: null }),
    ];
    expect(new Set(said).size, JSON.stringify(said)).toBe(4);
    expect(said[1]).toMatch(/不在线/);
    expect(said[2]).toMatch(/没在总线上登记过/);
    expect(said[3]).toMatch(/问不到/);
  });

  it("★★ 收掉：真收了 / 只摘了登记 / 两样都没发生 三句不同；形状不认识 ⇒「不知道收掉了没有」而不是「没收掉」", () => {
    const said = [
      saidOfKill("aya", "a_cc", { id: "a_cc", killed: true, stale_only: false }),
      saidOfKill("aya", "a_cc", { id: "a_cc", killed: false, stale_only: true }),
      saidOfKill("aya", "a_cc", { id: "a_cc", killed: false, stale_only: false }),
    ];
    expect(new Set(said).size, JSON.stringify(said)).toBe(3);
    expect(said[1]).toMatch(/会话没动/);
    const unsure = thrownBy(() => saidOfKill("aya", "a_cc", { id: "a_cc", killed: "yes" }));
    expect(unsure).toMatch(/不知道 a_cc 收掉了没有/);
    expect(unsure).toMatch(/不要直接重来/);
  });

  it("★★ 派生：认不出名字 ≠ 没起来（明说别重试）；形状不认识 ⇒「不确定起没起」", () => {
    const done = saidOfSpawn(SPAWN.reply);
    const noName = saidOfSpawn({ ...SPAWN.reply, id: null });
    expect(done).toMatch(/已派生 proj_cc/);
    expect(noName).toMatch(/已派生，但没认出新会话的名字/);
    expect(noName).toMatch(/不要重试/);
    expect(noName).not.toMatch(/失败|没有派生/);
    const unsure = thrownBy(() => saidOfSpawn({ ...SPAWN.reply, spawned: false }));
    expect(unsure).toMatch(/不确定会话有没有起来/);
  });

  it("★★ 广播：发到几个 · 跳过几个 · 失败几个分开说；问不到谁在线时明说全发了 —— 四形两两不同", () => {
    const base = { sent: 3, skipped_offline: 2, liveness_unknown: false, failed: [] as unknown[] };
    const one = [{ id: "x_cc", error: "timed_out", detail: "d" }];
    const said = [
      saidOfBroadcast("aya", base),
      saidOfBroadcast("aya", { ...base, failed: one }),
      saidOfBroadcast("aya", { ...base, skipped_offline: 0, liveness_unknown: true }),
      saidOfBroadcast("aya", { ...base, skipped_offline: 0, liveness_unknown: true, failed: one }),
    ];
    expect(new Set(said).size, JSON.stringify(said)).toBe(4);
    expect(said[0]).toMatch(/已广播给 3 个在线的 agent，跳过 2 个不在线的/);
    expect(said[1]).toMatch(/1 个失败：x_cc（timed_out）/);
    expect(said[2]).toMatch(/问不到谁在线，所以全发了/);
    expect(said[3]).toMatch(/所以全发了。1 个失败/);
  });
});

describe("〔C4e〕失败怎么说", () => {
  it("★★ 拒绝码（取自金样）逐码一句、两两不同、带后端原话；认不出的码原样带出去", async () => {
    const drives: [string, GoldenOp, () => Promise<unknown>][] = [
      ["bus-list", LIST, () => agentOnline("aya", "alpha_cc")],
      ["bus-send", SEND, () => sendMessage("aya", "alpha_cc", "hi")],
      ["bus-kill", KILL, () => killAgent("aya", "alpha_cc")],
      ["bus-spawn", SPAWN, () => spawnAgent("aya", { tool: "claude", dir: "/w", task: "" })],
      ["bus-broadcast", BCAST, () => broadcast("aya", "hi")],
    ];
    for (const [op, g, act] of drives) {
      const said: string[] = [];
      for (const code of g.codes) {
        answer({ fail: refusedReply(code, "RAW-WORDS") });
        said.push(await saidOf(act));
      }
      expect(said.length, `${op}：金样里一个码都没有`).toBeGreaterThan(0);
      expect(new Set(said).size, `${op}：有两档被压成了同一句：${JSON.stringify(said)}`).toBe(said.length);
      for (const s of said) expect(s, `${op}：后端的原话被吃掉了`).toContain("RAW-WORDS");
      answer({ fail: refusedReply("zzz_new_code", "RAW-WORDS") });
      const unknown = await saidOf(act);
      expect(unknown, op).toContain("zzz_new_code");
      expect(said, op).not.toContain(unknown);
      answer({ fail: { err: "Refused", body: [0xff] } });
      expect(await saidOf(act), `${op}：拒绝体读不出来时没说出一句`).not.toBe("");
    }
  });

  it("★ 本机照样经通道问；通道不在时本机与远端两句话不同，远端那句点得出是哪台", async () => {
    answer({ ok: KILL.reply });
    await killAgent(LOCAL_ORIGIN, "alpha_cc");
    expect(sentCalls()).toEqual([[LOCAL_ORIGIN, "bus-kill", KILL.request]]);
    answer({ fail: NO_CHANNEL });
    const local = await saidOf(() => sendMessage(LOCAL_ORIGIN, "alpha_cc", "hi"));
    const remote = await saidOf(() => sendMessage("kr-remote-label", "alpha_cc", "hi"));
    expect(local).not.toBe(remote);
    expect(remote).toContain("kr-remote-label");
  });
});
