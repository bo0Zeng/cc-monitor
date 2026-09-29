/**
 * 〔RESYNC · V149〕手动对齐的界面那一半（`src/resync.ts`）。
 *
 * 守的要求：`设计/15 §4.1b` 原文「关卡 2 拒绝提示里『对齐后重试』（只对那一个会话：重验 ＋ 重打 ＋ 再过一次关卡）」·
 * 「本机远端同一条 `chan.call(origin, "resync", …)`」。
 *
 * | 性质 | 判据 |
 * |---|---|
 * | 请求体 / 成品 == 跨语言金样（后端 `watcher_tests::resync_face_reply_matches_the_cross_language_golden` 对同一份） | 「金样」 |
 * | 关卡 2 的拒绝（真 `killSession` 抛出来的那个）认得出，别的拒绝不认 | 「认得出关卡 2」 |
 * | 点「对齐后重试」⇒ 先对齐**那一个会话**、再做一次原动作；顺序就是这个 | 「对齐后重试」 |
 * | 〔`99 §2.1` ㉟①〕对齐后固定条记录没了 ⇒ 标出来、说一句，**不自动摘**；点了才摘；问不到的不标 | 「固定条」 |
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const { toasts } = vi.hoisted(() => ({ toasts: [] as { headline: string; body: string; onClick?: () => void }[] }));
vi.mock("../src/error-toast", () => ({
  showActionFailureToast: (headline: string, body: string, opts: { onClick?: () => void } = {}) => {
    toasts.push({ headline, body, onClick: opts.onClick });
  },
}));

const { records } = vi.hoisted(() => ({ records: new Map<string, boolean | "throw">() }));
vi.mock("../src/session-reads", () => ({
  probeSessionRecord: async (_origin: string, sid: string) => {
    const r = records.get(sid);
    if (r === "throw" || r === undefined) throw new Error("问不到");
    return { present: r, root: "/r" };
  },
}));
vi.mock("../src/account-reads", () => ({ fetchAccounts: async () => ({ available: true, accounts: [] }) }));
vi.mock("../src/history-reads", () => ({ lastAccounts: async () => ({}) }));

import { invoke } from "@tauri-apps/api/core";
import { TabSessionActions } from "../src/tab-session-actions";
import type { Tab } from "../src/tab-model";
import { ControlError } from "../src/control-said";
import { decodeResynced, isIdentityRefusal, offerResyncRetry, resync } from "../src/resync";
import { killSession } from "../src/tmux-control";
import { REPO_ROOT } from "./test-support/repo-root";
import { chanArgsJson, chanReply, refusedReply, type ChanCallArgs } from "./test-support/chan-fake";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;
const golden = JSON.parse(readFileSync(resolve(REPO_ROOT, "tests/__fixtures__/resync.golden.json"), "utf8")) as {
  request: { sid: string };
  reply: Record<string, unknown>;
};

beforeEach(() => {
  invokeMock.mockReset();
  toasts.length = 0;
});

function sentCalls(): [string, string, unknown][] {
  return invokeMock.mock.calls
    .filter(([cmd]) => cmd === "chan_call")
    .map(([, a]) => {
      const args = a as ChanCallArgs;
      return [args.origin, args.op, chanArgsJson(args)];
    });
}

const flush = () => new Promise((r) => setTimeout(r, 0));

describe("〔RESYNC〕手动对齐", () => {
  it("★ 金样：带 sid 的请求体 == 金样那份；整机不带 sid；成品按金样解", async () => {
    invokeMock.mockImplementation(async () => chanReply(golden.reply));
    const { added, removed, retagged, caught_up: caughtUp, watchers } = golden.reply as Record<string, number>;
    expect(await resync("aya", golden.request.sid)).toEqual({ added, removed, retagged, caughtUp, watchers });
    await resync("aya");
    expect(sentCalls()).toEqual([
      ["aya", "resync", golden.request],
      ["aya", "resync", {}],
    ]);
    expect(() => decodeResynced("aya", { ...golden.reply, extra: 1 })).toThrow(ControlError);
    expect(() => decodeResynced("aya", { ...golden.reply, added: -1 })).toThrow(ControlError);
  });

  /** 主会话 09-28 裁 FIX4 ⑥（V149 手动兜底）：「本机 PATH 探针的 5 分钟缓存在『重新对齐』时作废，不再多等」。 */
  it("FIX4 ⑥：本机整机对齐那一下作废本机 ccm 那份缓存；远端 · 只对一个会话的不碰", async () => {
    invokeMock.mockImplementation(async (cmd: string) => (cmd === "chan_call" ? chanReply(golden.reply) : { ok: null, summary: "" }));
    await resync("aya");
    await resync("<local>", golden.request.sid);
    await resync("<local>");
    await flush();
    const probes = invokeMock.mock.calls.filter(([cmd]) => cmd === "local_ccm_entry_status");
    expect(probes).toEqual([["local_ccm_entry_status", { fresh: true }]]);
  });

  it("★ 认得出关卡 2：真 `killSession` 被 `wrong_owner` 拒 ⇒ 认；`no_such_session` ⇒ 不认", async () => {
    const thrown = async (code: string): Promise<unknown> => {
      invokeMock.mockImplementation(async () => {
        throw refusedReply(code, "m");
      });
      try {
        await killSession("aya", "proj");
      } catch (e) {
        return e;
      }
      throw new Error("本该失败");
    };
    expect([isIdentityRefusal(await thrown("wrong_owner")), isIdentityRefusal(await thrown("no_such_session"))]).toEqual([true, false]);
  });

  it("★★ 对齐后重试：点那条提示 ⇒ 先只对那一个会话对齐，再做一次原动作", async () => {
    const order: string[] = [];
    invokeMock.mockImplementation(async (_cmd: string, a: ChanCallArgs) => {
      order.push(`${a.op} ${JSON.stringify(chanArgsJson(a))}`);
      return chanReply(golden.reply);
    });
    offerResyncRetry("aya", "sid-7", "没结束", "被拒了", async () => {
      order.push("again");
    });
    expect(toasts).toHaveLength(1);
    expect(order).toEqual([]);
    toasts[0].onClick!();
    await flush();
    await flush();
    expect(order).toEqual(['resync {"sid":"sid-7"}', "again"]);
  });

  it("★ 〔㉟①〕固定条记录没了 ⇒ 标出来、说一句、不自动摘；点那条才摘；问不到的不标", async () => {
    records.clear();
    records.set("s-gone", false).set("s-here", true).set("s-dunno", "throw");
    const marked: [string, boolean][] = [];
    const unpinned: string[] = [];
    const actions = new TabSessionActions({
      tab: () => undefined,
      isAttachable: () => false,
      sessionAccount: () => undefined,
      refreshAccountBadgeFor: () => {},
      markRecord: (sid, present) => marked.push([sid, present]),
    });
    const tab = (sid: string) => ({ sessionId: sid, origin: "aya", title: sid, pinned: true }) as unknown as Tab;
    await actions.flagPinsWithoutRecord("aya", [tab("s-gone"), tab("s-here"), tab("s-dunno")], (sid) => unpinned.push(sid));
    expect(marked).toEqual([
      ["s-gone", false],
      ["s-here", true],
    ]);
    expect([toasts.length, unpinned]).toEqual([1, []]);
    toasts[0].onClick!();
    expect(unpinned).toEqual(["s-gone"]);
  });
});
