/**
 * 驾驶舱读面（`bus-state` · `bus-inbox`）的界面那一侧：按形状严格收、经通道问、拒绝码逐码一句。
 * 要求：「成品的两侧对拍 … 后端测试产出 == 金样 · TS 解码器读同一份」· 驾驶舱读面经通道。
 * 金样 `tests/__fixtures__/cc-bus-read.golden.json` 与后端 `cc_bus_tests.rs::the_cockpit_read_products_match_the_cross_language_golden` 读同一份。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { invoke } from "@tauri-apps/api/core";
import { decodeInbox, decodeState, readInbox, readState } from "../../../src/frontend/ui/cc-bus-control";
import { ControlError } from "../../../src/frontend/ui/control-said";
import { REPO_ROOT } from "../../test-support/repo-root";
import { chanArgsJson, chanReply, refusedReply, type ChanCallArgs } from "../../test-support/chan-fake";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;
const golden = JSON.parse(readFileSync(resolve(REPO_ROOT, "tests/__fixtures__/cc-bus-read.golden.json"), "utf8")) as {
  state: { reply: Record<string, unknown>; codes: string[] };
  inbox: { reply: Record<string, unknown>; codes: string[] };
};

beforeEach(() => {
  invokeMock.mockReset();
});

describe("SH1 · 驾驶舱读面：按形状严格收", () => {
  it("金样那两份成品原样收下", () => {
    expect(decodeState("devbox", golden.state.reply)).toEqual(golden.state.reply);
    expect(decodeInbox("devbox", golden.inbox.reply)).toEqual(golden.inbox.reply);
  });

  it("多一格 / 缺一格 / 类型不对 ⇒ 抛（不猜）", () => {
    const st = golden.state.reply as { agents: Record<string, unknown>[]; spawned: Record<string, unknown>[] };
    const bad: [string, unknown][] = [
      ["顶层多一格", { ...golden.state.reply, extra: 1 }],
      ["顶层缺 skipped", { agents: [], spawned: [] }],
      ["名册行缺登记时间", { ...golden.state.reply, agents: [{ ...st.agents[0], registered_at: undefined }] }],
      ["名册行 live 不是三态", { ...golden.state.reply, agents: [{ ...st.agents[0], live: "yes" }] }],
      ["台账行多一格", { ...golden.state.reply, spawned: [{ ...st.spawned[0], pid: 1 }] }],
    ];
    for (const [why, v] of bad) expect(() => decodeState("devbox", v), why).toThrow(ControlError);
    const ib = golden.inbox.reply as { messages: Record<string, unknown>[] };
    expect(() => decodeInbox("devbox", { ...golden.inbox.reply, truncated: "no" })).toThrow(ControlError);
    expect(() => decodeInbox("devbox", { ...golden.inbox.reply, messages: [{ ...ib.messages[0], kind: "msg" }] })).toThrow(ControlError);
  });
});

describe("SH1 · 驾驶舱读面：经通道问、失败说清", () => {
  it("读名册问的是那台的 `bus-state`、读收件箱问 `bus-inbox` 并带 id", async () => {
    invokeMock.mockImplementation(async (cmd: string, a: ChanCallArgs) => {
      if (cmd !== "chan_call") throw new Error(`不该调 ${cmd}`);
      return chanReply(a.op === "bus-state" ? golden.state.reply : golden.inbox.reply);
    });
    await expect(readState("devbox")).resolves.toEqual(golden.state.reply);
    await expect(readInbox("devbox", "alpha_cc")).resolves.toEqual(golden.inbox.reply);
    const sent = invokeMock.mock.calls.map(([, a]) => [(a as ChanCallArgs).origin, (a as ChanCallArgs).op, chanArgsJson(a as ChanCallArgs)]);
    expect(sent).toEqual([
      ["devbox", "bus-state", {}],
      ["devbox", "bus-inbox", { id: "alpha_cc" }],
    ]);
  });

  it("金样里登记的拒绝码逐码一句、两两不同（读不到 ≠ 一个都没有）", async () => {
    const codes = [...new Set([...golden.state.codes, ...golden.inbox.codes])];
    const said = new Set<string>();
    for (const code of codes) {
      invokeMock.mockReset();
      invokeMock.mockImplementation(async () => {
        throw refusedReply(code, `detail-${code}`);
      });
      const e = await readInbox("devbox", "alpha_cc").catch((x: unknown) => x);
      expect(e, code).toBeInstanceOf(ControlError);
      said.add((e as ControlError).message);
    }
    expect(said.size).toBe(codes.length);
  });
});
