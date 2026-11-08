/**
 * `src/frontend/ui/history-reads.ts` 的判据：历史注解经通道问常驻后端，按形状收。
 *
 * 守的要求：「用户的历史注解一条不许丢」—— 注解的读写者是本机常驻后端，界面严格收它回的那一条。
 *
 * 判据：**问的是谁、带了什么**：改注解问会话所在那台；入参照类型发。
 * 删会话连带删注解在那台后端的 `files-delete-session` 里一并做完，界面不另问。
 * 回包的形状不在这里逐格验：typed 命令的形状由握手时的形状指纹管。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { invoke } from "@tauri-apps/api/core";
import { annotate } from "../../../src/frontend/ui/history-reads";
import { chanArgsJson, chanReply, type ChanCallArgs } from "../../test-support/chan-fake";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;
const chanCalls = (): ChanCallArgs[] =>
  invokeMock.mock.calls.filter((c) => c[0] === "chan_call").map((c) => c[1] as ChanCallArgs);

beforeEach(() => {
  invokeMock.mockReset();
});

describe("问的是谁、带了什么", () => {
  it("改注解问会话所在那台（注解跟着会话住在那台）；请求体逐键", async () => {
    invokeMock.mockImplementation(() =>
      Promise.resolve(chanReply({ entry: { starred: true, customTitle: null, hide: "on", updatedAt: 9 }, prev: null })),
    );
    expect(await annotate("dev", "s1", { customTitle: "" })).toEqual({
      entry: { starred: true, customTitle: null, hide: "on", updatedAt: 9 },
      prev: null,
    });
    expect(chanCalls().map((a) => [a.origin, a.op, chanArgsJson(a)])).toEqual([
      ["dev", "history-annotate", { sid: "s1", patch: { customTitle: "" } }],
    ]);
  });
});
