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

import { invoke } from "@tauri-apps/api/core";
import { ControlError } from "../src/control-said";
import { decodeResynced, isIdentityRefusal, offerResyncRetry, resync } from "../src/resync";
import { killSession } from "../src/tmux-control";
import { REPO_ROOT } from "./test-support/repo-root";
import { chanArgsJson, chanReply, refusedReply, type ChanCallArgs } from "./test-support/chan-fake";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;
const golden = JSON.parse(readFileSync(resolve(REPO_ROOT, "tests/__fixtures__/resync.golden.json"), "utf8")) as {
  request: { sid: string };
  reply: Record<string, number>;
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
    expect(await resync("devbox", golden.request.sid)).toEqual(golden.reply);
    await resync("devbox");
    expect(sentCalls()).toEqual([
      ["devbox", "resync", golden.request],
      ["devbox", "resync", {}],
    ]);
    expect(() => decodeResynced("devbox", { ...golden.reply, extra: 1 })).toThrow(ControlError);
    expect(() => decodeResynced("devbox", { ...golden.reply, added: -1 })).toThrow(ControlError);
  });

  it("★ 认得出关卡 2：真 `killSession` 被 `wrong_owner` 拒 ⇒ 认；`no_such_session` ⇒ 不认", async () => {
    const thrown = async (code: string): Promise<unknown> => {
      invokeMock.mockImplementation(async () => {
        throw refusedReply(code, "m");
      });
      try {
        await killSession("devbox", "proj");
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
    offerResyncRetry("devbox", "sid-7", "没结束", "被拒了", async () => {
      order.push("again");
    });
    expect(toasts).toHaveLength(1);
    expect(order).toEqual([]);
    toasts[0].onClick!();
    await flush();
    await flush();
    expect(order).toEqual(['resync {"sid":"sid-7"}', "again"]);
  });
});
