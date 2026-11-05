/**
 * `src/frontend/ui/session-account-reads.ts` 的判据：一条会话的号问会话所在那台的 `session-account`，入参照类型发，回包原样交出。
 * 回包形状由握手时的形状指纹管，这里不逐格验。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { invoke } from "@tauri-apps/api/core";
import { readSessionAccount } from "../../../src/frontend/ui/session-account-reads";
import { chanArgsJson, chanReply, type ChanCallArgs } from "../../test-support/chan-fake";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;
const chanCalls = (): ChanCallArgs[] =>
  invokeMock.mock.calls.filter((c) => c[0] === "chan_call").map((c) => c[1] as ChanCallArgs);

beforeEach(() => {
  invokeMock.mockReset();
});

describe("session-account 问会话所在那台", () => {
  it("★ 远端 b 的会话 ⇒ 问 b、只带 sid；那台写好的那一份原样交出", async () => {
    const reply = {
      account: "bob",
      name: "bob",
      when: "now",
      from: null,
      accountLine: "账号 bob",
      fromLine: null,
      usage: null,
      stale: false,
      hover: [["bob"]],
      sampled: null,
      differs: false,
      badge: null,
    };
    invokeMock.mockImplementation(() => Promise.resolve(chanReply(reply)));
    expect(await readSessionAccount("b", "s1")).toEqual(reply);
    expect(chanCalls().map((a) => [a.origin, a.op, chanArgsJson(a)])).toEqual([["b", "session-account", { sid: "s1" }]]);
  });
});
