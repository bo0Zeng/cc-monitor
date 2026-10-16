/**
 * `src/frontend/ui/history-reads.ts` 的判据：历史注解与上次账号表经通道问常驻后端，按形状收。
 *
 * 守的要求：「用户的历史注解一条不许丢」—— 注解的读写者是本机常驻后端，界面严格收它回的那一条。
 *
 * 判据：**问的是谁、带了什么**：改注解 · 删注解 · 上次账号都问会话所在那台；入参照类型发。
 * 回包的形状不在这里逐格验：三条都是 typed 命令，形状由握手时的形状指纹管（第 8 批）。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { invoke } from "@tauri-apps/api/core";
import { annotate, forgetAnnotation, lastAccounts } from "../../../src/frontend/ui/history-reads";
import { chanArgsJson, chanReply, type ChanCallArgs } from "../../test-support/chan-fake";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;
const chanCalls = (): ChanCallArgs[] =>
  invokeMock.mock.calls.filter((c) => c[0] === "chan_call").map((c) => c[1] as ChanCallArgs);

beforeEach(() => {
  invokeMock.mockReset();
});

describe("问的是谁、带了什么", () => {
  it("改注解与上次账号都问会话所在那台（注解跟着会话住在那台）；请求体逐键", async () => {
    invokeMock.mockImplementation((_cmd: string, a: ChanCallArgs) =>
      Promise.resolve(
        a.op === "history-annotate"
          ? chanReply({ entry: { starred: true, customTitle: null, hide: "on", updatedAt: 9 }, prev: null })
          : a.op === "history-forget"
            ? chanReply({ removed: true })
            : chanReply({ accounts: { s1: "a" } }),
      ),
    );
    expect(await annotate("dev", "s1", { customTitle: "" })).toEqual({
      entry: { starred: true, customTitle: null, hide: "on", updatedAt: 9 },
      prev: null,
    });
    await forgetAnnotation("dev", "s1");
    expect(await lastAccounts("dev")).toEqual({ s1: "a" });
    expect(chanCalls().map((a) => [a.origin, a.op, chanArgsJson(a)])).toEqual([
      ["dev", "history-annotate", { sid: "s1", patch: { customTitle: "" } }],
      ["dev", "history-forget", { sid: "s1" }],
      ["dev", "history-last-accounts", {}],
    ]);
  });
});
