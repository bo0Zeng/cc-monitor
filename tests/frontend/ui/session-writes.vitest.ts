/**
 * 要求：「删会话 · 分叉仍经 monitor 这一跳转 …… 要先像广播那样收进后端，界面才谈得上直问」·
 * `§14.3` C 组（读会话正文 · 子 agent · 删会话 · 分叉「本机远端同一条路问那台后端」）。
 *
 * 界面经通道直说那台后端：删 `files-delete-session`（`src/frontend/ui/session-writes.ts`）；分叉源会话那三格的解码也住这里（起新会话框读它）。
 * ① 三格两侧对拍：后端测试产出金样 `tests/__fixtures__/session-fork.golden.json`（含 `launch` 那三格），这里的解码器读同一份；
 * ② 删会话只交 sid。夹具只造结构，不含真会话。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

const sent: { origin: string; op: string; body: unknown; until: unknown }[] = [];
let reply: unknown = null;
vi.mock("../../../src/comms/inward/chan", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../../../src/comms/inward/chan")>()),
  chan: {
    call: async (origin: string, op: string, body: Uint8Array, budget: { until: number }) => {
      sent.push({ origin, op, body: JSON.parse(new TextDecoder().decode(body)), until: budget?.until });
      return new TextEncoder().encode(JSON.stringify(reply));
    },
  },
}));

import golden from "../../__fixtures__/session-fork.golden.json";
import { decodeForkLaunch, deleteSession } from "../../../src/frontend/ui/session-writes";

beforeEach(() => {
  sent.length = 0;
});

describe("分叉源会话的三格（起新会话框读它；形状同后端 `session-fork` 金样的 `launch`）", () => {
  it("★ 金样的三格原样收下；多一格 / 缺一格 / 类型不对 / 码认不出 ⇒ 抛", () => {
    const { launch } = golden.product;
    expect(decodeForkLaunch(launch)).toEqual(launch);
    for (const bad of [
      { ...launch, extra: 1 },
      { cwd: launch.cwd, account: launch.account },
      { ...launch, cwd: { kind: "known", value: 1, from: "record" } },
      { ...launch, account: { kind: "unknown", why: "guessed" } },
      { ...launch, account: { kind: "known", value: "z", from: "nowhere" } },
      { ...launch, terminal: { kind: "known", value: { terminal: "t" }, from: "terminal_list" } },
      null,
    ]) {
      expect(() => decodeForkLaunch(bad), JSON.stringify(bad)).toThrow(/shape mismatch/);
    }
  });

  it("「源会话已退出」那一形（金样另一份）同样收下；号的值 null ＝ 账号 0、终端 `{host:\"none\"}` 也收", () => {
    expect(decodeForkLaunch(golden.launchExited)).toEqual(golden.launchExited);
    const base = {
      ...golden.product.launch,
      account: { kind: "known", value: null, from: "process" },
      terminal: { kind: "known", value: { host: "none" }, from: "terminal_list" },
    };
    expect(decodeForkLaunch(base).account).toEqual({ kind: "known", value: null, from: "process" });
  });
});

describe("MIG-3b 删会话经通道直说那台后端", () => {
  it("只交 sid；成品恰好一格 `path`，别的形状 ⇒ 抛", async () => {
    reply = { path: "/p/s1.jsonl" };
    await deleteSession("<local>", "s1");
    expect(sent).toEqual([{ origin: "<local>", op: "files-delete-session", body: { sid: "s1" }, until: sent[0].until }]);
    reply = { path: "/p", extra: 1 };
    await expect(deleteSession("devbox", "s1")).rejects.toThrow(/shape mismatch/);
  });
});
