/**
 * 手动对齐的界面那一半（`src/frontend/ui/resync.ts`）。
 *
 * 守的要求：「关卡 2 拒绝提示里『对齐后重试』（只对那一个会话：重验 ＋ 重打 ＋ 再过一次关卡）」·
 * 「本机远端同一条 `chan.call(origin, "resync", …)`」。
 *
 * | 性质 | 判据 |
 * |---|---|
 * | 请求体 / 成品 == 跨语言金样（后端 `watcher_tests::resync_face_reply_matches_the_cross_language_golden` 对同一份） | 「金样」 |
 * | 点「对齐后重试」⇒ 先对齐**那一个会话**、再做一次原动作；顺序就是这个 | 「对齐后重试」 |
 * | 对齐后固定条记录没了 ⇒ 标出来、说一句，**不自动摘**；点了才摘；问不到的不标 | 「固定条」 |
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const { toasts } = vi.hoisted(() => ({ toasts: [] as { headline: string; body: string; onClick?: () => void }[] }));
vi.mock("../../../src/frontend/ui/kit/toast", () => ({
  toast: (headline: string, body: string, opts: { onClick?: () => void } = {}) => {
    toasts.push({ headline, body, onClick: opts.onClick });
  },
}));

const { records } = vi.hoisted(() => ({ records: new Map<string, boolean | "throw">() }));
vi.mock("../../../src/frontend/ui/session-reads", () => ({
  probeSessionRecord: async (_origin: string, sid: string) => {
    const r = records.get(sid);
    if (r === "throw" || r === undefined) throw new Error("问不到");
    return { present: r, root: "/r" };
  },
}));
vi.mock("../../../src/frontend/ui/account-reads", () => ({ fetchAccounts: async () => ({ available: true, accounts: [] }) }));

import { invoke } from "@tauri-apps/api/core";
import { TabSessionActions } from "../../../src/frontend/ui/tab-session-actions";
import type { Tab } from "../../../src/frontend/ui/tab-model";
import { offerResyncRetry, resyncMachinesSaid } from "../../../src/frontend/ui/resync";
import { noteMachine } from "../../../src/frontend/ui/ipc/chan-caller";
import { LOCAL } from "../../../src/frontend/ui/ipc/origin";
import { copyText } from "../../../src/frontend/ui/copy-table";
import { resync } from "../../../src/frontend/ui/control-said";
import { REPO_ROOT } from "../../test-support/repo-root";
import { chanArgsJson, chanReply, type ChanCallArgs } from "../../test-support/chan-fake";

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
  it("★ 金样：带 sid 的请求体 == 金样那份；整机不带 sid；成品按类型用（形状由握手指纹管）", async () => {
    invokeMock.mockImplementation(async () => chanReply(golden.reply));
    // 那一句由后端写好（`said`），界面照抄，不再拿几格计数自己拼。
    expect(await resync("devbox", golden.request.sid)).toEqual({ said: golden.reply.said });
    await resync("devbox");
    expect(sentCalls()).toEqual([
      ["devbox", "resync", golden.request],
      ["devbox", "resync", {}],
    ]);
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

  it("★ 〔㉟①〕固定条记录没了 ⇒ 说一句（点名没了的那几条）、不自动摘；点那条才摘；问不到的不算", async () => {
    records.clear();
    records.set("s-gone", false).set("s-gone-2", false).set("s-here", true).set("s-dunno", "throw");
    const unpinned: string[] = [];
    const actions = new TabSessionActions({
      tab: () => undefined,
    });
    const tab = (sid: string) => ({ sessionId: sid, origin: "devbox", title: sid, pinned: true }) as unknown as Tab;
    await actions.flagPinsWithoutRecord("devbox", [tab("s-gone"), tab("s-here"), tab("s-gone-2"), tab("s-dunno")], (sid) => unpinned.push(sid));
    expect([toasts.length, unpinned]).toEqual([1, []]);
    // 点名的那几条用文案表的分隔符接（不写字面的逗号）。
    expect(toasts[0].body).toBe(copyText("sessionState.pinGone.body", { n: 2, names: ["s-gone", "s-gone-2"].join(copyText("kit.text.sep")) }));
    toasts[0].onClick!();
    expect(unpinned).toEqual(["s-gone", "s-gone-2"]);
  });

  it("★ 按台一行：称呼那台照它成品的名字（本机远端同一条，界面不自取「本机」）", () => {
    noteMachine(LOCAL, { name: "这台（夹具）", offlineSaid: "" });
    noteMachine("devbox", { name: "devbox（成品）", offlineSaid: "" });
    const said = resyncMachinesSaid([
      { origin: LOCAL, why: "why-a（夹具）" },
      { origin: "devbox", why: "why-b（夹具）" },
      { origin: "devbox", r: { said: "后端那一句（夹具）" } },
    ]);
    expect(said).toBe(
      [
        copyText("resync.machines.failed", { machine: "这台（夹具）", why: "why-a（夹具）" }),
        copyText("resync.machines.failed", { machine: "devbox（成品）", why: "why-b（夹具）" }),
        copyText("kit.text.machineSaid", { machine: "devbox（成品）", said: "后端那一句（夹具）" }),
      ].join("\n"),
    );
  });
});
