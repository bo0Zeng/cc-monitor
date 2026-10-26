/**
 * 界面直接说的 cc-bus 那几条帧命令（`src/frontend/ui/cc-bus-control.ts`）的判据。界面只读：名单 · 查在线 · 收件箱，不发信。
 *
 * 守的要求：「正路是**把解释挪进后端、直接出成品**：后端帧命令的应答就是界面要的那个形状，
 * 前端经 `chan.call` 直接问、按形状收（不解释）」·「成品的两侧对拍：线上形状由一份跨语言金样钉住」；
 * 另守「id 的形状由后端在交给 cc-bus 之前判，界面只把 `bad_id` 说成人话」与「失败要显式、归因要准确」（问不到 ≠ 不在线）。
 *
 * | 性质 | 判据 |
 * |---|---|
 * | 请求体 == 金样那一份；解码器读得懂后端真出的成品（后端那侧 `cc_bus_tests::the_bus_products_match_the_cross_language_golden` 对拍同一份） | 「金样」 |
 * | 界面不判 id：金样 `ids` 里坏的也原样交给后端；后端回 `bad_id` ⇒ 照它写好的那一句说 | 「id」 |
 * | 查在线只回确定的答案，问不到一律抛（结构上造不出灭灯） | 「查在线」 |
 * | 拒绝码（取自金样）逐码原样上屏、复制详情带码 | 「拒绝码」 |
 * | 本机与远端同一条路，通道不在时原样取 monitor 那一句 | 「本机」 |
 *
 * 买不到：真 Tauri IPC 与真后端（后端那一侧在 Rust 里，`tests/e2e/backend-cc-bus.sh` 真起过假 agent）；真机上的驾驶舱。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { invoke } from "@tauri-apps/api/core";
import { LOCAL_ORIGIN } from "../../../src/frontend/ui/ipc/origin";
import { agentOnline, readInbox } from "../../../src/frontend/ui/cc-bus-control";
import { detailOf as errDetail, writtenSaid } from "../../../src/frontend/ui/kit/detail";
import { REPO_ROOT } from "../../test-support/repo-root";
import { chanArgsJson, chanReply, refusedReply, NO_CHANNEL, type ChanCallArgs } from "../../test-support/chan-fake";
import { copyText, type CopyKey } from "../../../src/frontend/ui/copy-table";
import { copyPattern } from "../../test-support/copy-pattern";
import { nonEmpty } from "../../test-support/non-empty";

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
    const said = writtenSaid(e);
    expect(said, "抛的不是带那一句的错 —— 调用方拿不到那一句").not.toBeNull();
    return said ?? "";
  }
  throw new Error("本该失败却成功了");
}

/** 失败那一下的诊断（可复制的那一份）：错误码在这里，不在给人看的那一句里。 */
async function detailOf(act: () => Promise<unknown>): Promise<string> {
  try {
    await act();
  } catch (e) {
    return errDetail(e);
  }
  throw new Error("本该失败却成功了");
}

describe("〔C4e〕金样：请求体 == 金样、解码器读得懂后端真出的成品", () => {
  it("★★ 查在线（bus-list）", async () => {
    answer({ ok: LIST.reply });
    await expect(agentOnline("devbox", "aaaaaaaa-0000-4000-8000-000000000001")).resolves.toBe(true);
    await expect(agentOnline("devbox", "bbbbbbbb-0000-4000-8000-000000000002")).resolves.toBe(false);
    // 后端判不了（这台没有它的记录）⇒ `live` 是 null ⇒ 抛，不说成「不在线」。
    await expect(agentOnline("devbox", "cccccccc-0000-4000-8000-000000000003")).rejects.toThrow();
    expect(sentCalls()[0]).toEqual(["devbox", "bus-list", LIST.request]);
  });
});

describe("〔DUP2 · J12〕id：界面不判，后端判（规则只有一份）", () => {
  it("★★ 金样 `ids` 里坏的 id 也原样交给后端；后端回 `bad_id` ⇒ 照它写好的那一句说", async () => {
    nonEmpty(IDS.ok);
    nonEmpty(IDS.bad);
    const bad = IDS.bad.find((id) => id === "--help")!;
    answer({ fail: refusedReply("bad_id", "BACKEND-SAYS") });
    expect(await saidOf(() => readInbox("devbox", bad)), "那台写好的那一句没原样上屏").toBe("BACKEND-SAYS");
    expect(sentCalls().map((c) => c[1]), "界面自己把坏 id 拦下了 —— 规则只许后端那一份").toEqual(["bus-inbox"]);
    expect((sentCalls()[0][2] as Record<string, unknown>).id, "交给后端的不是原样那个 id").toBe(bad);
  });
});

describe("〔C4e〕查在线：问不到 ≠ 不在线", () => {
  it("★★ 不在名单里 / `live` 是 null / 通道不在 / 被拒 ⇒ 一律抛，没有一条路回 false", async () => {
    const withNull = { agents: [{ id: "n_cc", title: "n_cc", unread: 0, live: null }] };
    const cases: [string, { ok: unknown } | { fail: unknown }, string, CopyKey][] = [
      ["不在名单里", { ok: LIST.reply }, "nobody_cc", "ccBus.online.unknown"],
      ["live 是 null", { ok: withNull }, "n_cc", "ccBus.online.unknown"],
      ["通道不在", { fail: NO_CHANNEL }, "aaaaaaaa-0000-4000-8000-000000000001", "rsChanSaid.hop.localDown"],
      ["被拒", { fail: refusedReply("child_timed_out", copyText("ccBus.online.timedOut")) }, "aaaaaaaa-0000-4000-8000-000000000001", "ccBus.online.timedOut"],
    ];
    for (const [what, reply, id, key] of cases) {
      answer(reply);
      const said = await saidOf(() => agentOnline("devbox", id));
      expect(said, what).toMatch(copyPattern(key));
      expect(said, `${what}：说成了「不在线」`).not.toMatch(copyPattern("ccBus.check.offline"));
    }
  });
});

describe("〔C4e〕失败怎么说", () => {
  it("★★ 拒绝码（取自金样）：那台写好的那一句原样上屏、复制详情带码（逐码一句由后端判：`said_tests.rs`）", async () => {
    nonEmpty(LIST.codes);
    for (const code of LIST.codes) {
      answer({ fail: refusedReply(code, `S-${code}`) });
      expect(await saidOf(() => agentOnline("devbox", "aaaaaaaa-0000-4000-8000-000000000001")), code).toBe(`S-${code}`);
      answer({ fail: refusedReply(code, `S-${code}`) });
      expect(await detailOf(() => agentOnline("devbox", "aaaaaaaa-0000-4000-8000-000000000001")), `${code}：复制详情里没有码`).toContain(code);
    }
  });

  it("★ 本机照样经通道问；通道不在时原样取 monitor 写的那一句（本机远端怎么称呼由它说）", async () => {
    answer({ ok: LIST.reply });
    await agentOnline(LOCAL_ORIGIN, "aaaaaaaa-0000-4000-8000-000000000001");
    expect(sentCalls()).toEqual([[LOCAL_ORIGIN, "bus-list", LIST.request]]);
    answer({ fail: { ...NO_CHANNEL, said: "M-kr-remote-label" } });
    expect(await saidOf(() => agentOnline("kr-remote-label", "aaaaaaaa-0000-4000-8000-000000000001"))).toBe("M-kr-remote-label");
  });
});
